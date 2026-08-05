//! hotkey.rs: a global push-to-talk hotkey via a CGEventTap, firing on key/modifier press + release.
//! Public surface: is_accessibility_trusted(), request_accessibility(), install_hotkey(key, on_press, on_release).
//! Why this file (vs tauri-plugin-global-shortcut): push-to-talk needs SEPARATE press and release edges
//!   (hold to record, release to send); the global-shortcut plugin models discrete triggers, not hold.
//!   Ported from the old Electron native-core (napi callbacks swapped for plain Rust closures).
//! NOT responsible for: what happens on press/release (the caller wires recording + the pipeline).
//! Test strategy: with Accessibility granted, install on "cmd_r"; hold/release Right-Command; assert
//!   on_press then on_release fire exactly once each.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType, CallbackResult, EventField,
};

const KVK_RIGHT_COMMAND: i64 = 0x36;
const KVK_F5: i64 = 0x60;
const KVK_F6: i64 = 0x61;
const KVK_CONTROL: i64 = 0x3B;

const FLAG_RIGHT_COMMAND: u64 = 0x10;
const FLAG_CONTROL: u64 = 0x40000;

#[derive(Clone, Copy)]
enum HotkeyKind {
    Modifier(u64),
    Key,
}

#[derive(Clone, Copy)]
struct HotkeyConfig {
    keycode: i64,
    kind: HotkeyKind,
}

fn config_for(key: &str) -> Option<HotkeyConfig> {
    match key {
        "cmd_r" => Some(HotkeyConfig {
            keycode: KVK_RIGHT_COMMAND,
            kind: HotkeyKind::Modifier(FLAG_RIGHT_COMMAND),
        }),
        "ctrl" => Some(HotkeyConfig {
            keycode: KVK_CONTROL,
            kind: HotkeyKind::Modifier(FLAG_CONTROL),
        }),
        "f5" => Some(HotkeyConfig {
            keycode: KVK_F5,
            kind: HotkeyKind::Key,
        }),
        "f6" => Some(HotkeyConfig {
            keycode: KVK_F6,
            kind: HotkeyKind::Key,
        }),
        _ => None,
    }
}

extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
}

pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Prompt the user to grant Accessibility (opens the System Settings pane). Returns current trust.
pub fn request_accessibility() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const _)
    }
}

pub fn install_hotkey<P, R>(key: &str, on_press: P, on_release: R) -> Result<(), String>
where
    P: Fn() + Send + 'static,
    R: Fn() + Send + 'static,
{
    if !is_accessibility_trusted() {
        return Err("Accessibility permission required (System Settings -> Privacy & Security -> Accessibility).".to_string());
    }

    let cfg = config_for(key).ok_or_else(|| format!("unknown hotkey: {key}"))?;
    let event_types = match cfg.kind {
        HotkeyKind::Modifier(_) => vec![CGEventType::FlagsChanged],
        HotkeyKind::Key => vec![CGEventType::KeyDown, CGEventType::KeyUp],
    };
    log(&format!("install_hotkey key={key} keycode={:#x}", cfg.keycode));

    thread::spawn(move || {
        let port_holder: Arc<AtomicPtr<c_void>> = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let port_holder_cb = port_holder.clone();
        let pressed: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let pressed_cb = pressed.clone();

        let cb = move |_proxy: CGEventTapProxy, etype: CGEventType, event: &CGEvent| -> CallbackResult {
            if matches!(
                etype,
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
            ) {
                let p = port_holder_cb.load(Ordering::SeqCst);
                if !p.is_null() {
                    unsafe { CGEventTapEnable(p, true) };
                }
                log(&format!("tap re-enabled after {etype:?}"));
                return CallbackResult::Keep;
            }
            let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
            if keycode != cfg.keycode {
                return CallbackResult::Keep;
            }
            match (cfg.kind, etype) {
                (HotkeyKind::Modifier(bit), CGEventType::FlagsChanged) => {
                    let flags = event.get_flags().bits();
                    let now_pressed = (flags & bit) != 0;
                    let was_pressed = pressed_cb.swap(now_pressed, Ordering::SeqCst);
                    if now_pressed && !was_pressed {
                        on_press();
                    } else if !now_pressed && was_pressed {
                        on_release();
                    }
                }
                (HotkeyKind::Key, CGEventType::KeyDown) => {
                    if !pressed_cb.swap(true, Ordering::SeqCst) {
                        on_press();
                    }
                }
                (HotkeyKind::Key, CGEventType::KeyUp) => {
                    if pressed_cb.swap(false, Ordering::SeqCst) {
                        on_release();
                    }
                }
                _ => {}
            }
            CallbackResult::Keep
        };

        let tap = match CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            event_types,
            cb,
        ) {
            Ok(t) => t,
            Err(_) => {
                log("CGEventTap::new failed");
                return;
            }
        };

        let mp = tap.mach_port();
        port_holder.store(mp.as_concrete_TypeRef() as *mut c_void, Ordering::SeqCst);

        let source = match mp.create_runloop_source(0) {
            Ok(s) => s,
            Err(_) => {
                log("create_runloop_source failed");
                return;
            }
        };
        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        log("runloop entering");
        CFRunLoop::run_current();
    });

    Ok(())
}

fn log(msg: &str) {
    if std::env::var("ORELLIUS_STT_DEBUG").is_err() {
        return;
    }
    if let Ok(mut f) = OpenOptions::new()
        .append(true)
        .create(true)
        .open("/tmp/orellius-stt-hotkey.log")
    {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(f, "{ts} {msg}");
    }
}
