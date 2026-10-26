//! A passive Windows keyboard hook. Only chord state leaves this module; keys
//! are never logged, persisted, or sent to the frontend.
use serde::Serialize;
use std::{
    collections::BTreeSet,
    sync::{mpsc, Arc, Mutex, OnceLock},
    thread,
};
use tauri::{AppHandle, Emitter, State};
use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::Input::KeyboardAndMouse::GetAsyncKeyState,
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, PostThreadMessageW,
        SetTimer, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
        LLKHF_INJECTED, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
        WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
    },
};

const CTRL: u32 = 0x11;
const SHIFT: u32 = 0x10;
const ALT: u32 = 0x12;
const WIN: u32 = 0x5b;

fn normalized(key: u32) -> u32 {
    match key {
        0xa0 | 0xa1 => SHIFT,
        0xa2 | 0xa3 => CTRL,
        0xa4 | 0xa5 => ALT,
        0x5c => WIN,
        _ => key,
    }
}

fn key_code(token: &str) -> Option<u32> {
    let token = token.to_ascii_uppercase();
    if let Some(suffix) = token.strip_prefix("KEY") {
        if suffix.len() == 1 {
            return key_code(suffix);
        }
    }
    if let Some(suffix) = token.strip_prefix("DIGIT") {
        if suffix.len() == 1 {
            return key_code(suffix);
        }
    }
    if token.len() == 1 {
        let key = token.as_bytes()[0];
        if key.is_ascii_uppercase() || key.is_ascii_digit() {
            return Some(key.into());
        }
    }
    if let Some(number) = token.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&number) {
            return Some(0x70 + number - 1);
        }
    }
    if let Some(number) = token
        .strip_prefix("NUMPAD")
        .and_then(|n| n.parse::<u32>().ok())
    {
        if number <= 9 {
            return Some(0x60 + number);
        }
    }
    Some(match token.as_str() {
        "CTRL" | "CONTROL" | "COMMANDORCONTROL" | "COMMANDORCTRL" | "CMDORCTRL"
        | "CMDORCONTROL" => CTRL,
        "SHIFT" => SHIFT,
        "ALT" | "OPTION" => ALT,
        "SUPER" | "WIN" | "WINDOWS" | "META" | "CMD" | "COMMAND" => WIN,
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "ENTER" => 0x0d,
        "ESC" | "ESCAPE" => 0x1b,
        "BACKSPACE" => 0x08,
        "DELETE" => 0x2e,
        "INSERT" => 0x2d,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "ARROWLEFT" | "LEFT" => 0x25,
        "ARROWUP" | "UP" => 0x26,
        "ARROWRIGHT" | "RIGHT" => 0x27,
        "ARROWDOWN" | "DOWN" => 0x28,
        "CAPSLOCK" => 0x14,
        "NUMLOCK" => 0x90,
        "SCROLLLOCK" => 0x91,
        "PAUSE" => 0x13,
        "PRINTSCREEN" => 0x2c,
        "SEMICOLON" | ";" => 0xba,
        "EQUAL" | "=" => 0xbb,
        "COMMA" | "," => 0xbc,
        "MINUS" | "-" => 0xbd,
        "PERIOD" | "." => 0xbe,
        "SLASH" | "/" => 0xbf,
        "BACKQUOTE" | "`" => 0xc0,
        "BRACKETLEFT" | "[" => 0xdb,
        "BACKSLASH" | "\\" => 0xdc,
        "BRACKETRIGHT" | "]" => 0xdd,
        "QUOTE" | "'" => 0xde,
        "NUMPADADD" => 0x6b,
        "NUMPADSUBTRACT" => 0x6d,
        "NUMPADMULTIPLY" => 0x6a,
        "NUMPADDIVIDE" => 0x6f,
        "NUMPADDECIMAL" => 0x6e,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
struct Chord(BTreeSet<u32>);
impl Chord {
    fn parse(value: &str) -> Result<Self, String> {
        let mut keys = BTreeSet::new();
        for token in value.split('+') {
            let key =
                key_code(token.trim()).ok_or_else(|| "Unsupported shortcut key".to_string())?;
            if !keys.insert(key) {
                return Err("Use each shortcut key only once".into());
            }
        }
        if keys.len() > 4 {
            return Err("Choose a shortcut with at most four keys".into());
        }
        let modifiers = keys
            .iter()
            .filter(|k| [CTRL, SHIFT, ALT, WIN].contains(k))
            .count();
        let main_keys = keys.len() - modifiers;
        if main_keys > 1 {
            return Err("Choose only one letter, number or other main key".into());
        }
        if main_keys == 0 && keys.len() < 2 {
            return Err("Use at least two modifier keys, such as Ctrl + Windows".into());
        }
        if main_keys == 1 && modifiers == 0 && !keys.iter().all(|key| (0x70..=0x87).contains(key)) {
            return Err("Add Ctrl, Alt, Shift or Windows, or choose a function key".into());
        }
        Ok(Self(keys))
    }
}

pub fn validate_shortcut(value: &str) -> Result<(), String> {
    Chord::parse(value).map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum ShortcutState {
    Pressed,
    Released,
}
#[derive(Clone, Serialize)]
struct ShortcutEvent {
    state: ShortcutState,
}

struct Tracker {
    chord: Chord,
    held: BTreeSet<u32>,
    pressed: bool,
    paused: bool,
    wait_for_release: bool,
}
impl Tracker {
    fn new(chord: Chord) -> Self {
        Self {
            chord,
            held: BTreeSet::new(),
            pressed: false,
            paused: false,
            wait_for_release: false,
        }
    }
    fn held_chord(&self) -> BTreeSet<u32> {
        self.held.iter().copied().map(normalized).collect()
    }
    fn key(&mut self, key: u32, down: bool) -> Option<ShortcutState> {
        let changed = if down {
            self.held.insert(key)
        } else {
            self.held.remove(&key)
        };
        if !changed {
            return None;
        }
        let held = self.held_chord();
        if self.wait_for_release && self.held.is_empty() {
            self.wait_for_release = false;
        }
        let matches = !self.paused && !self.wait_for_release && held == self.chord.0;
        if self.pressed && !matches {
            self.pressed = false;
            return Some(ShortcutState::Released);
        }
        // Releasing an unrelated key must never start a chord accidentally.
        if !self.pressed && down && matches {
            self.pressed = true;
            return Some(ShortcutState::Pressed);
        }
        None
    }
    fn reset(&mut self) -> Option<ShortcutState> {
        self.wait_for_release = !self.held.is_empty();
        if self.pressed {
            self.pressed = false;
            Some(ShortcutState::Released)
        } else {
            None
        }
    }
}

struct HookState {
    tracker: Tracker,
    events: mpsc::Sender<ShortcutState>,
}
impl HookState {
    fn send(&self, state: Option<ShortcutState>) {
        if let Some(state) = state {
            let _ = self.events.send(state);
        }
    }
}
static HOOK_STATE: OnceLock<Arc<Mutex<HookState>>> = OnceLock::new();

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if !event.flags.contains(LLKHF_INJECTED) {
            let message = wparam.0 as u32;
            if [WM_KEYDOWN, WM_SYSKEYDOWN, WM_KEYUP, WM_SYSKEYUP].contains(&message) {
                if let Some(state) = HOOK_STATE.get() {
                    if let Ok(mut state) = state.lock() {
                        let next = state.tracker.key(
                            event.vkCode,
                            message == WM_KEYDOWN || message == WM_SYSKEYDOWN,
                        );
                        state.send(next);
                    }
                }
            }
        }
    }
    // Passive hook: never block normal typing, Windows shortcuts or other hooks.
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

pub struct HotkeyService {
    state: Arc<Mutex<HookState>>,
    thread_id: u32,
}
impl HotkeyService {
    pub fn start(app: &AppHandle, shortcut: &str) -> Result<Self, String> {
        let chord = Chord::parse(shortcut)?;
        let (events, receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(HookState {
            tracker: Tracker::new(chord),
            events,
        }));
        HOOK_STATE
            .set(state.clone())
            .map_err(|_| "Shortcut listener is already initialized")?;
        let app = app.clone();
        thread::Builder::new()
            .name("dictation-shortcut-events".into())
            .spawn(move || {
                while let Ok(state) = receiver.recv() {
                    let _ = app.emit("dictation-shortcut", ShortcutEvent { state });
                }
            })
            .map_err(|_| "Could not start shortcut event delivery")?;
        let (started, ready) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("dictation-keyboard-hook".into())
            .spawn(move || unsafe {
                let thread_id = GetCurrentThreadId();
                let mut message = MSG::default();
                // Ensure PostThreadMessage has a message queue to address.
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                let hook = GetModuleHandleW(None).and_then(|module| {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), HINSTANCE(module.0), 0)
                });
                match hook {
                    Ok(hook) => {
                        let timer = SetTimer(None, 1, 500, None);
                        let _ = started.send(Ok(thread_id));
                        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                            if message.message == WM_TIMER && timer != 0 {
                                // Recover missing keyups after desktop changes, sleep or
                                // lock. This only releases known keys; polling cannot
                                // create a shortcut press or collect unrelated keys.
                                if let Some(state) = HOOK_STATE.get() {
                                    if let Ok(mut state) = state.lock() {
                                        let released: Vec<_> = state
                                            .tracker
                                            .held
                                            .iter()
                                            .copied()
                                            .filter(|key| GetAsyncKeyState(*key as i32) >= 0)
                                            .collect();
                                        for key in released {
                                            let next = state.tracker.key(key, false);
                                            state.send(next);
                                        }
                                    }
                                }
                            }
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                        if timer != 0 {
                            let _ = KillTimer(None, timer);
                        }
                        let _ = UnhookWindowsHookEx(hook);
                    }
                    Err(error) => {
                        let _ = started.send(Err(format!(
                            "Could not start keyboard shortcut listener: {error}"
                        )));
                    }
                }
            })
            .map_err(|_| "Could not start keyboard shortcut listener")?;
        let thread_id = ready
            .recv()
            .map_err(|_| "Keyboard shortcut listener did not start")??;
        Ok(Self { state, thread_id })
    }
    pub fn update(&self, shortcut: &str) -> Result<(), String> {
        let chord = Chord::parse(shortcut)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Shortcut listener is unavailable")?;
        if state.tracker.chord != chord {
            let release = state.tracker.reset();
            state.send(release);
            state.tracker.chord = chord;
        }
        Ok(())
    }
    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Shortcut listener is unavailable")?;
        if state.tracker.paused != paused {
            let release = state.tracker.reset();
            state.send(release);
            state.tracker.paused = paused;
        }
        Ok(())
    }
}
impl Drop for HotkeyService {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

#[tauri::command]
pub fn set_shortcut_capture(paused: bool, service: State<'_, HotkeyService>) -> Result<(), String> {
    service.set_paused(paused)
}
#[tauri::command]
pub fn validate_dictation_shortcut(shortcut: String) -> Result<(), String> {
    validate_shortcut(&shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tracker(value: &str) -> Tracker {
        Tracker::new(Chord::parse(value).unwrap())
    }
    #[test]
    fn validates_supported_chords() {
        for value in [
            "Ctrl+Super",
            "Ctrl+Windows+Space",
            "Ctrl+Alt+Space",
            "F12",
            "Ctrl+KeyK",
            "Alt+Digit1",
        ] {
            assert!(validate_shortcut(value).is_ok(), "{value}");
        }
        for value in [
            "",
            "Ctrl",
            "Ctrl+Ctrl",
            "Super+Win",
            "A",
            "Ctrl+A+B",
            "Ctrl+Invalid",
            "Ctrl++Space",
        ] {
            assert!(validate_shortcut(value).is_err(), "{value}");
        }
    }
    #[test]
    fn modifier_only_suppresses_repeat_and_releases_once() {
        let mut t = tracker("Ctrl+Super");
        assert_eq!(t.key(0xa2, true), None);
        assert_eq!(t.key(0x5c, true), Some(ShortcutState::Pressed));
        assert_eq!(t.key(0x5c, true), None);
        assert_eq!(t.key(0xa2, false), Some(ShortcutState::Released));
        assert_eq!(t.key(0x5c, false), None);
    }
    #[test]
    fn regular_chord_and_extra_keys() {
        let mut t = tracker("Ctrl+Alt+Space");
        t.key(0xa2, true);
        t.key(0xa4, true);
        assert_eq!(t.key(0x20, true), Some(ShortcutState::Pressed));
        assert_eq!(t.key(0x41, true), Some(ShortcutState::Released));
        assert_eq!(t.key(0x41, false), None);
    }
    #[test]
    fn both_modifier_sides_do_not_release_early() {
        let mut t = tracker("Ctrl+Super");
        t.key(0xa2, true);
        t.key(0xa3, true);
        assert_eq!(t.key(0x5b, true), Some(ShortcutState::Pressed));
        assert_eq!(t.key(0xa2, false), None);
        assert_eq!(t.key(0xa3, false), Some(ShortcutState::Released));
    }
    #[test]
    fn resume_waits_until_held_keys_are_released() {
        let mut t = tracker("Ctrl+Super");
        t.paused = true;
        t.key(0xa2, true);
        t.key(0x5b, true);
        t.reset();
        t.paused = false;
        assert_eq!(t.key(0xa2, false), None);
        assert_eq!(t.key(0x5b, false), None);
        t.key(0xa2, true);
        assert_eq!(t.key(0x5b, true), Some(ShortcutState::Pressed));
    }
}
