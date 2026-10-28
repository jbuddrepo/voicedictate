//! Pause-delimited recognition overlaps microphone capture. It never pastes partial text.
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    audio::{write_wav, AudioController, RecordingMetadata, RecordingSnapshot},
    models::AppSettings,
    streaming::ChunkPlanner,
    transcription,
    worker::WorkerManager,
};
use anyhow::{anyhow, Result};

// The planner requires 3 seconds of context plus 300 ms of confirmed silence.
// It remains the authority on whether a passage actually has a safe boundary.
const MIN_INCREMENTAL_SAMPLES: usize = 16_000 * 33 / 10;

/// Small recognition seam allows synthetic regression tests without a model,
/// microphone or transcription service. Production still uses `Recognizer`.
pub trait ChunkRecognizer {
    fn recognize(&mut self, samples: &[f32]) -> Result<String>;
}

impl<F> ChunkRecognizer for F
where
    F: FnMut(&[f32]) -> Result<String>,
{
    fn recognize(&mut self, samples: &[f32]) -> Result<String> {
        self(samples)
    }
}

pub struct Recognizer {
    settings: AppSettings,
    worker: Arc<Mutex<WorkerManager>>,
    local_paths: Option<(PathBuf, PathBuf)>,
    remote_key: Option<String>,
    temp_dir: PathBuf,
    local_fallback: bool,
    pub source: String,
    pub warning: Option<String>,
    pub inference_ms: u64,
    pub threads: usize,
}

impl Recognizer {
    pub fn new(
        settings: AppSettings,
        worker: Arc<Mutex<WorkerManager>>,
        local_paths: Option<(PathBuf, PathBuf)>,
        remote_key: Option<String>,
        temp_dir: PathBuf,
    ) -> Self {
        Self {
            settings,
            worker,
            local_paths,
            remote_key,
            temp_dir,
            local_fallback: false,
            source: String::new(),
            warning: None,
            inference_ms: 0,
            threads: 0,
        }
    }

    fn local(&mut self, wav: &std::path::Path) -> Result<String> {
        let initial_prompt = crate::vocabulary::hint_prompt(&self.settings.personal_vocabulary);
        let (worker, model) = self
            .local_paths
            .as_ref()
            .ok_or_else(|| anyhow!("The local Whisper worker or model is missing"))?;
        let response = self
            .worker
            .lock()
            .map_err(|_| anyhow!("The transcription worker is unavailable"))?
            .transcribe(worker, model, wav, &self.settings.language, &initial_prompt)?;
        self.inference_ms += response.inference_ms;
        self.threads = response.threads;
        Ok(response.text.trim().to_string())
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let audio =
            crate::TemporaryAudio(self.temp_dir.join(format!("{}.wav", uuid::Uuid::new_v4())));
        write_wav(&audio.0, samples)?;
        let (text, source) = if self.settings.transcription_mode == "local" || self.local_fallback {
            (
                self.local(&audio.0)?,
                format!("local:{}", self.settings.model),
            )
        } else {
            match transcription::transcribe(&self.settings, self.remote_key.as_deref(), &audio.0) {
                Ok(text) => (
                    text,
                    format!("remote:{}", self.settings.transcription_provider),
                ),
                Err(error) if self.settings.transcription_mode == "automatic" => {
                    self.local_fallback = true;
                    self.warning = Some(format!(
                        "Remote transcription failed; local Whisper was used: {error}"
                    ));
                    (
                        self.local(&audio.0)?,
                        format!("local:{}", self.settings.model),
                    )
                }
                Err(error) => return Err(error),
            }
        };
        if self.source.is_empty() {
            self.source = source;
        } else if self.source != source {
            self.source = "mixed:remote+local".into();
        }
        Ok(text)
    }
}

impl ChunkRecognizer for Recognizer {
    fn recognize(&mut self, samples: &[f32]) -> Result<String> {
        self.transcribe(samples)
    }
}

pub struct Progress<R = Recognizer> {
    pub recognizer: R,
    pub planner: ChunkPlanner,
    pub parts: Vec<String>,
    pub chunks: usize,
}

