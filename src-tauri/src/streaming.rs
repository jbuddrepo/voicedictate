//! Conservative incremental recognition boundaries for 16 kHz mono audio.
//!
//! Unlike a live captioning loop, this planner does not repeatedly recognize a
//! growing prefix or commit provisional words. It proposes disjoint chunks only
//! after a sustained low-energy pause, with padding on both sides. A caller
//! commits a boundary only after transcription succeeds. Continuous speech is
//! deliberately left for final recognition rather than cut at an arbitrary time.

pub const SAMPLE_RATE: usize = 16_000;
const FRAME: usize = SAMPLE_RATE / 50; // 20 ms
const MIN_CONTEXT: usize = SAMPLE_RATE * 3;
const PAUSE: usize = SAMPLE_RATE * 3 / 5; // 600 ms
const PAD: usize = PAUSE / 2;
const MIN_VOICE_FRAMES: usize = 10; // Reject isolated clicks/transients.
const SILENCE_RMS: f32 = 0.002;
const SILENCE_PEAK: f32 = 0.01;
const VOICE_RMS: f32 = 0.004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioChunk {
    /// Inclusive sample offset in the full recording (16 kHz mono).
    pub start: usize,
    /// Exclusive sample offset in the full recording (16 kHz mono).
    pub end: usize,
}

#[derive(Default, Debug)]
pub struct ChunkPlanner {
    committed: usize,
}

