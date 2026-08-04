use std::{path::PathBuf, time::Instant};

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Parser)]
struct Arguments {
    #[arg(long)] model: PathBuf,
    #[arg(long)] audio: PathBuf,
    #[arg(long, default_value = "auto")] language: String,
    #[arg(long)] threads: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response { text: String, inference_ms: u64, language: Option<String> }

fn main() {
    if let Err(error) = run() { eprintln!("{error:#}"); std::process::exit(1); }
}

fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let samples = read_wav(&arguments.audio)?;
    let started = Instant::now();
    let context = WhisperContext::new_with_params(arguments.model.to_string_lossy().as_ref(), WhisperContextParameters::default()).context("Could not load the Whisper model")?;
    let mut state = context.create_state().context("Could not create a Whisper inference state")?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(arguments.threads.unwrap_or_else(|| std::thread::available_parallelism().map(|value| value.get().min(4) as i32).unwrap_or(2)));
    params.set_translate(false);
    params.set_no_context(true);
    // Whisper's default encoder window is 30 seconds. Matching it to short
    // dictations avoids doing a full 30-second pass on older CPUs.
    let audio_ctx = (((samples.len() as f64 / 16_000.0) * 50.0).ceil() as i32 + 50).clamp(150, 1500);
    params.set_audio_ctx(audio_ctx);
    params.set_single_segment(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    if arguments.language != "auto" { params.set_language(Some(&arguments.language)); }
    state.full(params, &samples).context("Whisper inference failed")?;
    let count = state.full_n_segments().context("Could not read Whisper segments")?;
    let mut text = String::new();
    for index in 0..count { text.push_str(&state.full_get_segment_text(index).context("Could not read a Whisper segment")?); }
    let language = state.full_lang_id_from_state().ok().and_then(|id| whisper_rs::get_lang_str(id).map(str::to_string));
    println!("{}", serde_json::to_string(&Response { text: text.trim().to_string(), inference_ms: started.elapsed().as_millis() as u64, language })?);
    Ok(())
}

fn read_wav(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("Could not open the recorded audio")?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 { bail!("The worker requires 16 kHz mono audio"); }
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => reader.samples::<i16>().map(|sample| sample.map(|value| value as f32 / i16::MAX as f32)).collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
    };
    Ok(samples)
}
