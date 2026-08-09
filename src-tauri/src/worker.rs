use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::models::WorkerResponse;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerRequest<'a> {
    audio: &'a Path,
    language: &'a str,
}

struct WorkerProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    worker_path: PathBuf,
    model_path: PathBuf,
    last_error: Arc<Mutex<String>>,
}

pub struct WorkerManager {
    process: Option<WorkerProcess>,
}

impl WorkerManager {
    pub fn new() -> Self {
        Self { process: None }
    }

    /// Starts model loading without waiting for the first dictation. The model
    /// remains resident in the worker until the app exits or the model changes.
    pub fn preload(&mut self, worker: &Path, model: &Path) -> Result<()> {
        self.ensure_started(worker, model)
    }

    pub fn transcribe(
        &mut self,
        worker: &Path,
        model: &Path,
        audio: &Path,
        language: &str,
    ) -> Result<WorkerResponse> {
        self.ensure_started(worker, model)?;
        match self.request(audio, language) {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => Err(anyhow!("Local transcription failed: {error}")),
            Err(first_error) => {
                // A worker crash must not close VoiceDictate. Replace it and
                // retry this recording once using the same temporary WAV.
                self.stop();
                self.ensure_started(worker, model)?;
                match self.request(audio, language) {
                    Ok(Ok(response)) => Ok(response),
                    Ok(Err(error)) => Err(anyhow!("Local transcription failed: {error}")),
                    Err(second_error) => Err(anyhow!(
                        "The local transcription worker stopped and could not recover: {second_error}. First error: {first_error}"
                    )),
                }
            }
        }
    }

    fn ensure_started(&mut self, worker: &Path, model: &Path) -> Result<()> {
        let reusable = if let Some(process) = self.process.as_mut() {
            process.worker_path == worker
                && process.model_path == model
                && process.child.try_wait()?.is_none()
        } else {
            false
        };
        if reusable {
            return Ok(());
        }

        self.stop();
        let mut command = Command::new(worker);
        command
            .arg("--model")
            .arg(model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }

        let mut child = command
            .spawn()
            .context("The local transcription worker could not start")?;
        let stdin = child
            .stdin
            .take()
            .context("The transcription worker input is unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("The transcription worker output is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("The transcription worker error output is unavailable")?;
        let last_error = Arc::new(Mutex::new(String::new()));
        let stderr_message = Arc::clone(&last_error);
        thread::Builder::new()
            .name("voicedictate-worker-errors".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Ok(mut message) = stderr_message.lock() {
                        *message = line.clone();
                    }
                    eprintln!("VoiceDictate transcription worker: {line}");
                }
            })
            .context("Could not monitor the transcription worker")?;

        self.process = Some(WorkerProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            worker_path: worker.to_path_buf(),
            model_path: model.to_path_buf(),
            last_error,
        });
        Ok(())
    }

    /// The outer result represents transport/process failure. The inner result
    /// represents a valid response containing either a transcript or a request
    /// error, which should not cause the healthy worker to restart.
    fn request(&mut self, audio: &Path, language: &str) -> Result<Result<WorkerResponse, String>> {
        let process = self
            .process
            .as_mut()
            .context("The transcription worker is not running")?;
        if let Some(status) = process.child.try_wait()? {
            return Err(anyhow!(
                "worker exited with {status}: {}",
                last_error(process)
            ));
        }

        let request = serde_json::to_string(&WorkerRequest { audio, language })?;
        process
            .stdin
            .write_all(request.as_bytes())
            .context("Could not send audio to the transcription worker")?;
        process
            .stdin
            .write_all(b"\n")
            .context("Could not finish the transcription request")?;
        process
            .stdin
            .flush()
            .context("Could not flush the transcription request")?;

        let mut line = String::new();
        let count = process
            .stdout
            .read_line(&mut line)
            .context("Could not read the transcription result")?;
        if count == 0 {
            return Err(anyhow!("worker closed its output: {}", last_error(process)));
        }
        let response: WorkerResponse = serde_json::from_str(&line)
            .context("The transcription worker returned an invalid response")?;
        if let Some(error) = response.error.clone() {
            Ok(Err(error))
        } else {
            Ok(Ok(response))
        }
    }

    fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}

impl Drop for WorkerManager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn last_error(process: &WorkerProcess) -> String {
    process
        .last_error
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default()
}
