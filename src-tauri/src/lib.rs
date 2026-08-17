mod audio;
mod models;
mod provider;
mod storage;
mod transcription;
mod windows_text;
mod worker;

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{anyhow, Result};
use audio::{write_wav, AudioController};
use keyring::Entry;
use models::{AppSettings, DictationResult, HistoryEntry};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, PhysicalPosition, State, WindowEvent,
};
use uuid::Uuid;
use worker::WorkerManager;

struct AppState {
    settings_path: PathBuf,
    temp_dir: PathBuf,
    settings: Mutex<AppSettings>,
    recorder: Mutex<AudioController>,
    history: Mutex<Connection>,
    worker: Mutex<WorkerManager>,
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
fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    storage::save_settings(&state.settings_path, &settings).map_err(|error| error.to_string())?;
    *state
        .settings
        .lock()
        .map_err(|_| "Settings are unavailable")? = settings;
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
fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings are unavailable")?
        .clone();
    state
        .recorder
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .start(settings.microphone_name.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_recording(state: State<'_, AppState>) -> Result<(), String> {
    state
        .recorder
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .cancel();
    Ok(())
}

#[tauri::command]
async fn stop_and_process(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DictationResult, String> {
    let recording = state
        .recorder
        .lock()
        .map_err(|_| "The recorder is unavailable")?
        .stop()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&state.temp_dir).map_err(|error| error.to_string())?;
    let wav_path = state.temp_dir.join(format!("{}.wav", Uuid::new_v4()));
    let temporary_audio = TemporaryAudio(wav_path);
    write_wav(&temporary_audio.0, &recording.samples).map_err(|error| error.to_string())?;
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Settings are unavailable")?
        .clone();
    let local_paths = if settings.transcription_mode == "remote" {
        None
    } else {
        Some((
            locate_worker(&app).map_err(|error| error.to_string())?,
            locate_model(&app, &settings.model).map_err(|error| error.to_string())?,
        ))
    };
    let cleanup_key = api_key();
    let remote_key = transcription_api_key();
    let app_for_task = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_task.state::<AppState>();
        process_recording(
            &state.worker,
            local_paths
                .as_ref()
                .map(|(worker, model)| (worker.as_path(), model.as_path())),
            &temporary_audio.0,
            &settings,
            cleanup_key.as_deref(),
            remote_key.as_deref(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;
    windows_text::paste_text(recording.target_window, &result.final_text)
        .map_err(|error| error.to_string())?;
    let history = state.history.lock().map_err(|_| "History is unavailable")?;
    storage::add_history(
        &history,
        &result.raw_text,
        &result.final_text,
        recording.duration_ms,
        &result.processing_mode,
    )
    .map_err(|error| error.to_string())?;
    Ok(DictationResult {
        duration_ms: recording.duration_ms,
        ..result
    })
}

fn process_recording(
    worker_manager: &Mutex<WorkerManager>,
    local_paths: Option<(&Path, &Path)>,
    wav: &Path,
    settings: &AppSettings,
    cleanup_key: Option<&str>,
    remote_key: Option<&str>,
) -> Result<DictationResult> {
    let local_transcription = || -> Result<String> {
        let (worker, model) = local_paths
            .ok_or_else(|| anyhow!("Local Whisper is unavailable for automatic fallback"))?;
        let response = worker_manager
            .lock()
            .map_err(|_| anyhow!("The transcription worker is unavailable"))?
            .transcribe(worker, model, wav, &settings.language)?;
        Ok(response.text.trim().to_string())
    };

    let (raw, transcription_warning) = match settings.transcription_mode.as_str() {
        "local" => (local_transcription()?, None),
        "remote" => (transcription::transcribe(settings, remote_key, wav)?, None),
        "automatic" => match transcription::transcribe(settings, remote_key, wav) {
            Ok(text) => (text, None),
            Err(error) => (
                local_transcription()?,
                Some(format!(
                    "Remote transcription failed; local Whisper was used: {error}"
                )),
            ),
        },
        _ => return Err(anyhow!("The selected transcription mode is unsupported")),
    };
    if raw.is_empty() {
        return Err(anyhow!("No speech was detected"));
    }
    if settings.cleanup_enabled && settings.provider != "none" {
        match provider::clean_text(settings, cleanup_key, &raw) {
            Ok(cleaned) => Ok(DictationResult {
                raw_text: raw,
                final_text: cleaned,
                duration_ms: 0,
                processing_mode: "clean".into(),
                warning: transcription_warning,
            }),
            Err(error) => Ok(DictationResult {
                raw_text: raw.clone(),
                final_text: raw,
                duration_ms: 0,
                processing_mode: "clean-fallback".into(),
                warning: Some(match transcription_warning {
                    Some(warning) => {
                        format!("{warning} AI cleanup also failed; raw text was pasted: {error}")
                    }
                    None => format!("AI cleanup failed; raw text was pasted: {error}"),
                }),
            }),
        }
    } else {
        Ok(DictationResult {
            raw_text: raw.clone(),
            final_text: raw,
            duration_ms: 0,
            processing_mode: "raw".into(),
            warning: transcription_warning,
        })
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
    .map_err(|error| error.to_string())
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
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let base = app.path().app_data_dir()?;
            fs::create_dir_all(&base)?;
            let (settings_path, history_path, temp_dir) = storage::app_paths(base);
            if temp_dir.exists() {
                fs::remove_dir_all(&temp_dir)?;
            }
            fs::create_dir_all(&temp_dir)?;
            let settings = storage::load_settings(&settings_path);
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
                settings: Mutex::new(settings),
                recorder: Mutex::new(AudioController::new()),
                history: Mutex::new(history),
                worker: Mutex::new(worker),
            });
            if let Some(overlay) = app.get_webview_window("overlay") {
                if let (Ok(Some(monitor)), Ok(window_size)) =
                    (overlay.primary_monitor(), overlay.outer_size())
                {
                    let monitor_position = monitor.position();
                    let monitor_size = monitor.size();
                    let x = monitor_position.x
                        + ((monitor_size.width.saturating_sub(window_size.width)) / 2) as i32;
                    // Leave room for the Windows taskbar.
                    let y = monitor_position.y
                        + monitor_size.height.saturating_sub(window_size.height + 72) as i32;
                    let _ = overlay.set_position(PhysicalPosition::new(x, y));
                }
            }
            let show = MenuItem::with_id(app, "show", "Open VoiceDictate", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("VoiceDictate")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
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
            test_transcription_provider,
            is_base_model_installed,
            install_base_model
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("VoiceDictate failed to start");
}
