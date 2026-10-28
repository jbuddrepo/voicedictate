mod audio;
mod desktop;
mod hotkey;
mod models;
mod overlay;
mod pipeline;
mod provider;
mod startup;
mod storage;
mod streaming;
mod transcription;
mod vocabulary;
mod windows_text;
mod worker;

use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use anyhow::{anyhow, Result};
use audio::AudioController;
use keyring::Entry;
use models::{AppSettings, DictationResult, HistoryEntry, ProcessingTimings, StatusPayload};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_opener::OpenerExt;
use worker::WorkerManager;

struct AppState {
    settings_path: PathBuf,
    temp_dir: PathBuf,
    settings: Mutex<AppSettings>,
    recorder: Arc<Mutex<AudioController>>,
    history: Mutex<Connection>,
    worker: Arc<Mutex<WorkerManager>>,
    session: Mutex<Option<pipeline::Session>>,
    status: Mutex<StatusPayload>,
    visibility: Mutex<overlay::Visibility>,
    settings_requested: AtomicBool,
    processing: Arc<AtomicBool>,
}

struct ProcessingGuard(Arc<AtomicBool>);
impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn present_status(app: &AppHandle) {
    let queued_app = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let state = queued_app.state::<AppState>();
        let (current, visible) = match state.status.lock() {
            Ok(current) => (
                current.clone(),
                state
                    .visibility
                    .lock()
                    .map(|v| v.is_visible())
                    .unwrap_or(false),
            ),
            Err(_) => return,
        };
        // No data mutex may be held across UI operations. Read the latest state
        // on the UI thread so an old idle task cannot hide a new recording.
        if let Err(error) = desktop::show_status(&queued_app, visible) {
            eprintln!("VoiceDictate status window failed: {error:#}");
        }
        if let Err(error) = queued_app.emit("dictation-status", current) {
            eprintln!("VoiceDictate could not deliver status: {error}");
        }
    }) {
        eprintln!("VoiceDictate could not queue status: {error}");
    }
}

fn publish_status(app: &AppHandle, status: &str, text: &str) -> Option<u64> {
    let state = app.state::<AppState>();
    let behavior = state
        .settings
        .lock()
        .map(|s| s.activation_behavior.clone())
        .unwrap_or_else(|_| "toggle".into());
    let (revision, hide_token) = if let Ok(mut current) = state.status.lock() {
        current.status = status.into();
        current.text = text.into();
        current.activation_behavior = behavior;
        current.revision += 1;
        let hide_token = state
            .visibility
            .lock()
            .ok()
            .and_then(|mut v| v.update(status));
        (Some(current.revision), hide_token)
    } else {
        (None, None)
    };
    if let Some(token) = hide_token {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = tauri::async_runtime::spawn_blocking(|| {
                std::thread::sleep(std::time::Duration::from_millis(1200));
            })
            .await;
            let changed = app
                .state::<AppState>()
                .visibility
                .lock()
                .map(|mut v| v.hide_if_current(token))
                .unwrap_or(false);
            if changed {
                present_status(&app);
            }
        });
    }
    present_status(app);
    revision
}

fn terminal_status(app: &AppHandle, status: &str, text: &str) {
    let Some(revision) = publish_status(app, status, text) else {
        return;
    };
    let app = app.clone();
    let delay = if status == "error" { 8 } else { 3 };
    tauri::async_runtime::spawn(async move {
        // Blocking sleep stays off the GUI and async runtime threads.
        let _ = tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_secs(delay))
        })
        .await;
        let state = app.state::<AppState>();
        let changed = if let Ok(mut current) = state.status.lock() {
            if current.revision == revision {
                current.revision += 1;
                current.status = "idle".into();
                current.text = "Ready to dictate".into();
                if let Ok(mut visibility) = state.visibility.lock() {
                    visibility.update("idle");
                }
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            present_status(&app);
        }
    });
}

#[tauri::command]
fn move_overlay(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "overlay" {
        return Err("Only the dictation pop-up can be moved here".into());
    }
    app.state::<overlay::Placement>().begin_drag();
    window.start_dragging().map_err(|error| error.to_string())
}

