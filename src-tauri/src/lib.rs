//! lib.rs: wires the menu-bar push-to-talk loop and the dashboard commands.
//! Public surface: run() (the Tauri entry point).
//! Why this file: it is the imperative shell - it owns the tray, the global hotkey, app state, and the
//!   orchestration (hold -> record -> transcribe Hebrew -> translate to English -> paste). The heavy,
//!   testable pieces live in audio/whisper/translate/paste/hotkey; this file only sequences them.
//! NOT responsible for: audio capture, ASR, translation, pasting, or hotkey mechanics.
//! Test strategy: launch, hold the hotkey, speak Hebrew, release; assert English lands in the focused
//!   app, the tray cycles idle->recording->transcribing->idle, and the dashboard shows the pair.

mod audio;
mod hotkey;
mod paste;
mod translate;
mod whisper;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use audio::AudioHandle;
use whisper::WhisperEngine;

const DEFAULT_MODEL: &str = "hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K";
const DEFAULT_HOTKEY: &str = "cmd_r";
/// Biases whisper decoding toward Orel's speech domain: Hebrew dev-speak with English tech
/// terms kept in Latin script. Override with ORELLIUS_STT_PROMPT (empty string disables).
const DEFAULT_WHISPER_PROMPT: &str = "שיחה טכנית על פיתוח תוכנה. מונחים טכניים נשארים באנגלית: \
commit, push, branch, merge, repo, terminal, build, deploy, test, debug, Rust, TypeScript, \
Tauri, bun, קובץ, פונקציה, שרת, תיקייה.";
const MIN_SAMPLES: usize = 1600; // ~0.1s at 16k; shorter is a fat-finger, not speech.
const RMS_FLOOR: f32 = 0.012; // below this the clip is silence/room noise.
const HISTORY_CAP: usize = 30;

// Whisper model is large; load it once, lazily, behind a OnceLock (preloaded at startup in a thread).
static ENGINE: OnceLock<Result<WhisperEngine, String>> = OnceLock::new();
fn whisper_engine() -> &'static Result<WhisperEngine, String> {
    ENGINE.get_or_init(WhisperEngine::load)
}

#[derive(Clone, Serialize)]
struct Entry {
    hebrew: String,
    english: String,
    at: u64,
}

struct AppState {
    audio: AudioHandle,
    recording: AtomicBool,
    processing: AtomicBool,
    translate_enabled: AtomicBool,
    polish_enabled: AtomicBool,
    ollama_model: String,
    hotkey: String,
    whisper_prompt: String,
    history: Mutex<VecDeque<Entry>>,
}

#[derive(Clone, Copy)]
enum TrayState {
    Idle,
    Recording,
    Transcribing,
    Translating,
    Error,
}

#[derive(Serialize)]
struct Snapshot {
    recording: bool,
    processing: bool,
    translate_enabled: bool,
    model_ready: bool,
    hotkey: String,
    model: String,
    history: Vec<Entry>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Whisper invents a few stock Hebrew phrases on silence/noise (subtitle credits, "thank you").
/// The RMS floor catches most silence; this drops the rest so junk never reaches translation.
fn clean_transcript(text: &str) -> String {
    let t = text.trim();
    const EXACT: &[&str] = &["תודה", "תודה רבה", "תודה רבה.", "תודה.", "להתראות"];
    if EXACT.contains(&t) {
        return String::new();
    }
    if t.contains("כתוביות") || t.contains("הבא לכם") || t.contains("תרגום וכתוביות") {
        return String::new();
    }
    t.to_string()
}

fn set_tray(app: &AppHandle, s: TrayState) {
    let app_for_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app_for_main.tray_by_id("main") {
            let (title, tip) = match s {
                TrayState::Idle => ("", "Orellius STT - מוכן"),
                TrayState::Recording => ("●", "מקליט…"),
                TrayState::Transcribing => ("⠿", "מתמלל…"),
                TrayState::Translating => ("⠿", "מתרגם…"),
                TrayState::Error => ("⚠", "שגיאה - ראה לוח בקרה"),
            };
            let _ = tray.set_title(Some(title));
            let _ = tray.set_tooltip(Some(tip));
        }
    });
    let state_str = match s {
        TrayState::Idle => "idle",
        TrayState::Recording => "recording",
        TrayState::Transcribing => "transcribing",
        TrayState::Translating => "translating",
        TrayState::Error => "error",
    };
    let _ = app.emit("state", state_str);
}

