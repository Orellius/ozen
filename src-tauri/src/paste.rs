//! paste.rs: drop text into whatever app is focused via the clipboard + a synthetic Cmd+V.
//! Public surface: paste_text(text) -> Result<()>.
//! Why this file (vs CGEvent set_string typing): clipboard+paste is the reliable path in terminals
//!   and TUIs (bracketed paste, long strings) where char-by-char synthetic typing drops/mis-orders.
//!   The previous clipboard contents are saved and restored so we don't clobber the user's clipboard.
//! NOT responsible for: deciding WHAT to paste, focus management, or the hotkey.
//! Test strategy: focus a text field, call paste_text("hello"), assert it appears and clipboard restored.

use std::thread::sleep;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KVK_ANSI_V: u16 = 0x09;

pub fn paste_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }

    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard open: {e}"))?;
    let previous = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("clipboard set: {e}"))?;

    // Let the pasteboard settle before the keystroke, and let the target consume it before restore.
    sleep(Duration::from_millis(120));
    press_cmd_v()?;
    sleep(Duration::from_millis(150));

    if let Some(prev) = previous {
        let _ = clipboard.set_text(prev);
    }
    Ok(())
}

fn press_cmd_v() -> Result<(), String> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "CGEventSource create failed".to_string())?;

    let down = CGEvent::new_keyboard_event(source.clone(), KVK_ANSI_V, true)
        .map_err(|_| "keydown create failed".to_string())?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);

    let up = CGEvent::new_keyboard_event(source, KVK_ANSI_V, false)
        .map_err(|_| "keyup create failed".to_string())?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    up.post(CGEventTapLocation::HID);

    Ok(())
}