impl<R: ChunkRecognizer> Progress<R> {
    /// Commit audio only when recognition succeeds. On failure the final pass still has it.
    pub fn advance(&mut self, samples: &[f32]) -> Result<bool> {
        let Some(chunk) = self.planner.next_chunk(samples) else {
            return Ok(false);
        };
        let text = self
            .recognizer
            .recognize(&samples[chunk.start..chunk.end])?;
        if text.trim().is_empty() {
            return Err(anyhow!("No text returned for an incremental passage"));
        }
        if !self.planner.commit(chunk) {
            return Err(anyhow!("Incremental audio cursor changed unexpectedly"));
        }
        self.parts.push(text.trim().to_owned());
        self.chunks += 1;
        Ok(true)
    }

    pub fn finish(&mut self, samples: &[f32]) -> Result<String> {
        if samples.len() < self.planner.committed_samples() {
            return Err(anyhow!(
                "Final recording is shorter than the recognized audio"
            ));
        }
        if let Some(chunk) = self.planner.final_chunk(samples) {
            let text = self
                .recognizer
                .recognize(&samples[chunk.start..chunk.end])?;
            // Do not silently discard an audible final passage. On error or an
            // empty response, its cursor and the successful prefix remain intact.
            if text.trim().is_empty() {
                return Err(anyhow!("No text returned for the final passage"));
            }
            if !self.planner.commit(chunk) {
                return Err(anyhow!("Final audio cursor changed unexpectedly"));
            }
            self.parts.push(text.trim().to_owned());
        }
        let text = self.parts.join(" ");
        if text.trim().is_empty() {
            return Err(anyhow!("No speech was detected"));
        }
        Ok(text)
    }
}

pub struct Session {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<Progress>>,
    pub settings: AppSettings,
}

impl Session {
    pub fn start(
        settings: AppSettings,
        recorder: Arc<Mutex<AudioController>>,
        recognizer: Recognizer,
    ) -> Result<Self> {
        let recording_id = recorder
            .lock()
            .map_err(|_| anyhow!("The recorder is unavailable"))?
            .metadata()?
            .ok_or_else(|| anyhow!("No recording is active"))?
            .recording_id;
        let stop = Arc::new(AtomicBool::new(false));
        let metadata_recorder = Arc::clone(&recorder);
        let task = spawn_progress_loop(
            Arc::clone(&stop),
            recording_id,
            move || {
                metadata_recorder
                    .lock()
                    .ok()
                    .and_then(|recorder| recorder.metadata().ok())
                    .flatten()
            },
            move || {
                recorder
                    .lock()
                    .ok()
                    .and_then(|recorder| recorder.snapshot().ok())
                    .flatten()
            },
            recognizer,
            Duration::from_millis(800),
        )?;
        Ok(Self {
            stop,
            task: Some(task),
            settings,
        })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = &self.task {
            task.thread().unpark();
        }
    }