fn emit_error(app: &AppHandle, msg: &str) {
    let _ = app.emit("error", msg.to_string());
}

fn on_press(app: &AppHandle, st: &Arc<AppState>) {
    if st.processing.load(Ordering::SeqCst) {
        return;
    }
    if st.recording.swap(true, Ordering::SeqCst) {
        return; // already recording
    }
    st.audio.start();
    set_tray(app, TrayState::Recording);
}

fn on_release(app: &AppHandle, st: &Arc<AppState>) {
    if !st.recording.swap(false, Ordering::SeqCst) {
        return; // wasn't recording
    }
    st.processing.store(true, Ordering::SeqCst);
    let samples = st.audio.stop();
    let app = app.clone();
    let st = st.clone();
    std::thread::spawn(move || run_pipeline(app, st, samples));
}

fn run_pipeline(app: AppHandle, st: Arc<AppState>, samples: Vec<f32>) {
    let finish = |app: &AppHandle, st: &Arc<AppState>| {
        st.processing.store(false, Ordering::SeqCst);
        set_tray(app, TrayState::Idle);
    };

    if samples.len() < MIN_SAMPLES {
        emit_error(&app, "קצר מדי, נסה שוב");
        finish(&app, &st);
        return;
    }
    if rms(&samples) < RMS_FLOOR {
        emit_error(&app, "לא נשמע דיבור");
        finish(&app, &st);
        return;
    }

    set_tray(&app, TrayState::Transcribing);
    let hebrew = match whisper_engine() {
        Ok(engine) => match engine.transcribe(&samples, "he", &st.whisper_prompt) {
            Ok(t) => t,
            Err(e) => {
                emit_error(&app, &format!("תמלול נכשל: {e}"));
                set_tray(&app, TrayState::Error);
                finish(&app, &st);
                return;
            }
        },
        Err(e) => {
            emit_error(&app, &format!("טעינת מודל נכשלה: {e}"));
            set_tray(&app, TrayState::Error);
            finish(&app, &st);
            return;
        }
    };
    let hebrew = clean_transcript(&hebrew);
    if hebrew.is_empty() {
        emit_error(&app, "לא זוהה טקסט");
        finish(&app, &st);
        return;
    }

    let translate_on = st.translate_enabled.load(Ordering::SeqCst);
    let english = if translate_on || st.polish_enabled.load(Ordering::SeqCst) {
        set_tray(&app, TrayState::Translating);
        let result = if translate_on {
            translate::to_english(&hebrew, &st.ollama_model)
        } else {
            // Hebrew-out mode: same model cleans the transcript instead of translating it.
            translate::polish_hebrew(&hebrew, &st.ollama_model)
        };
        match result {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => hebrew.clone(),
            Err(e) => {
                // Don't strand the user: fall back to pasting the raw Hebrew, but tell them why.
                emit_error(&app, &format!("עיבוד נכשל (הודבק עברית גולמית): {e}"));
                hebrew.clone()
            }
        }
    } else {
        hebrew.clone()
    };

    if let Err(e) = paste::paste_text(&english) {
        emit_error(&app, &format!("הדבקה נכשלה: {e}"));
    }

    let entry = Entry {
        hebrew,
        english,
        at: now_ms(),
    };
    if let Ok(mut h) = st.history.lock() {
        h.push_front(entry.clone());
        while h.len() > HISTORY_CAP {
            h.pop_back();
        }
    }
    let _ = app.emit("result", entry);
    finish(&app, &st);
}

