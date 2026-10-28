use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Tracks whether the overlay is visible and guards delayed hides with a token.
#[derive(Debug, Default)]
pub struct Visibility {
    visible: bool,
    ending: bool,
    generation: u64,
}

impl Visibility {
    /// Update from a recording status. A returned token can be passed to
    /// `hide_if_current` after the overlay's linger delay.
    pub fn update(&mut self, status: &str) -> Option<u64> {
        if status.eq_ignore_ascii_case("listening") {
            self.generation = self.generation.wrapping_add(1);
            self.visible = true;
            self.ending = false;
            return None;
        }

        if status.eq_ignore_ascii_case("idle") {
            self.generation = self.generation.wrapping_add(1);
            self.visible = false;
            self.ending = false;
            return None;
        }

        if ["transcribing", "cleaning", "success", "error"]
            .iter()
            .any(|candidate| status.eq_ignore_ascii_case(candidate))
            && self.visible
            && !self.ending
        {
            self.ending = true;
            return Some(self.generation);
        }

        None
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Hide only if this is still the same recording that scheduled the hide.
    pub fn hide_if_current(&mut self, token: u64) -> bool {
        if self.visible && self.ending && token == self.generation {
            self.visible = false;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SavedPosition {
    pub x: i32,
    pub y: i32,
}

struct PlacementState {
    manual_drag: AtomicBool,
    position: Mutex<Option<SavedPosition>>,
}

enum WriterMessage {
    Save(SavedPosition),
    Stop,
}

/// Loads and remembers manually chosen overlay coordinates.
///
/// Persistence is debounced on a single owned worker thread. Only the position
/// is serialized to the supplied path.
pub struct Placement {
    state: Arc<PlacementState>,
    sender: Sender<WriterMessage>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Placement {
    pub fn load(path: PathBuf) -> Self {
        let position = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SavedPosition>(&bytes).ok());
        let state = Arc::new(PlacementState {
            manual_drag: AtomicBool::new(false),
            position: Mutex::new(position),
        });
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || writer_loop(receiver, path));

        Self {
            state,
            sender,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn position(&self) -> Option<SavedPosition> {
        self.state
            .position
            .lock()
            .ok()
            .and_then(|position| *position)
    }

    /// Marks that the user has intentionally dragged the overlay.
    pub fn begin_drag(&self) {
        self.state.manual_drag.store(true, Ordering::Release);
    }

    /// Remember coordinates only after a user initiated drag.
    pub fn remember(&self, position: SavedPosition) {
        if !self.state.manual_drag.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut current) = self.state.position.lock() {
            *current = Some(position);
        }
        let _ = self.sender.send(WriterMessage::Save(position));
    }
}

impl Drop for Placement {
    fn drop(&mut self) {
        let _ = self.sender.send(WriterMessage::Stop);
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

fn writer_loop(receiver: mpsc::Receiver<WriterMessage>, path: PathBuf) {
    loop {
        let mut pending = match receiver.recv() {
            Ok(WriterMessage::Save(position)) => position,
            Ok(WriterMessage::Stop) | Err(_) => return,
        };

        loop {
            match receiver.recv_timeout(Duration::from_millis(300)) {
                Ok(WriterMessage::Save(position)) => pending = position,
                Ok(WriterMessage::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    write_position(&path, pending);
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {
                    write_position(&path, pending);
                    break;
                }
            }
        }
    }
}

fn write_position(path: &PathBuf, position: SavedPosition) {
    if let Ok(bytes) = serde_json::to_vec(&position) {
        let _ = fs::write(path, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    #[test]
    fn new_recording_survives_an_old_hide_token() {
        let mut visibility = Visibility::default();
        visibility.update("listening");
        let old_token = visibility.update("transcribing").unwrap();
        visibility.update("listening");

        assert!(!visibility.hide_if_current(old_token));
        assert!(visibility.is_visible());
    }

    #[test]
    fn repeated_processing_does_not_extend_the_linger() {
        let mut visibility = Visibility::default();
        visibility.update("listening");
        let token = visibility.update("transcribing").unwrap();

        assert_eq!(visibility.update("cleaning"), None);
        assert!(visibility.hide_if_current(token));
        assert!(!visibility.is_visible());
    }

    #[test]
    fn terminal_status_does_not_reshow_a_hidden_overlay() {
        let mut visibility = Visibility::default();
        visibility.update("success");
        assert!(!visibility.is_visible());
    }

    #[test]
    fn idle_clears_visibility_and_invalidates_pending_hides() {
        let mut visibility = Visibility::default();
        visibility.update("listening");
        let token = visibility.update("error").unwrap();
        visibility.update("idle");

        assert!(!visibility.is_visible());
        assert!(!visibility.hide_if_current(token));
    }

    fn temp_path() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "voice-dictate-placement-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, AtomicOrdering::Relaxed)
        ))
    }

    #[test]
    fn position_is_not_persisted_until_a_manual_drag() {
        let path = temp_path();
        let placement = Placement::load(path.clone());
        placement.remember(SavedPosition { x: 12, y: 34 });
        drop(placement);

        assert!(!path.exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn loaded_position_is_preserved() {
        let path = temp_path();
        let saved = SavedPosition { x: -42, y: 100 };
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let placement = Placement::load(path.clone());

        assert_eq!(placement.position(), Some(saved));
        drop(placement);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn manual_position_is_flushed_on_exit_and_reloaded() {
        let path = temp_path();
        let saved = SavedPosition { x: -300, y: 90 };
        let placement = Placement::load(path.clone());
        placement.begin_drag();
        placement.remember(saved);
        drop(placement);
        let reloaded = Placement::load(path.clone());
        assert_eq!(reloaded.position(), Some(saved));
        drop(reloaded);
        fs::remove_file(path).unwrap();
    }
}
