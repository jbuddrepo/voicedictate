use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use tauri::{AppHandle, Manager, PhysicalPosition};
use windows::{
    core::w,
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_INVALID_PARAMETER, HANDLE, HWND,
            POINT, RECT, WAIT_OBJECT_0,
        },
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
        System::Threading::{CreateMutexW, OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        UI::WindowsAndMessaging::{
            FindWindowW, GetCursorPos, SetForegroundWindow, SetWindowPos, ShowWindowAsync,
            HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE,
        },
    },
};

/// Keep this handle alive for the complete app lifetime, including tray-only use.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn acquire_instance() -> Result<Option<InstanceGuard>> {
    let (handle, existing) = unsafe {
        let handle = CreateMutexW(None, false, w!("Local\\org.voicedictate.desktop.singleton"))
            .context("Could not establish VoiceDictate's single-instance guard")?;
        // Read immediately: subsequent Windows calls can overwrite this value.
        (handle, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    let guard = InstanceGuard(handle);
    if !existing {
        return Ok(Some(guard));
    }

    // Another process may still be creating its webview. Never start another worker.
    for _ in 0..40 {
        if let Ok(window) = unsafe { FindWindowW(None, w!("VoiceDictate")) } {
            unsafe {
                let _ = ShowWindowAsync(window, SW_RESTORE);
                let _ = SetForegroundWindow(window);
            }
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    drop(guard);
    Ok(None)
}

/// Never erase another process's audio. Cleanup is limited to our own WAV directory.
pub struct PrivateTempDirectory(PathBuf);

impl PrivateTempDirectory {
    pub fn create(parent: &Path) -> Result<Self> {
        fs::create_dir_all(parent)?;
        let metadata = fs::symlink_metadata(parent)?;
        if !plain_directory(&metadata) {
            return Err(anyhow!("The audio temporary directory must not be a link"));
        }
        cleanup_orphaned_audio(parent);
        let path = parent.join(format!("{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

fn plain_directory(metadata: &fs::Metadata) -> bool {
    metadata.is_dir() && metadata.file_attributes() & 0x400 == 0
}

fn process_definitely_stopped(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                let stopped = WaitForSingleObject(handle, 0) == WAIT_OBJECT_0;
                let _ = CloseHandle(handle);
                stopped
            }
            // Access denial or any other failure leaves liveness unknown.
            Err(error) => {
                error.code() == windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0)
            }
        }
    }
}

fn cleanup_orphaned_audio(parent: &Path) {
    cleanup_orphaned_audio_when(parent, process_definitely_stopped);
}

fn cleanup_orphaned_audio_when(parent: &Path, stopped: impl Fn(u32) -> bool) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((pid, nonce)) = name.to_str().and_then(|name| name.split_once('-')) else {
            continue;
        };
        let (Ok(pid), Ok(_)) = (pid.parse::<u32>(), uuid::Uuid::parse_str(nonce)) else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !plain_directory(&metadata) || !stopped(pid) {
            continue;
        }
        let Ok(files) = fs::read_dir(entry.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let known_audio = path.extension().and_then(|ext| ext.to_str()) == Some("wav")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| uuid::Uuid::parse_str(stem).is_ok());
            let regular = fs::symlink_metadata(&path)
                .map(|meta| meta.is_file() && meta.file_attributes() & 0x400 == 0)
                .unwrap_or(false);
            if known_audio && regular {
                let _ = fs::remove_file(&path);
            }
        }
        // Nonrecursive removal refuses unknown contents, nested directories and links.
        let _ = fs::remove_dir(entry.path());
    }
}

impl Drop for PrivateTempDirectory {
    fn drop(&mut self) {
        if let Ok(entries) = fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                if entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        let _ = fs::remove_dir(&self.0);
    }
}

fn overlay_coordinates(work: RECT, width: u32, height: u32) -> (i32, i32) {
    // All values are physical desktop coordinates, including negative monitor origins.
    let available_width = i64::from(work.right) - i64::from(work.left);
    let max_x = (i64::from(work.right) - i64::from(width)).max(i64::from(work.left));
    let max_y = (i64::from(work.bottom) - i64::from(height)).max(i64::from(work.top));
    let x = i64::from(work.left) + (available_width - i64::from(width)).max(0) / 2;
    let y = i64::from(work.bottom) - i64::from(height) - 16;
    (
        x.clamp(i64::from(work.left), max_x) as i32,
        y.clamp(i64::from(work.top), max_y) as i32,
    )
}

