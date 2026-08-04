use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, Device, SampleFormat, Stream, StreamConfig};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

pub struct ActiveRecording {
    pub stream: Stream,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub started: Instant,
    pub target_window: isize,
    pub error: Arc<Mutex<Option<String>>>,
}

pub struct CompletedRecording {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
    pub target_window: isize,
}

struct AudioRecorder {
    pub active: Option<ActiveRecording>,
}

enum RecorderRequest {
    Start(Option<String>, mpsc::Sender<Result<(), String>>),
    Stop(mpsc::Sender<Result<CompletedRecording, String>>),
    Cancel,
}

pub struct AudioController {
    requests: mpsc::Sender<RecorderRequest>,
}

impl AudioController {
    pub fn new() -> Self {
        let (requests, receiver) = mpsc::channel();
        thread::Builder::new().name("voicedictate-audio".into()).spawn(move || {
            let mut recorder = AudioRecorder::new();
            while let Ok(request) = receiver.recv() {
                match request {
                    RecorderRequest::Start(name, reply) => { let _ = reply.send(recorder.start(name.as_deref()).map_err(|error| error.to_string())); }
                    RecorderRequest::Stop(reply) => { let _ = reply.send(recorder.stop().map_err(|error| error.to_string())); }
                    RecorderRequest::Cancel => recorder.cancel(),
                }
            }
        }).expect("Could not start the microphone controller");
        Self { requests }
    }

    pub fn list_devices() -> Result<Vec<String>> { AudioRecorder::list_devices() }

    pub fn start(&self, preferred_name: Option<&str>) -> Result<()> {
        let (reply, response) = mpsc::channel();
        self.requests.send(RecorderRequest::Start(preferred_name.map(str::to_string), reply)).map_err(|_| anyhow!("The microphone controller stopped"))?;
        response.recv().map_err(|_| anyhow!("The microphone controller did not respond"))?.map_err(anyhow::Error::msg)
    }

    pub fn stop(&self) -> Result<CompletedRecording> {
        let (reply, response) = mpsc::channel();
        self.requests.send(RecorderRequest::Stop(reply)).map_err(|_| anyhow!("The microphone controller stopped"))?;
        response.recv().map_err(|_| anyhow!("The microphone controller did not respond"))?.map_err(anyhow::Error::msg)
    }

    pub fn cancel(&self) { let _ = self.requests.send(RecorderRequest::Cancel); }
}

impl AudioRecorder {
    pub fn new() -> Self { Self { active: None } }

    pub fn list_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        Ok(host.input_devices()?.filter_map(|device| device.name().ok()).collect())
    }

    pub fn start(&mut self, preferred_name: Option<&str>) -> Result<()> {
        if self.active.is_some() { return Err(anyhow!("Recording is already active")); }
        let host = cpal::default_host();
        let device = choose_device(&host, preferred_name)?;
        let supported = device.default_input_config().context("The selected microphone has no supported input format")?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let config: StreamConfig = supported.clone().into();
        let samples = Arc::new(Mutex::new(Vec::with_capacity(sample_rate as usize * 30)));
        let target = Arc::clone(&samples);
        let stream_error = Arc::new(Mutex::new(None));
        let callback_error = Arc::clone(&stream_error);
        let error_callback = move |error: cpal::StreamError| {
            if let Ok(mut value) = callback_error.lock() { *value = Some(error.to_string()); }
        };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(&config, move |data: &[f32], _| append_f32(data, channels, &target), error_callback, None)?,
            SampleFormat::I16 => device.build_input_stream(&config, move |data: &[i16], _| append_i16(data, channels, &target), error_callback, None)?,
            SampleFormat::U16 => device.build_input_stream(&config, move |data: &[u16], _| append_u16(data, channels, &target), error_callback, None)?,
            format => return Err(anyhow!("Unsupported microphone sample format: {format:?}")),
        };
        stream.play()?;
        let target_window = unsafe { GetForegroundWindow().0 as isize };
        self.active = Some(ActiveRecording { stream, samples, sample_rate, started: Instant::now(), target_window, error: stream_error });
        Ok(())
    }

    pub fn stop(&mut self) -> Result<CompletedRecording> {
        let active = self.active.take().ok_or_else(|| anyhow!("No recording is active"))?;
        let duration_ms = active.started.elapsed().as_millis() as u64;
        drop(active.stream);
        if let Some(error) = active.error.lock().map_err(|_| anyhow!("Audio capture failed"))?.clone() {
            return Err(anyhow!("The microphone disconnected or stopped responding: {error}"));
        }
        let captured = active.samples.lock().map_err(|_| anyhow!("Audio capture failed"))?.clone();
        if captured.len() < active.sample_rate as usize / 5 { return Err(anyhow!("The recording was too short")); }
        let samples = resample_linear(&captured, active.sample_rate, 16_000);
        let peak = samples.iter().fold(0.0f32, |value, sample| value.max(sample.abs()));
        if peak < 0.002 { return Err(anyhow!("No speech was detected. Check the selected microphone.")); }
        Ok(CompletedRecording { samples, duration_ms, target_window: active.target_window })
    }

    pub fn cancel(&mut self) { self.active.take(); }
}

fn choose_device(host: &cpal::Host, preferred: Option<&str>) -> Result<Device> {
    if let Some(name) = preferred {
        if let Some(device) = host.input_devices()?.find(|candidate| candidate.name().ok().as_deref() == Some(name)) { return Ok(device); }
    }
    host.default_input_device().ok_or_else(|| anyhow!("No microphone was found"))
}

fn append_f32(data: &[f32], channels: usize, target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut output) = target.lock() { for frame in data.chunks(channels) { output.push(frame.iter().sum::<f32>() / channels as f32); } }
}
fn append_i16(data: &[i16], channels: usize, target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut output) = target.lock() { for frame in data.chunks(channels) { output.push(frame.iter().map(|v| *v as f32 / i16::MAX as f32).sum::<f32>() / channels as f32); } }
}
fn append_u16(data: &[u16], channels: usize, target: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut output) = target.lock() { for frame in data.chunks(channels) { output.push(frame.iter().map(|v| (*v as f32 / u16::MAX as f32) * 2.0 - 1.0).sum::<f32>() / channels as f32); } }
}

fn resample_linear(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate { return input.to_vec(); }
    let output_len = (input.len() as u64 * target_rate as u64 / source_rate as u64) as usize;
    let ratio = source_rate as f64 / target_rate as f64;
    (0..output_len).map(|index| {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(input.len() - 1);
        let fraction = (position - left as f64) as f32;
        input[left] * (1.0 - fraction) + input[right] * fraction
    }).collect()
}

pub fn write_wav(path: &std::path::Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in samples { writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?; }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resample_linear;
    #[test]
    fn resamples_to_expected_length() {
        let source = vec![0.0; 48_000];
        assert_eq!(resample_linear(&source, 48_000, 16_000).len(), 16_000);
    }
}
