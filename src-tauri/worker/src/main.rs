use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    threads: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    audio: PathBuf,
    language: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    text: String,
    inference_ms: u64,
    threads: i32,
    language: Option<String>,
    error: Option<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let threads = select_threads(
        arguments.threads,
        std::thread::available_parallelism()
            .map(|value| value.get())
            .ok(),
    )?;
    let context = WhisperContext::new_with_params(
        arguments.model.to_string_lossy().as_ref(),
        WhisperContextParameters::default(),
    )
    .context("Could not load the Whisper model")?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) => match serde_json::from_str::<Request>(&line) {
                Ok(request) => {
                    match transcribe(&context, &request.audio, &request.language, threads) {
                        Ok(response) => response,
                        Err(error) => Response {
                            text: String::new(),
                            inference_ms: 0,
                            threads,
                            language: None,
                            error: Some(format!("{error:#}")),
                        },
                    }
                }
                Err(error) => Response {
                    text: String::new(),
                    inference_ms: 0,
                    threads,
                    language: None,
                    error: Some(format!("Invalid worker request: {error}")),
                },
            },
            Err(error) => return Err(error.into()),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn select_threads(requested: Option<i32>, available: Option<usize>) -> Result<i32> {
    match requested {
        Some(value) if value <= 0 => bail!("--threads must be a positive number"),
        Some(value) => Ok(value),
        // Leave one logical CPU for recording, the UI and the rest of Windows.
        // More than eight threads can hurt short CPU dictations through overhead.
        None => Ok(available
            .map(|value| value.saturating_sub(1).clamp(1, 8) as i32)
            .unwrap_or(2)),
    }
}

fn transcribe(
    context: &WhisperContext,
    audio: &Path,
    language: &str,
    threads: i32,
) -> Result<Response> {
    let samples = read_wav(audio)?;
    let started = Instant::now();
    let mut state = context
        .create_state()
        .context("Could not create a Whisper inference state")?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads);
    params.set_translate(false);
    params.set_no_context(true);
    // Whisper's default encoder window is 30 seconds. Matching it to short
    // dictations avoids doing a full 30-second pass on older CPUs.
    let audio_ctx =
        (((samples.len() as f64 / 16_000.0) * 50.0).ceil() as i32 + 50).clamp(150, 1500);
    params.set_audio_ctx(audio_ctx);
    params.set_single_segment(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    if language != "auto" {
        params.set_language(Some(language));
    }
    state
        .full(params, &samples)
        .context("Whisper inference failed")?;
    let count = state
        .full_n_segments()
        .context("Could not read Whisper segments")?;
    let mut text = String::new();
    for index in 0..count {
        text.push_str(
            &state
                .full_get_segment_text(index)
                .context("Could not read a Whisper segment")?,
        );
    }
    let language = state
        .full_lang_id_from_state()
        .ok()
        .and_then(|id| whisper_rs::get_lang_str(id).map(str::to_string));
    Ok(Response {
        text: text.trim().to_string(),
        inference_ms: started.elapsed().as_millis() as u64,
        threads,
        language,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_threads_reserve_one_cpu_and_cap_at_eight() {
        for (available, expected) in [(1, 1), (2, 1), (4, 3), (8, 7), (12, 8), (32, 8)] {
            assert_eq!(select_threads(None, Some(available)).unwrap(), expected);
        }
        assert_eq!(select_threads(None, None).unwrap(), 2);
    }

    #[test]
    fn explicit_thread_count_is_honored_and_must_be_positive() {
        assert_eq!(select_threads(Some(1), Some(32)).unwrap(), 1);
        assert_eq!(select_threads(Some(16), Some(32)).unwrap(), 16);
        assert!(select_threads(Some(0), Some(32)).is_err());
        assert!(select_threads(Some(-1), Some(32)).is_err());
    }

    #[test]
    fn response_reports_thread_count_and_camel_case_timing() {
        let response = Response {
            text: String::new(),
            inference_ms: 42,
            threads: 8,
            language: None,
            error: None,
        };
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["threads"], 8);
        assert_eq!(json["inferenceMs"], 42);
    }
}

fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("Could not open the recorded audio")?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 {
        bail!("The worker requires 16 kHz mono audio");
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| value as f32 / i16::MAX as f32))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
    };
    Ok(samples)
}