    pub fn finish(mut self, samples: &[f32]) -> Result<(String, Progress)> {
        self.stop();
        let mut progress = self
            .task
            .take()
            .ok_or_else(|| anyhow!("Dictation session is unavailable"))?
            .join()
            .map_err(|_| anyhow!("Incremental transcription stopped unexpectedly"))?;
        let text = progress.finish(samples)?;
        Ok((text, progress))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A cancelled in-flight recognizer may finish its existing immutable audio
/// copy, but can never acquire or recognize a subsequent recording. It cannot
/// paste: only `Session::finish` returns text to the finalization path.
fn spawn_progress_loop<R, M, S>(
    stop: Arc<AtomicBool>,
    recording_id: u64,
    mut metadata: M,
    mut snapshot: S,
    recognizer: R,
    poll_interval: Duration,
) -> Result<JoinHandle<Progress<R>>>
where
    R: ChunkRecognizer + Send + 'static,
    M: FnMut() -> Option<RecordingMetadata> + Send + 'static,
    S: FnMut() -> Option<RecordingSnapshot> + Send + 'static,
{
    Ok(thread::Builder::new()
        .name("voicedictate-incremental".into())
        .spawn(move || {
            let mut progress = Progress {
                recognizer,
                planner: ChunkPlanner::new(),
                parts: Vec::new(),
                chunks: 0,
            };
            while !stop.load(Ordering::Acquire) {
                // Unparking on release/cancel avoids adding a full poll interval.
                thread::park_timeout(poll_interval);
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let Some(metadata) = metadata() else { break };
                if stop.load(Ordering::Acquire) || metadata.recording_id != recording_id {
                    break;
                }
                // Polling a count is O(1). Avoid copying/resampling the growing
                // recording when there cannot yet be an eligible chunk.
                if metadata
                    .sample_count_16k
                    .saturating_sub(progress.planner.committed_samples())
                    < MIN_INCREMENTAL_SAMPLES
                {
                    continue;
                }
                let Some(snapshot) = snapshot() else { break };
                if stop.load(Ordering::Acquire) || snapshot.recording_id != recording_id {
                    break;
                }
                // A failed pass leaves its audio uncommitted for final recognition.
                // Avoid repeatedly calling an unavailable worker/server while recording.
                if progress.advance(&snapshot.samples).is_err() {
                    break;
                }
            }
            progress
        })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::SAMPLE_RATE;
    use std::sync::{atomic::AtomicUsize, mpsc};

    fn progress<R: ChunkRecognizer>(recognizer: R) -> Progress<R> {
        Progress {
            recognizer,
            planner: ChunkPlanner::new(),
            parts: Vec::new(),
            chunks: 0,
        }
    }

    fn speech(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * SAMPLE_RATE as f32) as usize)
            .map(|index| (index as f32 * 0.04).sin() * amplitude)
            .collect()
    }

    fn completed_passage() -> Vec<f32> {
        let mut audio = speech(4.0, 0.1);
        audio.extend(vec![0.0; SAMPLE_RATE * 3 / 5]);
        audio
    }

    fn snapshot(recording_id: u64, samples: Vec<f32>) -> RecordingSnapshot {
        RecordingSnapshot {
            recording_id,
            samples,
            duration_ms: 5_000,
            target_window: 0,
        }
    }

    fn eligible_metadata() -> Option<RecordingMetadata> {
        Some(RecordingMetadata {
            recording_id: 1,
            sample_count_16k: SAMPLE_RATE * 5,
        })
    }

    #[test]
    fn prefix_and_final_suffix_are_recognized_once_without_duplicate_words() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observations = Arc::clone(&observed);
        let mut pipeline = progress(move |samples: &[f32]| {
            let mut observations = observations.lock().unwrap();
            observations.push(samples.to_vec());
            Ok(if observations.len() == 1 {
                "  First sentence.  "
            } else {
                "Last words."
            }
            .into())
        });
        let mut audio = completed_passage();
        assert!(pipeline.advance(&audio).unwrap());
        let prefix_end = pipeline.planner.committed_samples();
        assert!(!pipeline.advance(&audio).unwrap());
        audio.extend(speech(0.4, 0.2));
        assert_eq!(
            pipeline.finish(&audio).unwrap(),
            "First sentence. Last words."
        );
        // Idempotent finalization must not re-decode or append the tail again.
        assert_eq!(
            pipeline.finish(&audio).unwrap(),
            "First sentence. Last words."
        );
        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0], audio[..prefix_end]);
        assert_eq!(observed[1], audio[prefix_end..]);
        assert_eq!(pipeline.planner.committed_samples(), audio.len());
        assert_eq!(pipeline.chunks, 1);
    }

    #[test]
    fn failed_live_recognition_keeps_all_audio_for_final_pass() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_recognizer = Arc::clone(&calls);
        let mut pipeline = progress(move |samples: &[f32]| {
            let mut calls = calls_for_recognizer.lock().unwrap();
            calls.push(samples.len());
            if calls.len() == 1 {
                Err(anyhow!("Synthetic worker failure"))
            } else {
                Ok("Recovered full recording.".into())
            }
        });
        let mut audio = completed_passage();
        assert!(pipeline.advance(&audio).is_err());
        assert_eq!(pipeline.planner.committed_samples(), 0);
        assert!(pipeline.parts.is_empty());
        audio.extend(speech(0.3, 0.2));
        assert_eq!(
            pipeline.finish(&audio).unwrap(),
            "Recovered full recording."
        );
        assert_eq!(calls.lock().unwrap()[1], audio.len());
    }

    #[test]
    fn empty_live_response_does_not_advance_the_cursor() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed_attempts = Arc::clone(&attempts);
        let mut pipeline = progress(move |_: &[f32]| {
            Ok(if observed_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                "   "
            } else {
                "Complete transcript."
            }
            .into())
        });
        let audio = completed_passage();
        assert!(pipeline.advance(&audio).is_err());
        assert_eq!(pipeline.planner.committed_samples(), 0);
        assert_eq!(pipeline.finish(&audio).unwrap(), "Complete transcript.");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn empty_final_response_preserves_prefix_and_tail_for_retry() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed_attempts = Arc::clone(&attempts);
        let mut pipeline = progress(move |_: &[f32]| {
            Ok(match observed_attempts.fetch_add(1, Ordering::SeqCst) {
                0 => "First sentence.",
                1 => " ",
                _ => "Quiet last word.",
            }
            .into())
        });
        let mut audio = completed_passage();
        pipeline.advance(&audio).unwrap();
        let prefix_end = pipeline.planner.committed_samples();
        audio.extend(speech(0.08, 0.003));
        assert!(pipeline.finish(&audio).is_err());
        assert_eq!(pipeline.parts, vec!["First sentence."]);
        assert_eq!(pipeline.planner.committed_samples(), prefix_end);
        assert_eq!(
            pipeline.finish(&audio).unwrap(),
            "First sentence. Quiet last word."
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn trailing_silence_returns_committed_text_without_another_decode() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed_calls = Arc::clone(&calls);
        let mut pipeline = progress(move |_: &[f32]| {
            observed_calls.fetch_add(1, Ordering::SeqCst);
            Ok("Finished thought.".into())
        });
        let audio = completed_passage();
        pipeline.advance(&audio).unwrap();
        assert_eq!(pipeline.finish(&audio).unwrap(), "Finished thought.");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn silence_never_invokes_recognizer_and_reports_no_speech() {
        let mut pipeline = progress(|_: &[f32]| -> Result<String> {
            panic!("silence must not be sent for recognition")
        });
        assert!(!pipeline.advance(&vec![0.0; SAMPLE_RATE * 5]).unwrap());
        assert_eq!(
            pipeline
                .finish(&vec![0.0; SAMPLE_RATE * 5])
                .unwrap_err()
                .to_string(),
            "No speech was detected"
        );
        assert!(pipeline.finish(&[]).is_err());
    }

    #[test]
    fn rejects_shorter_final_recording_instead_of_returning_wrong_prefix() {
        let mut pipeline = progress(|_: &[f32]| Ok("Prefix.".into()));
        pipeline.advance(&completed_passage()).unwrap();
        assert!(pipeline.finish(&speech(0.5, 0.1)).is_err());
    }

    #[test]
    fn cancellation_before_first_poll_does_not_snapshot_or_finalize() {
        let stop = Arc::new(AtomicBool::new(true));
        let task = spawn_progress_loop(
            stop,
            1,
            || -> Option<RecordingMetadata> { panic!("cancelled task must not poll metadata") },
            || -> Option<RecordingSnapshot> { panic!("cancelled task must not snapshot") },
            |_: &[f32]| -> Result<String> { panic!("cancelled task must not recognize") },
            Duration::from_millis(1),
        )
        .unwrap();
        let result = task.join().unwrap();
        assert!(result.parts.is_empty());
        assert_eq!(result.chunks, 0);
    }

    #[test]
    fn cancellation_during_snapshot_does_not_recognize_that_audio() {
        let stop = Arc::new(AtomicBool::new(false));
        let cancel_during_snapshot = Arc::clone(&stop);
        let task = spawn_progress_loop(
            stop,
            1,
            eligible_metadata,
            move || {
                cancel_during_snapshot.store(true, Ordering::Release);
                Some(snapshot(1, completed_passage()))
            },
            |_: &[f32]| -> Result<String> { panic!("cancelled snapshot must not recognize") },
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(task.join().unwrap().parts.is_empty());
    }

    #[test]
    fn old_session_never_recognizes_a_new_recording() {
        let task = spawn_progress_loop(
            Arc::new(AtomicBool::new(false)),
            1,
            eligible_metadata,
            || Some(snapshot(2, completed_passage())),
            |_: &[f32]| -> Result<String> { panic!("new recording belongs to another session") },
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(task.join().unwrap().parts.is_empty());
    }

    #[test]
    fn metadata_gate_skips_copies_before_threshold_and_after_a_commit() {
        let polls = Arc::new(AtomicUsize::new(0));
        let observed_polls = Arc::clone(&polls);
        let snapshots = Arc::new(AtomicUsize::new(0));
        let observed_snapshots = Arc::clone(&snapshots);
        let task = spawn_progress_loop(
            Arc::new(AtomicBool::new(false)),
            1,
            move || {
                let poll = observed_polls.fetch_add(1, Ordering::SeqCst);
                match poll {
                    0 => Some(RecordingMetadata {
                        recording_id: 1,
                        sample_count_16k: MIN_INCREMENTAL_SAMPLES - 1,
                    }),
                    1 | 2 => Some(RecordingMetadata {
                        recording_id: 1,
                        sample_count_16k: MIN_INCREMENTAL_SAMPLES,
                    }),
                    _ => None,
                }
            },
            move || {
                observed_snapshots.fetch_add(1, Ordering::SeqCst);
                let mut audio = speech(1.0, 0.1);
                audio.resize(MIN_INCREMENTAL_SAMPLES, 0.0);
                Some(snapshot(1, audio))
            },
            |_: &[f32]| Ok("Short complete utterance.".into()),
            Duration::from_millis(1),
        )
        .unwrap();
        let result = task.join().unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 4);
        assert_eq!(snapshots.load(Ordering::SeqCst), 1);
        assert_eq!(result.chunks, 1);
        assert_eq!(result.parts, vec!["Short complete utterance."]);
    }

    #[test]
    fn changed_metadata_generation_does_not_copy_or_recognize_new_recording() {
        let task = spawn_progress_loop(
            Arc::new(AtomicBool::new(false)),
            1,
            || {
                Some(RecordingMetadata {
                    recording_id: 2,
                    sample_count_16k: SAMPLE_RATE * 5,
                })
            },
            || -> Option<RecordingSnapshot> { panic!("new metadata must not lead to a snapshot") },
            |_: &[f32]| -> Result<String> { panic!("new recording must not be recognized") },
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(task.join().unwrap().parts.is_empty());
    }

    #[test]
    fn cancellation_during_metadata_does_not_copy_audio() {
        let stop = Arc::new(AtomicBool::new(false));
        let cancel_during_metadata = Arc::clone(&stop);
        let task = spawn_progress_loop(
            stop,
            1,
            move || {
                cancel_during_metadata.store(true, Ordering::Release);
                eligible_metadata()
            },
            || -> Option<RecordingSnapshot> {
                panic!("cancelled metadata must not lead to a snapshot")
            },
            |_: &[f32]| -> Result<String> { panic!("cancelled recording must not be recognized") },
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(task.join().unwrap().parts.is_empty());
    }

    #[test]
    fn cancellation_during_decode_does_not_poll_or_finalize_again() {
        let stop = Arc::new(AtomicBool::new(false));
        let (started, recognition_started) = mpsc::channel();
        let (resume, recognition_resume) = mpsc::channel();
        let polls = Arc::new(AtomicUsize::new(0));
        let observed_polls = Arc::clone(&polls);
        let task = spawn_progress_loop(
            Arc::clone(&stop),
            1,
            eligible_metadata,
            move || {
                observed_polls.fetch_add(1, Ordering::SeqCst);
                Some(snapshot(1, completed_passage()))
            },
            move |_: &[f32]| {
                started.send(()).unwrap();
                recognition_resume
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                Ok("Old immutable audio only.".into())
            },
            Duration::from_millis(1),
        )
        .unwrap();
        recognition_started
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        stop.store(true, Ordering::Release);
        task.thread().unpark();
        resume.send(()).unwrap();
        let result = task.join().unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert_eq!(result.chunks, 1); // Only the in-flight old snapshot completed.
        assert_eq!(result.parts, vec!["Old immutable audio only."]);
    }
}