fn open_main_window(app: &AppHandle, settings: bool) {
    if settings {
        app.state::<AppState>()
            .settings_requested
            .store(true, Ordering::Release);
    }
    if let Some(window) = app.get_webview_window("main") {
        let result = window
            .unminimize()
            .and_then(|_| window.show())
            .and_then(|_| window.set_focus());
        if let Err(error) = result {
            eprintln!("VoiceDictate could not open its main window: {error}");
        }
        if settings {
            if let Err(error) = window.emit("open-settings", ()) {
                eprintln!("VoiceDictate could not open Settings: {error}");
            }
        }
    }
}

#[tauri::command]
fn take_settings_request(state: State<'_, AppState>) -> bool {
    state.settings_requested.swap(false, Ordering::AcqRel)
}

#[tauri::command]
fn get_current_status(app: AppHandle, state: State<'_, AppState>) -> Result<StatusPayload, String> {
    let current = state
        .status
        .lock()
        .map_err(|_| "Status is unavailable")?
        .clone();
    // Renderer ready/reload handshake: replay current state and recover visibility.
    present_status(&app);
    Ok(current)
}

struct TemporaryAudio(PathBuf);

impl Drop for TemporaryAudio {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn keyring_entry() -> Result<Entry> {
    Entry::new("VoiceDictate", "provider-api-key").map_err(Into::into)
}

fn transcription_keyring_entry() -> Result<Entry> {
    Entry::new("VoiceDictate", "transcription-api-key").map_err(Into::into)
}

fn api_key() -> Option<String> {
    keyring_entry()
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

fn transcription_api_key() -> Option<String> {
    transcription_keyring_entry()
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let mut value = state
        .settings
        .lock()
        .map_err(|_| "Settings are unavailable")?
        .clone();
    value.provider_api_key_configured = api_key().is_some();
    value.transcription_api_key_configured = transcription_api_key().is_some();
    Ok(value)
}

#[tauri::command]
fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
    hotkey: State<'_, hotkey::HotkeyService>,
) -> Result<(), String> {
    if !matches!(settings.activation_behavior.as_str(), "toggle" | "hold") {
        return Err("Choose Tap to start/stop or Hold to talk".into());
    }
    hotkey::validate_shortcut(&settings.shortcut)?;
    vocabulary::validate(&settings.personal_vocabulary)?;
    let lifecycle = state
        .session
        .lock()
        .map_err(|_| "Recorder is unavailable")?;
    if lifecycle.is_some() {
        return Err("Finish dictating before saving settings".into());
    }
    if state.processing.load(Ordering::Acquire) {
        return Err("Wait until dictation finishes before saving settings".into());
    }
    if !matches!(
        settings.transcription_mode.as_str(),
        "local" | "remote" | "automatic"
    ) {
        return Err("Choose a supported transcription method".into());
    }
    let mut current = state
        .settings
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    hotkey.update(&settings.shortcut)?;
    if let Err(error) = storage::save_settings(&state.settings_path, &settings) {
        let _ = hotkey.update(&current.shortcut);
        return Err(error.to_string());
    }
    *current = settings;
    drop(current);
    drop(lifecycle);
    Ok(())
}

#[tauri::command]
fn set_provider_api_key(api_key: String) -> Result<(), String> {
    let entry = keyring_entry().map_err(|error| error.to_string())?;
    if api_key.is_empty() {
        entry
            .delete_credential()
            .or_else(|_| Ok(()))
            .map_err(|error: keyring::Error| error.to_string())
    } else {
        entry
            .set_password(&api_key)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn set_transcription_api_key(api_key: String) -> Result<(), String> {
    let entry = transcription_keyring_entry().map_err(|error| error.to_string())?;
    if api_key.is_empty() {
        entry
            .delete_credential()
            .or_else(|_| Ok(()))
            .map_err(|error: keyring::Error| error.to_string())
    } else {
        entry
            .set_password(&api_key)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn list_microphones() -> Result<Vec<String>, String> {
    AudioController::list_devices().map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_recording(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = begin_recording(&app, &state);
        if let Err(error) = &result {
            terminal_status(&app, "error", error);
        }
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

fn begin_recording(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "The recorder is unavailable")?;
    if session.is_some() {
        return Err("Recording is already active".into());
    }
    if state.processing.load(Ordering::Acquire) {
        return Err("A dictation is still being processed".into());
    }
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings are unavailable")?
        .clone();
    vocabulary::validate(&settings.personal_vocabulary)?;
    let local_paths = if settings.transcription_mode == "remote" {
        None
    } else {
        Some((
            locate_worker(&app).map_err(|e| e.to_string())?,
            locate_model(&app, &settings.model).map_err(|e| e.to_string())?,
        ))
    };
    let recognizer = pipeline::Recognizer::new(
        settings.clone(),
        Arc::clone(&state.worker),
        local_paths,
        transcription_api_key(),
        state.temp_dir.clone(),
    );
    let result = state
        .recorder
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .start(settings.microphone_name.as_deref())
        .map_err(|error| error.to_string());
    result?;
    match pipeline::Session::start(settings, Arc::clone(&state.recorder), recognizer) {
        Ok(active) => {
            *session = Some(active);
            publish_status(app, "listening", "Listening…");
            Ok(())
        }
        Err(error) => {
            if let Ok(recorder) = state.recorder.lock() {
                recorder.cancel();
            }
            Err(error.to_string())
        }
    }
}

#[tauri::command]
async fn cancel_recording(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cancel_active_recording(&app))
        .await
        .map_err(|error| error.to_string())?
}

fn cancel_active_recording(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut session = state
        .session
        .lock()
        .map_err(|_| "The recorder is unavailable")?;
    if state.processing.load(Ordering::Acquire) {
        return Err("Dictation is already being processed".into());
    }
    session.take();
    state
        .recorder
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .cancel();
    drop(session);
    publish_status(&app, "idle", "Ready to dictate");
    Ok(())
}

#[tauri::command]
async fn stop_and_process(app: AppHandle) -> Result<DictationResult, String> {
    let result = finish_recording(&app).await;
    if let Err(error) = &result {
        terminal_status(&app, "error", error);
    }
    result
}

async fn finish_recording(app: &AppHandle) -> Result<DictationResult, String> {
    let state = app.state::<AppState>();
    let started = Instant::now();
    state
        .processing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "A dictation is still being processed")?;
    let _processing = ProcessingGuard(Arc::clone(&state.processing));
    let mut session = state
        .session
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .take()
        .ok_or_else(|| "No recording is active".to_string())?;
    session.stop();
    let settings = session.settings.clone();
    let recording = match state
        .recorder
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .stop()
        .map_err(|error| error.to_string())
    {
        Ok(recording) => recording,
        Err(error) => {
            terminal_status(&app, "error", &error);
            return Err(error);
        }
    };
    let capture_ms = started.elapsed().as_millis() as u64;
    publish_status(
        &app,
        "transcribing",
        if settings.transcription_mode == "local" {
            "Finishing local transcription…"
        } else {
            "Finishing transcription…"
        },
    );
    let cleanup_key = api_key();
    let app_for_task = app.clone();
    let target_window = recording.target_window;
    let duration_ms = recording.duration_ms;
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<DictationResult> {
        let finishing = Instant::now();
        let (raw, progress) = session.finish(&recording.samples)?;
        let finish_ms = finishing.elapsed().as_millis() as u64;
        let cleanup = Instant::now();
        if settings.cleanup_enabled && settings.provider != "none" {
            publish_status(&app_for_task, "cleaning", "Cleaning up text…");
        }
        let mut result = finish_transcription(
            raw,
            progress.recognizer.warning,
            &settings,
            cleanup_key.as_deref(),
        )?;
        result.transcription_source = progress.recognizer.source;
        result.timings = ProcessingTimings {
            capture_ms,
            finish_ms,
            cleanup_ms: cleanup.elapsed().as_millis() as u64,
            inference_ms: progress.recognizer.inference_ms,
            incremental_chunks: progress.chunks,
            cpu_threads: progress.recognizer.threads,
            ..Default::default()
        };
        Ok(result)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string());
    let mut result = match result {
        Ok(result) => result,
        Err(error) => {
            terminal_status(&app, "error", &error);
            return Err(error);
        }
    };
    let paste = Instant::now();
    if let Err(error) = windows_text::paste_text(target_window, &result.final_text) {
        let error = error.to_string();
        terminal_status(&app, "error", &error);
        return Err(error);
    }
    result.timings.paste_ms = paste.elapsed().as_millis() as u64;
    result.timings.release_to_text_ms = started.elapsed().as_millis() as u64;
    let history_result = state
        .history
        .lock()
        .map_err(|_| anyhow!("History is unavailable"))
        .and_then(|history| {
            storage::add_history(
                &history,
                &result.raw_text,
                &result.final_text,
                duration_ms,
                &result.processing_mode,
                &result.transcription_source,
                result.timings.release_to_text_ms,
            )
        });
    if let Err(error) = history_result {
        let warning = format!("Text was pasted but could not be saved in history: {error}");
        result.warning = Some(match result.warning {
            Some(previous) => format!("{previous} {warning}"),
            None => warning,
        });
    }
    result.duration_ms = duration_ms;
    terminal_status(
        &app,
        "success",
        if result.warning.is_some() {
            "Text pasted · see app for details"
        } else {
            "Text pasted"
        },
    );
    Ok(result)
}

fn finish_transcription(
    raw: String,
    transcription_warning: Option<String>,
    settings: &AppSettings,
    cleanup_key: Option<&str>,
) -> Result<DictationResult> {
    if raw.is_empty() {
        return Err(anyhow!("No speech was detected"));
    }
    let corrected = vocabulary::apply(&raw, &settings.personal_vocabulary);
    if settings.cleanup_enabled && settings.provider != "none" {
        // Keep the glossary local: the cleanup provider receives only the original transcript.
        match provider::clean_text(settings, cleanup_key, &raw) {
            Ok(cleaned) => Ok(DictationResult {
                raw_text: raw,
                final_text: vocabulary::apply(&cleaned, &settings.personal_vocabulary),
                duration_ms: 0,
                processing_mode: "clean".into(),
                warning: transcription_warning,
                transcription_source: String::new(),
                timings: ProcessingTimings::default(),
            }),
            Err(error) => Ok(DictationResult {
                raw_text: raw.clone(),
                final_text: corrected,
                duration_ms: 0,
                processing_mode: "clean-fallback".into(),
                warning: Some(match transcription_warning {
                    Some(warning) => {
                        format!("{warning} AI cleanup also failed; raw text was pasted: {error}")
                    }
                    None => format!("AI cleanup failed; raw text was pasted: {error}"),
                }),
                transcription_source: String::new(),
                timings: ProcessingTimings::default(),
            }),
        }
    } else {
        Ok(DictationResult {
            raw_text: raw.clone(),
            final_text: corrected,
            duration_ms: 0,
            processing_mode: "raw".into(),
            warning: transcription_warning,
            transcription_source: String::new(),
            timings: ProcessingTimings::default(),
        })
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[test]
    fn personal_vocabulary_corrects_output_and_preserves_original_history() {
        let mut settings = AppSettings::default();
        settings
            .personal_vocabulary
            .push(vocabulary::VocabularyEntry {
                term: "AcmeCloud".into(),
                mishearings: vec!["ack me cloud".into()],
                enabled: true,
            });
        for cleanup_enabled in [false, true] {
            settings.cleanup_enabled = cleanup_enabled;
            settings.provider = "ollama".into();
            settings.provider_model = "test".into();
            settings.provider_base_url = "invalid-url".into();
            let result =
                finish_transcription("Use ack me cloud today.".into(), None, &settings, None)
                    .unwrap();
            assert_eq!(result.raw_text, "Use ack me cloud today.");
            assert_eq!(result.final_text, "Use AcmeCloud today.");
        }
    }

    #[test]
    fn failed_cleanup_preserves_raw_transcript_and_fallback_warning() {
        let settings = AppSettings {
            cleanup_enabled: true,
            provider: "ollama".into(),
            provider_base_url: "invalid-url".into(),
            provider_model: "test-model".into(),
            ..AppSettings::default()
        };
        let result = finish_transcription(
            "A synthetic test sentence.".into(),
            Some("Local fallback was used.".into()),
            &settings,
            None,
        )
        .unwrap();
        assert_eq!(result.final_text, "A synthetic test sentence.");
        assert_eq!(result.raw_text, result.final_text);
        assert_eq!(result.processing_mode, "clean-fallback");
        let warning = result.warning.unwrap();
        assert!(warning.contains("Local fallback was used."));
        assert!(warning.contains("AI cleanup also failed"));
    }
}

#[tauri::command]
fn list_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    let history = state.history.lock().map_err(|_| "History is unavailable")?;
    storage::list_history(&history).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_history(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .history
        .lock()
        .map_err(|_| "History is unavailable")?
        .execute("DELETE FROM history WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state
        .history
        .lock()
        .map_err(|_| "History is unavailable")?
        .execute_batch("DELETE FROM history; VACUUM;")
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn test_provider(settings: AppSettings) -> Result<Vec<String>, String> {
    let key = api_key();
    tauri::async_runtime::spawn_blocking(move || {
        provider::discover_models(&settings, key.as_deref())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
async fn test_cleanup(mut settings: AppSettings) -> Result<(), String> {
    if !matches!(settings.provider.as_str(), "ollama" | "openai-compatible") {
        return Err("Select a cleanup provider first".into());
    }
    settings.cleanup_enabled = true;
    let key = api_key();
    tauri::async_runtime::spawn_blocking(move || {
        provider::clean_text(
            &settings,
            key.as_deref(),
            "This is a short connection test.",
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map(|_| ())
    .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
async fn test_transcription_provider(settings: AppSettings) -> Result<(), String> {
    let key = transcription_api_key();
    tauri::async_runtime::spawn_blocking(move || {
        transcription::test_connection(&settings, key.as_deref())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_help_document(app: AppHandle, document: String) -> Result<(), String> {
    let file = match document.as_str() {
        "remote-transcription" => "REMOTE-TRANSCRIPTION.md",
        "building" => "BUILDING.md",
        "video-guide" => "VIDEO-GUIDE.md",
        _ => return Err("The requested help document is unavailable".into()),
    };
    let path = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join("docs")
        .join(file);
    if !path.exists() {
        return Err(format!("The bundled help document is missing: {file}"));
    }
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| error.to_string())
}

const BASE_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
const BASE_MODEL_SHA256: &str = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";

#[tauri::command]
fn is_base_model_installed(app: AppHandle) -> Result<bool, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("models")
        .join("ggml-base.bin");
    Ok(path.exists())
}

#[tauri::command]
async fn install_base_model(app: AppHandle) -> Result<(), String> {
    let model_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("models");
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        fs::create_dir_all(&model_dir)?;
        let destination = model_dir.join("ggml-base.bin");
        let temporary = model_dir.join("ggml-base.bin.download");
        struct RemoveDownload(PathBuf);
        impl Drop for RemoveDownload {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _remove_download = RemoveDownload(temporary.clone());
        let mut response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()?
            .get(BASE_MODEL_URL)
            .send()?
            .error_for_status()?;
        let mut file = fs::File::create(&temporary)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 128 * 1024];
        loop {
            let count = response.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])?;
            hasher.update(&buffer[..count]);
        }
        file.sync_all()?;
        if format!("{:x}", hasher.finalize()) != BASE_MODEL_SHA256 {
            return Err(anyhow!(
                "The Base model checksum did not match; the download was discarded"
            ));
        }
        if destination.exists() {
            fs::remove_file(&destination)?;
        }
        fs::rename(&temporary, &destination)?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

fn locate_worker(app: &AppHandle) -> Result<PathBuf> {
    let use_avx2 = cfg!(target_arch = "x86_64")
        && std::is_x86_feature_detected!("avx2")
        && std::is_x86_feature_detected!("fma");
    let name = if use_avx2 {
        "voicedictate-worker-avx2.exe"
    } else {
        "voicedictate-worker.exe"
    };
    let candidates = [
        app.path().resource_dir()?.join("binaries").join(name),
        app.path().resource_dir()?.join(name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug")
            .join("voicedictate-worker.exe"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("release")
            .join("voicedictate-worker.exe"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .or_else(|| {
            if use_avx2 {
                let fallback = app
                    .path()
                    .resource_dir()
                    .ok()?
                    .join("voicedictate-worker.exe");
                fallback.exists().then_some(fallback)
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("The local transcription worker is missing"))
}

fn locate_model(app: &AppHandle, model: &str) -> Result<PathBuf> {
    let file = if model == "base" {
        "ggml-base.bin"
    } else {
        "ggml-tiny.bin"
    };
    let app_data = app.path().app_data_dir()?.join("models").join(file);
    let candidates = [
        app_data,
        app.path()
            .resource_dir()?
            .join("resources")
            .join("models")
            .join(file),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("models")
            .join(file),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("The selected Whisper model is not installed"))
}

pub fn run() {
    let _instance = match desktop::acquire_instance() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(error) => {
            eprintln!("VoiceDictate could not establish a single app instance: {error:#}");
            return;
        }
    };
    let mut context = tauri::generate_context!();
    let startup_windows = startup::defer_windows(context.config_mut());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let base = app.path().app_data_dir()?;
            fs::create_dir_all(&base)?;
            app.manage(overlay::Placement::load(base.join("overlay-position.json")));
            let (settings_path, history_path, temp_root) = storage::app_paths(base);
            let private_temp = desktop::PrivateTempDirectory::create(&temp_root)?;
            let temp_dir = private_temp.path().to_path_buf();
            app.manage(private_temp);
            let mut settings = storage::load_settings(&settings_path);
            let shortcut_reset = hotkey::validate_shortcut(&settings.shortcut).is_err();
            if shortcut_reset {
                settings.shortcut = AppSettings::default().shortcut;
            }
            let hotkey = hotkey::HotkeyService::start(app.handle(), &settings.shortcut)
                .map_err(anyhow::Error::msg)?;
            app.manage(hotkey);
            let history = storage::open_history(&history_path)?;
            let mut worker = WorkerManager::new();
            if settings.transcription_mode != "remote" {
                if let (Ok(worker_path), Ok(model_path)) = (
                    locate_worker(app.handle()),
                    locate_model(app.handle(), &settings.model),
                ) {
                    // Starting the process here lets it load the selected model in
                    // the background while the user opens or focuses another app.
                    if let Err(error) = worker.preload(&worker_path, &model_path) {
                        eprintln!("VoiceDictate could not preload Whisper: {error:#}");
                    }
                }
            }
            app.manage(AppState {
                settings_path,
                temp_dir,
                status: Mutex::new(StatusPayload {
                    status: "idle".into(),
                    text: "Ready to dictate".into(),
                    activation_behavior: settings.activation_behavior.clone(),
                    revision: 0,
                }),
                settings: Mutex::new(settings),
                recorder: Arc::new(Mutex::new(AudioController::new())),
                history: Mutex::new(history),
                worker: Arc::new(Mutex::new(worker)),
                session: Mutex::new(None),
                visibility: Mutex::new(overlay::Visibility::default()),
                settings_requested: AtomicBool::new(false),
                processing: Arc::new(AtomicBool::new(false)),
            });
            // All commands' state, including the shortcut service, is managed
            // before either renderer can load and make its first IPC request.
            for window in &startup_windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), window)?.build()?;
            }
            if shortcut_reset {
                terminal_status(
                    app.handle(),
                    "error",
                    "Saved shortcut unsupported · using Ctrl + Alt + Space",
                );
            }
            if let Err(error) = desktop::position_overlay(app.handle()) {
                eprintln!("VoiceDictate could not position its status window: {error:#}");
            }
            let show = MenuItem::with_id(app, "show", "Open VoiceDictate", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &settings, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("VoiceDictate")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        }
                    ) {
                        open_main_window(tray.app_handle(), false);
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => open_main_window(app, false),
                    "settings" => open_main_window(app, true),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            get_current_status,
            move_overlay,
            take_settings_request,
            hotkey::set_shortcut_capture,
            hotkey::validate_dictation_shortcut,
            save_settings,
            set_provider_api_key,
            set_transcription_api_key,
            list_microphones,
            start_recording,
            cancel_recording,
            stop_and_process,
            list_history,
            delete_history,
            clear_history,
            test_provider,
            test_cleanup,
            test_transcription_provider,
            open_help_document,
            is_base_model_installed,
            install_base_model
        ])
        .on_window_event(|window, event| {
            if window.label() == "overlay" {
                if let WindowEvent::Moved(position) = event {
                    window.app_handle().state::<overlay::Placement>().remember(
                        overlay::SavedPosition {
                            x: position.x,
                            y: position.y,
                        },
                    );
                }
            }
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(context)
        .expect("VoiceDictate failed to start");
}