impl ChunkPlanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn committed_samples(&self) -> usize {
        self.committed
    }

    /// Return the first stable pause after enough context. Repeating this call
    /// without `commit` returns the same proposal; failed work loses no audio.
    pub fn next_chunk(&self, samples: &[f32]) -> Option<AudioChunk> {
        let suffix = samples.get(self.committed..)?;
        if suffix.len() < MIN_CONTEXT + PAD {
            return None;
        }
        let mut silence_start = None;
        let mut voice_frames = 0;
        for (index, frame) in suffix.chunks_exact(FRAME).enumerate() {
            let frame_start = index * FRAME;
            let frame_end = frame_start + FRAME;
            let rms = (frame.iter().map(|v| v * v).sum::<f32>() / FRAME as f32).sqrt();
            let peak = frame.iter().fold(0.0_f32, |peak, v| peak.max(v.abs()));
            if rms > VOICE_RMS {
                voice_frames += 1;
            }
            if rms <= SILENCE_RMS && peak <= SILENCE_PEAK {
                let start = *silence_start.get_or_insert(frame_start);
                // A short utterance followed by a long pause can still supply
                // the model's minimum context, entirely within confirmed silence.
                let boundary = (start + PAD).max(MIN_CONTEXT);
                if frame_end - start >= PAUSE
                    && frame_end >= boundary + PAD
                    && voice_frames >= MIN_VOICE_FRAMES
                {
                    return Some(AudioChunk {
                        start: self.committed,
                        end: self.committed + boundary,
                    });
                }
            } else {
                silence_start = None;
            }
        }
        None
    }

    /// Acknowledge only successfully decoded, contiguous audio. This refuses a
    /// stale or unrelated proposal instead of advancing across unheard samples.
    pub fn commit(&mut self, chunk: AudioChunk) -> bool {
        if chunk.start != self.committed || chunk.end <= chunk.start {
            return false;
        }
        self.committed = chunk.end;
        true
    }

    /// Finalize every remaining sample, including quiet speech that did not
    /// satisfy conservative live detection. An all-but-silent tail below the
    /// existing recorder's 0.002 peak threshold need not be sent to Whisper.
    /// Call `commit` after successful final decoding just as for live chunks.
    pub fn final_chunk(&self, samples: &[f32]) -> Option<AudioChunk> {
        let suffix = samples.get(self.committed..)?;
        if suffix.is_empty() || !suffix.iter().any(|value| value.abs() >= 0.002) {
            return None;
        }
        Some(AudioChunk {
            start: self.committed,
            end: samples.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech(seconds: f32) -> Vec<f32> {
        (0..(seconds * SAMPLE_RATE as f32) as usize)
            .map(|n| (n as f32 * 0.04).sin() * 0.1)
            .collect()
    }

    fn pause(audio: &mut Vec<f32>, seconds: f32) {
        audio.extend(vec![0.0; (seconds * SAMPLE_RATE as f32) as usize]);
    }

    #[test]
    fn does_not_decode_empty_silence_short_speech_or_isolated_clicks_live() {
        let planner = ChunkPlanner::new();
        assert_eq!(planner.next_chunk(&[]), None);
        assert_eq!(planner.next_chunk(&vec![0.0; SAMPLE_RATE * 10]), None);
        let mut short = speech(1.0);
        pause(&mut short, 0.7);
        assert_eq!(planner.next_chunk(&short), None);
        let mut click = vec![0.0; SAMPLE_RATE * 5];
        click[0] = 0.5;
        assert_eq!(planner.next_chunk(&click), None);
    }

    #[test]
    fn waits_for_stable_pause_even_when_snapshot_ends_mid_pause() {
        let planner = ChunkPlanner::new();
        let mut audio = speech(4.0);
        pause(&mut audio, 0.4);
        assert_eq!(planner.next_chunk(&audio), None);
        pause(&mut audio, 0.2);
        assert_eq!(
            planner.next_chunk(&audio),
            Some(AudioChunk {
                start: 0,
                end: SAMPLE_RATE * 43 / 10
            })
        );
    }

    #[test]
    fn short_utterance_can_complete_with_sufficient_silent_context() {
        let mut audio = speech(1.0);
        pause(&mut audio, 2.4);
        assert_eq!(
            ChunkPlanner::new().next_chunk(&audio),
            Some(AudioChunk {
                start: 0,
                end: MIN_CONTEXT
            })
        );
    }

    #[test]
    fn does_not_cut_long_continuous_speech_to_force_a_time_limit() {
        assert_eq!(ChunkPlanner::new().next_chunk(&speech(20.0)), None);
    }

    #[test]
    fn failed_or_uncommitted_decode_keeps_proposal_and_final_audio() {
        let planner = ChunkPlanner::new();
        let mut audio = speech(4.0);
        pause(&mut audio, 0.6);
        let proposal = planner.next_chunk(&audio).unwrap();
        assert_eq!(planner.next_chunk(&audio), Some(proposal));
        assert_eq!(planner.committed_samples(), 0);
        assert_eq!(
            planner.final_chunk(&audio),
            Some(AudioChunk {
                start: 0,
                end: audio.len()
            })
        );
    }

    #[test]
    fn committed_chunks_and_final_suffix_cover_each_spoken_sample_once() {
        let mut planner = ChunkPlanner::new();
        let mut audio = speech(4.0);
        pause(&mut audio, 0.6);
        let first = planner.next_chunk(&audio).unwrap();
        assert!(planner.commit(first));
        assert_eq!(planner.next_chunk(&audio), None);
        assert_eq!(planner.final_chunk(&audio), None); // Only 300 ms of silence remains.
        audio.extend(speech(4.0));
        pause(&mut audio, 0.6);
        let second = planner.next_chunk(&audio).unwrap();
        assert_eq!(second.start, first.end);
        assert!(planner.commit(second));
        audio.extend(speech(0.4));
        let last = planner.final_chunk(&audio).unwrap();
        assert_eq!(last.start, second.end);
        assert_eq!(last.end, audio.len());
        assert!(planner.commit(last));
        assert_eq!(planner.final_chunk(&audio), None);
        assert!(!planner.commit(first)); // Refuse stale completion.
    }

    #[test]
    fn background_noise_does_not_qualify_as_a_safe_pause() {
        let mut audio = speech(4.0);
        audio.extend(vec![0.003; SAMPLE_RATE]);
        assert_eq!(ChunkPlanner::new().next_chunk(&audio), None);
        assert!(ChunkPlanner::new().final_chunk(&audio).is_some());
    }

    #[test]
    fn quiet_final_speech_is_preserved_even_if_live_voice_threshold_not_met() {
        let audio = vec![0.003; SAMPLE_RATE];
        assert_eq!(ChunkPlanner::new().next_chunk(&audio), None);
        assert_eq!(
            ChunkPlanner::new().final_chunk(&audio),
            Some(AudioChunk {
                start: 0,
                end: SAMPLE_RATE
            })
        );
    }

    #[test]
    fn partial_frame_never_moves_live_boundary_into_unconfirmed_audio() {
        let mut audio = speech(4.0);
        pause(&mut audio, 0.6);
        audio.extend(vec![0.5; FRAME / 2]);
        let chunk = ChunkPlanner::new().next_chunk(&audio).unwrap();
        assert!(chunk.end < SAMPLE_RATE * 46 / 10);
        assert!(chunk.end <= audio.len());
    }
}
