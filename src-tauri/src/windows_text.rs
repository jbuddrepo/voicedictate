use std::{thread, time::Duration};

use anyhow::{anyhow, Result};
use clipboard_win::{formats, get_clipboard, set_clipboard};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_V},
        WindowsAndMessaging::SetForegroundWindow,
    },
};

pub fn paste_text(target_window: isize, text: &str) -> Result<()> {
    let previous: Option<String> = get_clipboard(formats::Unicode).ok();
    set_clipboard(formats::Unicode, text).map_err(|error| anyhow!("Could not access the Windows clipboard: {error}"))?;
    unsafe {
        if target_window != 0 { let _ = SetForegroundWindow(HWND(target_window as *mut _)); }
    }
    thread::sleep(Duration::from_millis(90));
    send_ctrl_v()?;
    thread::sleep(Duration::from_millis(180));
    if let Some(value) = previous { let _ = set_clipboard(formats::Unicode, value); }
    Ok(())
}

fn send_ctrl_v() -> Result<()> {
    fn key(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
    }
    let inputs = [key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)), key(VK_V, KEYBD_EVENT_FLAGS(0)), key(VK_V, KEYEVENTF_KEYUP), key(VK_CONTROL, KEYEVENTF_KEYUP)];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 { return Err(anyhow!("Windows did not accept the paste command")); }
    Ok(())
}