#[tauri::command]
fn get_state(state: State<'_, Arc<AppState>>) -> Snapshot {
    let history = state
        .history
        .lock()
        .map(|h| h.iter().cloned().collect())
        .unwrap_or_default();
    Snapshot {
        recording: state.recording.load(Ordering::SeqCst),
        processing: state.processing.load(Ordering::SeqCst),
        translate_enabled: state.translate_enabled.load(Ordering::SeqCst),
        model_ready: ENGINE.get().map(|r| r.is_ok()).unwrap_or(false),
        hotkey: state.hotkey.clone(),
        model: state.ollama_model.clone(),
        history,
    }
}

#[tauri::command]
fn set_translate(app: AppHandle, state: State<'_, Arc<AppState>>, enabled: bool) {
    state.translate_enabled.store(enabled, Ordering::SeqCst);
    let _ = app.emit("translate", enabled);
}

#[tauri::command]
fn request_accessibility() -> bool {
    hotkey::request_accessibility()
}

/// Trigger the macOS microphone permission prompt by briefly opening + closing the input stream.
#[tauri::command]
fn request_microphone(state: State<'_, Arc<AppState>>) {
    state.audio.start();
    std::thread::sleep(std::time::Duration::from_millis(250));
    let _ = state.audio.stop();
}

fn show_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let hk = std::env::var("ORELLIUS_STT_HOTKEY").unwrap_or_else(|_| DEFAULT_HOTKEY.to_string());
    let prompt = std::env::var("ORELLIUS_STT_PROMPT")
        .unwrap_or_else(|_| DEFAULT_WHISPER_PROMPT.to_string());

    let state = Arc::new(AppState {
        audio: AudioHandle::spawn(),
        recording: AtomicBool::new(false),
        processing: AtomicBool::new(false),
        translate_enabled: AtomicBool::new(true),
        // Hebrew-out polish (DictaLM cleanup when translate is off). ORELLIUS_STT_POLISH=0 disables.
        polish_enabled: AtomicBool::new(
            std::env::var("ORELLIUS_STT_POLISH").map_or(true, |v| v != "0"),
        ),
        ollama_model: model,
        hotkey: hk,
        whisper_prompt: prompt,
        history: Mutex::new(VecDeque::new()),
    });

    tauri::Builder::default()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_translate,
            request_accessibility,
            request_microphone
        ])
        .on_window_event(|window, event| {
            // Keep the app alive in the menu bar: hide the dashboard instead of destroying it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            build_tray(app)?;

            // Wire the global push-to-talk hotkey to the record/pipeline loop.
            let handle = app.handle().clone();
            let st = state.clone();
            let press = {
                let h = handle.clone();
                let s = st.clone();
                move || on_press(&h, &s)
            };
            let release = {
                let h = handle.clone();
                let s = st.clone();
                move || on_release(&h, &s)
            };
            if let Err(e) = hotkey::install_hotkey(&state.hotkey, press, release) {
                // First run before Accessibility is granted: prompt and tell the dashboard.
                hotkey::request_accessibility();
                let h = handle.clone();
                let msg = e.clone();
                // Emit a touch later so the webview has a listener attached.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let _ = h.emit("needs-accessibility", msg);
                });
            }

            // Preload the whisper model so the first push-to-talk isn't a multi-second stall.
            let h = app.handle().clone();
            std::thread::spawn(move || {
                let ok = whisper_engine().is_ok();
                let _ = h.emit("model-ready", ok);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "פתח לוח בקרה", true, None::<&str>)?;
    let translate_i = MenuItem::with_id(
        app,
        "translate",
        "החלף תרגום (עברית→אנגלית)",
        true,
        None::<&str>,
    )?;
    let quit_i = MenuItem::with_id(app, "quit", "יציאה", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open_i,
            &PredefinedMenuItem::separator(app)?,
            &translate_i,
            &PredefinedMenuItem::separator(app)?,
            &quit_i,
        ],
    )?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Orellius STT - מוכן")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_dashboard(app),
            "quit" => app.exit(0),
            "translate" => {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    let next = !st.translate_enabled.load(Ordering::SeqCst);
                    st.translate_enabled.store(next, Ordering::SeqCst);
                    let _ = app.emit("translate", next);
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_dashboard(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