pub fn position_overlay(app: &AppHandle) -> Result<()> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or_else(|| anyhow!("Status window is unavailable"))?;
    let size = overlay.outer_size()?;
    let saved = app.state::<crate::overlay::Placement>().position();
    let work = unsafe {
        let mut cursor = POINT::default();
        if let Some(position) = saved {
            cursor = POINT {
                x: position.x,
                y: position.y,
            };
        } else {
            GetCursorPos(&mut cursor).context("Could not locate the active display")?;
        }
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err(anyhow!("Could not read the display work area"));
        }
        info.rcWork
    };
    let (x, y) = if let Some(position) = saved {
        clamp_saved_position(work, size.width, size.height, position)
    } else {
        overlay_coordinates(work, size.width, size.height)
    };
    overlay.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

fn clamp_saved_position(
    work: RECT,
    width: u32,
    height: u32,
    position: crate::overlay::SavedPosition,
) -> (i32, i32) {
    let max_x = (i64::from(work.right) - i64::from(width)).max(i64::from(work.left));
    let max_y = (i64::from(work.bottom) - i64::from(height)).max(i64::from(work.top));
    (
        i64::from(position.x).clamp(i64::from(work.left), max_x) as i32,
        i64::from(position.y).clamp(i64::from(work.top), max_y) as i32,
    )
}

/// Visibility is independent of ongoing transcription or cleanup.
pub fn show_status(app: &AppHandle, visible: bool) -> Result<()> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or_else(|| anyhow!("Status window is unavailable"))?;
    if !visible {
        overlay.hide()?;
        return Ok(());
    }
    position_overlay(app)?;
    // Show without taking keyboard focus from the application receiving dictation.
    unsafe {
        SetWindowPos(
            HWND(overlay.hwnd()?.0 as *mut _),
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_position_is_clamped_after_monitor_changes() {
        let work = RECT {
            left: -1000,
            top: 0,
            right: 0,
            bottom: 800,
        };
        assert_eq!(
            clamp_saved_position(
                work,
                210,
                57,
                crate::overlay::SavedPosition { x: 4000, y: -500 }
            ),
            (-210, 0)
        );
    }

    #[test]
    fn uses_negative_monitor_origin_and_taskbar_work_area() {
        let work = RECT {
            left: -1920,
            top: -100,
            right: 0,
            bottom: 940,
        };
        assert_eq!(overlay_coordinates(work, 280, 76), (-1100, 848));
    }

    #[test]
    fn clamps_to_small_work_area() {
        let work = RECT {
            left: 50,
            top: 60,
            right: 250,
            bottom: 110,
        };
        assert_eq!(overlay_coordinates(work, 280, 76), (50, 60));
    }

    #[test]
    fn temp_cleanup_never_removes_another_instances_audio() {
        let root =
            std::env::temp_dir().join(format!("voicedictate-temp-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let first = PrivateTempDirectory::create(&root).unwrap();
        let second = PrivateTempDirectory::create(&root).unwrap();
        let first_path = first.path().to_path_buf();
        let second_path = second.path().to_path_buf();
        assert_ne!(first_path, second_path);
        fs::write(first_path.join("first.wav"), b"synthetic fixture").unwrap();
        fs::write(second_path.join("second.wav"), b"synthetic fixture").unwrap();
        drop(first);
        assert!(!first_path.exists());
        assert!(second_path.join("second.wav").exists());
        drop(second);
        assert!(!second_path.exists());
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn orphan_cleanup_requires_known_dead_owner_and_preserves_unknown_files() {
        let root =
            std::env::temp_dir().join(format!("voicedictate-orphan-test-{}", uuid::Uuid::new_v4()));
        let orphan = root.join(format!("404-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&orphan).unwrap();
        let wav = orphan.join(format!("{}.wav", uuid::Uuid::new_v4()));
        let unrelated = orphan.join("keep.txt");
        fs::write(&wav, b"synthetic fixture").unwrap();
        fs::write(&unrelated, b"unrelated").unwrap();
        cleanup_orphaned_audio_when(&root, |_| false);
        assert!(wav.exists());
        cleanup_orphaned_audio_when(&root, |pid| pid == 404);
        assert!(!wav.exists());
        assert!(unrelated.exists());
        assert!(orphan.exists());
        fs::remove_file(unrelated).unwrap();
        fs::remove_dir(orphan).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
