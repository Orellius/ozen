//! lib.rs: wires the menu-bar dictation loop and the dashboard commands.
//! Public surface: run() (the Tauri entry point).
//! Why this file: it is the imperative shell - it owns the tray, the global hotkey, app state, and the
//!   orchestration (arm -> record -> transcribe Hebrew -> translate to English -> paste -> learn). The
//!   heavy, testable pieces live in audio/whisper/translate/paste/hotkey/sound/store; this file only
//!   sequences them and decides when each audio cue fires.
//! NOT responsible for: audio capture, ASR, translation, pasting, hotkey mechanics, cue synthesis, or
//!   persistence.
//! Test strategy: launch, tap the hotkey, speak Hebrew, tap again; assert English lands in the focused
//!   app, five cues fire in order, the tray cycles idle->recording->transcribing->idle, and the entry
//!   survives a restart.

mod audio;
mod hotkey;
mod paste;
mod phonetics;
mod sound;
mod store;
mod translate;
mod whisper;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use audio::AudioHandle;
use sound::{Cue, SoundHandle};
use store::{Hints, LogEntry, Mishearing, Rejection, Settings, Store, Term};
use whisper::WhisperEngine;

const DEFAULT_MODEL: &str = "hf.co/dicta-il/DictaLM-3.0-Nemotron-12B-Instruct-GGUF:Q6_K";
/// The Hebrew "pre-warm": an extensive initial prompt biasing whisper toward Orel's actual
/// speech register - everyday Hebrew dev-speak with English tech terms in Latin script.
/// Whisper conditions on STYLE, so this is written as natural example speech, not a word
/// list. Budget: whisper reads at most ~224 tokens of initial prompt - this sits under it.
/// Override with OZEN_PROMPT (empty string disables).
const DEFAULT_WHISPER_PROMPT: &str = "הכתבה קולית של מפתח תוכנה ישראלי, עברית מדוברת עם \
הרבה סלנג. מונחים טכניים באנגלית: commit, push, branch, merge, rebase, repo, terminal, \
build, deploy, debug, test, refactor, function, endpoint, API, server, frontend, backend, \
Rust, TypeScript, Python, Tauri, React, bun, cargo, git, GitHub, Docker, macOS, Ollama, \
Claude. דוגמאות: יאללה, תעשה commit ותדחוף ל-branch. סבבה אחי, ה-build עבר, מעולה, פצצה. \
וואלה, ה-test נכשל, על הפנים, חבל על הזמן. רגע, שנייה, תריץ את זה שוב. נו, תכל'ס זה עובד, \
קדימה, זהו. אין מצב, בוא'נה, אחלה. תפתח את main.rs ותוסיף function שמחזירה Result. בוא \
נבדוק למה ה-server מחזיר שגיאה על ה-endpoint.";
/// The English-register counterpart to the Hebrew prompt above, used when whisper is told to
/// expect English. Written as accented dev-speech on purpose: whisper conditions on STYLE, so
/// showing it fast, clipped, Israeli-accented English is what biases it away from decoding
/// those clips as Hebrew or as generic newsreader English. Same ~224-token budget.
const DEFAULT_EN_PROMPT: &str = "Fast, casual English dictation by an Israeli software \
developer, spoken with a Hebrew accent and clipped articulation. Technical terms: commit, \
push, branch, merge, rebase, repo, terminal, build, deploy, debug, test, refactor, function, \
endpoint, API, server, frontend, backend, Rust, TypeScript, Python, Tauri, React, bun, cargo, \
git, GitHub, Docker, macOS, Ollama, Claude. Examples: okay so let's commit this and push it to \
the branch. The build passed, nice. Wait, the test is failing again, check why the server \
returns an error on that endpoint. Open main.rs and add a function that returns a Result. \
Let's just refactor the whole thing tomorrow.";
/// Auto mode cannot know the language before decoding, so it gets a short bilingual bias
/// rather than either full-register prompt - a Hebrew-only prompt actively pushes English
/// clips toward Hebrew output, which is the failure this mode exists to avoid.
const DEFAULT_AUTO_PROMPT: &str = "הכתבה של מפתח ישראלי, עברית או אנגלית, עם מונחים טכניים \
באנגלית. Dictation by an Israeli developer, Hebrew or English, with English technical terms: \
commit, push, branch, build, deploy, debug, test, refactor, function, endpoint, API, server, \
Rust, TypeScript, Tauri, React, bun, cargo, git, GitHub, Docker, macOS, Ollama, Claude.";
/// Corner radius of the orb tile. MUST equal `--radius` in `public/pill.html`: this clips the
/// glass material, that clips the light drawn over it, and a mismatch leaves tile corners with
/// no glass in them (which reads as a flat dark square, not as a wrong colour).
const PILL_RADIUS: f64 = 23.0;
const MIN_SAMPLES: usize = 1600; // ~0.1s at 16k; shorter is a fat-finger, not speech.
const RMS_FLOOR: f32 = 0.012; // below this the clip is silence/room noise.
const SAMPLE_RATE: f32 = 16_000.0;

// Whisper model is large; load it once, lazily, behind a OnceLock (preloaded at startup in a thread).
static ENGINE: OnceLock<Result<WhisperEngine, String>> = OnceLock::new();
fn whisper_engine() -> &'static Result<WhisperEngine, String> {
    ENGINE.get_or_init(WhisperEngine::load)
}

struct AppState {
    audio: AudioHandle,
    sound: SoundHandle,
    store: Arc<Store>,
    recording: AtomicBool,
    processing: AtomicBool,
    /// Wall-clock ms when the current recording armed - drives the toggle-mode runaway cap.
    armed_at: AtomicU64,
    ollama_model: String,
    /// Explicit override from OZEN_PROMPT. When unset the prompt is chosen per clip
    /// from the language mode, because a Hebrew-register prompt drags English clips to Hebrew.
    whisper_prompt: Option<String>,
}

impl AppState {
    fn settings(&self) -> Settings {
        self.store.settings()
    }
    fn prompt_for(&self, lang: &str) -> String {
        if let Some(p) = &self.whisper_prompt {
            return p.clone();
        }
        match lang {
            "en" => DEFAULT_EN_PROMPT.to_string(),
            "auto" => DEFAULT_AUTO_PROMPT.to_string(),
            _ => DEFAULT_WHISPER_PROMPT.to_string(),
        }
    }
    fn cue(&self, cue: Cue) {
        self.sound.play(cue);
    }
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
    model_ready: bool,
    model: String,
    settings: Settings,
    logs: Vec<LogEntry>,
    rejections: Vec<Rejection>,
    glossary: Vec<Term>,
    mishearings: Vec<Mishearing>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Did this clip come back as English rather than Hebrew? Decided by script, not by a language
/// tag: whisper's own detection is not exposed per-clip here, and the script of the output is
/// the thing that actually determines which repair the text needs. Latin-dominant wins, so a
/// mostly-English sentence carrying one Hebrew word still routes to the English repair.
fn is_latin_script(text: &str) -> bool {
    let mut latin = 0usize;
    let mut hebrew = 0usize;
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            latin += 1;
        } else if ('\u{0590}'..='\u{05FF}').contains(&c) {
            hebrew += 1;
        }
    }
    latin > hebrew && latin > 0
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Hesitation sounds. Whisper transcribes them faithfully, which is correct of it and useless
/// here: they are the sound of thinking, not of speaking. Matched after trailing punctuation is
/// removed, because they almost always arrive as "אה..." rather than bare.
/// Hebrew fillers only - the English ones ("um", "uh") are also real English words in context
/// ("um" never is, but "er"/"a" are), and the English path is a repair pass with grammatical
/// context, so it drops them by prompt instead.
const FILLERS: &[&str] = &[
    "אה", "אהה", "אההה", "אמ", "אמם", "אממ", "המ", "המם", "אוו", "אהם", "אמממ",
];

/// Leading punctuation that whisper emits when the clip's first phoneme is clipped or when it
/// continues the initial prompt's sentence. Measured 2026-08-06 in the live log: 22 of 66
/// transcripts began with ", " - and 7 of those commas survived translation into the pasted
/// English, which is what "it doesn't capitalize" actually looks like. The first character was
/// never a letter.
/// A leading hyphen is NOT stripped unless a space follows it: `--force` and `-m` are flags the
/// prompts promise to preserve verbatim, and eating those dashes is a far worse failure than
/// leaving a stray bullet dash in place. Caught by a test, not by review.
pub(crate) fn trim_leading_noise(s: &str) -> &str {
    let mut t = s;
    loop {
        let stripped = t.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '.' | '·' | '…' | ';' | ':' | '!' | '?' | '\u{05BE}')
        });
        // "- text" is a dictated bullet; "-m" is an argument.
        let stripped = match stripped.strip_prefix("- ") {
            Some(rest) => rest,
            None => stripped,
        };
        if stripped == t {
            return t;
        }
        t = stripped;
    }
}

/// Drop filler tokens and tidy the punctuation they leave behind. Done BEFORE the LLM, not
/// after: a filler inside the input makes the model build a sentence around it ("we can
/// system... use the buttons" in the live log), and no post-processing can unmake that.
fn strip_fillers(text: &str) -> String {
    let kept: Vec<&str> = text
        .split_whitespace()
        .filter(|w| {
            let core = w.trim_matches(|c: char| !c.is_alphanumeric());
            !FILLERS.contains(&core)
        })
        .collect();
    let joined = kept.join(" ");
    // Removing "אה..." from "לגבי הפורינג, אה... תבנה" leaves ", תבנה" hanging off the previous
    // clause; collapse the orphaned punctuation so the model is not handed a broken sentence.
    let mut out = String::with_capacity(joined.len());
    let mut prev_punct = false;
    for c in joined.chars() {
        let is_punct = matches!(c, ',' | ';' | '.' | '…');
        if is_punct && prev_punct {
            continue;
        }
        if is_punct && out.ends_with(' ') {
            out.pop();
        }
        prev_punct = is_punct;
        out.push(c);
    }
    out
}

/// Whisper invents a few stock Hebrew phrases on silence/noise (subtitle credits, "thank you").
/// The RMS floor catches most silence; this drops the rest so junk never reaches translation.
/// It also removes the two artifacts that are always noise and never speech: a leading comma
/// and hesitation sounds.
/// ORDER MATTERS: the noise is stripped BEFORE the blocklist is consulted, not after. The
/// blocklist is an exact-match list, and a hallucination that arrives as ", תודה רבה." is not
/// equal to "תודה רבה." - it walked straight through the gate. Caught by the test below, which
/// is the only reason this is written down rather than shipped.
fn clean_transcript(text: &str) -> String {
    let stripped = strip_fillers(text.trim());
    let t = trim_leading_noise(&stripped).trim_end();
    const EXACT: &[&str] = &["תודה", "תודה רבה", "תודה רבה.", "תודה.", "להתראות"];
    if EXACT.contains(&t) {
        return String::new();
    }
    if t.contains("כתוביות") || t.contains("הבא לכם") || t.contains("תרגום וכתוביות") {
        return String::new();
    }
    t.to_string()
}

/// Menu-bar frames: a dot plus 0-3 sound arcs, and a dimmed one-arc frame for idle. Template
/// images (black + alpha), so macOS recolours them for a light or dark menu bar by itself.
/// These are what make the menu bar say whether you are actually being HEARD - during a
/// recording the arc count tracks live mic level, so a dead mic or a too-quiet room is visible
/// at a glance instead of being discovered after the paste.
const TRAY_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const TRAY_LEVELS: [&[u8]; 4] = [
    include_bytes!("../icons/tray-0.png"),
    include_bytes!("../icons/tray-1.png"),
    include_bytes!("../icons/tray-2.png"),
    include_bytes!("../icons/tray-3.png"),
];

/// Live RMS -> arc count. Thresholds match the orb's `min(1, level * 9)` curve so the two
/// indicators never disagree about how loud you are.
fn arcs_for_level(level: f32) -> usize {
    let v = (level * 9.0).min(1.0);
    if v < 0.12 {
        0
    } else if v < 0.40 {
        1
    } else if v < 0.70 {
        2
    } else {
        3
    }
}

fn set_tray_icon(app: &AppHandle, bytes: &'static [u8]) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let (Some(tray), Ok(icon)) = (app.tray_by_id("main"), tauri::image::Image::from_bytes(bytes))
        {
            let _ = tray.set_icon(Some(icon));
        }
    });
}

fn set_tray(app: &AppHandle, s: TrayState) {
    let app_for_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app_for_main.tray_by_id("main") {
            // The icon carries the state now, so the title stays empty and the menu bar keeps
            // its width - a text glyph beside the icon was always a workaround for a static one.
            let (title, tip) = match s {
                TrayState::Idle => ("", "Ozen - מוכן"),
                TrayState::Recording => ("", "מקליט…"),
                TrayState::Transcribing => ("", "מתמלל…"),
                TrayState::Translating => ("", "מתרגם…"),
                TrayState::Error => ("⚠", "שגיאה - ראה לוח בקרה"),
            };
            let _ = tray.set_title(Some(title));
            let _ = tray.set_tooltip(Some(tip));
        }
    });
    match s {
        TrayState::Idle | TrayState::Error => set_tray_icon(app, TRAY_IDLE),
        TrayState::Recording => set_tray_icon(app, TRAY_LEVELS[0]),
        // Processing has no level to show, so the arcs sweep as a spinner instead (driven by
        // the thread in stop_recording).
        TrayState::Transcribing | TrayState::Translating => set_tray_icon(app, TRAY_LEVELS[1]),
    }
    let state_str = match s {
        TrayState::Idle => "idle",
        TrayState::Recording => "recording",
        TrayState::Transcribing => "transcribing",
        TrayState::Translating => "translating",
        TrayState::Error => "error",
    };
    let _ = app.emit("state", state_str);
}

fn emit_error(app: &AppHandle, st: &Arc<AppState>, msg: &str, reason: &str) {
    st.store.note_rejection(reason, now_ms());
    st.cue(Cue::Error);
    let _ = app.emit("error", msg.to_string());
}

/// The pill is ALWAYS visible (Orel's order, 2026-08-06): shown once at startup,
/// click-through, never hidden. State changes reach it via the same "state"/"level"
/// events the dashboard uses; it renders an equalizer, not text.
fn show_pill(app: &AppHandle) {
    // apply_vibrancy is main-thread-only (caught live 2026-08-06); dispatch the whole thing.
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || show_pill_on_main(&app));
}

fn show_pill_on_main(app: &AppHandle) {
    let Some(pill) = app.get_webview_window("pill") else {
        eprintln!("[pill] window 'pill' not found");
        return;
    };
    position_pill(&pill);
    // The Dock-style glass: a real NSVisualEffectView clipped to the SAME squircle radius the
    // page uses (`--radius` in pill.html). CSS backdrop-filter on a transparent Tauri window
    // paints a square halo - measured 2026-08-06 - so the material lives at the window layer.
    //
    // These two radii must agree. They did not in the first 0.4.0 build: the material was
    // still clipped to 36 (half of 72 - the radius that makes a CIRCLE, left over from the
    // round orb) while the page drew a 16px squircle over it. The visible result is a tile
    // whose corners contain no glass at all, which reads as flat and dark rather than as a
    // Dock tile, and it is easy to misread as "the material is wrong" instead of "the mask is".
    //
    // Two details are what make it read as Dock rather than as a grey disc:
    // - `Popover` is the milky menu/popover glass. `HudWindow` (used until 0.3.0) is the flat
    //   dark HUD material and never picks up the luminance behind it.
    // - `Active` is REQUIRED here. The default follows the window's active state, and this
    //   window is deliberately never key (focusable:false, so the synthetic Cmd+V always
    //   reaches the user's app) - so it would render in its INACTIVE appearance forever,
    //   which is exactly the flat, lifeless grey. Forced Active is what turns the blur on.
    #[cfg(target_os = "macos")]
    if let Err(e) = window_vibrancy::apply_vibrancy(
        &pill,
        window_vibrancy::NSVisualEffectMaterial::Popover,
        Some(window_vibrancy::NSVisualEffectState::Active),
        Some(PILL_RADIUS),
    ) {
        eprintln!("[pill] vibrancy failed: {e}");
    }
    // No click-through here on purpose: the orb is DRAGGABLE (mousedown starts a native
    // window drag), which requires receiving mouse events. It stays focusable:false, so
    // it can never become key and the synthetic Cmd+V always reaches the user's app.
    if let Err(e) = pill.show() {
        eprintln!("[pill] show failed: {e}");
    }
}

/// Top-center of the primary monitor, in logical coordinates (HiDPI-safe: the faked-Retina
/// panel reports scale 2.0 and logical positioning respects it).
fn position_pill(pill: &tauri::WebviewWindow) {
    let Ok(Some(mon)) = pill.primary_monitor() else {
        return;
    };
    let scale = mon.scale_factor();
    let screen = mon.size().to_logical::<f64>(scale);
    let origin = mon.position().to_logical::<f64>(scale);
    let width = pill
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale).width)
        .unwrap_or(72.0);
    let x = origin.x + (screen.width - width) / 2.0;
    let y = origin.y + 64.0;
    let _ = pill.set_position(tauri::LogicalPosition::new(x, y));
}

// ---- the two hotkey edges, dispatched by input mode ----

/// Hold mode records for exactly as long as the key is down. Toggle mode ignores the press
/// entirely and acts on a clean tap instead, so Right-⌘ keeps working as a modifier.
fn on_press(app: &AppHandle, st: &Arc<AppState>) {
    if st.settings().input_mode == "hold" {
        start_recording(app, st);
    }
}

fn on_release(app: &AppHandle, st: &Arc<AppState>, clean_tap: bool) {
    if st.settings().input_mode == "hold" {
        stop_recording(app, st);
    } else if clean_tap {
        if st.recording.load(Ordering::SeqCst) {
            stop_recording(app, st);
        } else {
            start_recording(app, st);
        }
    }
}

fn start_recording(app: &AppHandle, st: &Arc<AppState>) {
    if st.processing.load(Ordering::SeqCst) {
        return;
    }
    if st.recording.swap(true, Ordering::SeqCst) {
        return; // already recording
    }
    st.audio.start();
    st.cue(Cue::Start);
    st.armed_at.store(now_ms(), Ordering::SeqCst);
    set_tray(app, TrayState::Recording);

    // Feed the pill's equalizer at ~30Hz, and enforce the runaway cap from the same loop: in
    // toggle mode nothing else will ever stop a recording the user forgot about.
    let deadline_ms = st.settings().max_seconds.max(10) * 1000;
    let app = app.clone();
    let st = st.clone();
    std::thread::spawn(move || {
        let mut tick: u32 = 0;
        let mut shown_arcs = usize::MAX;
        while st.recording.load(Ordering::SeqCst) {
            let level = st.audio.level();
            let _ = app.emit("level", level);
            // Menu bar at ~8Hz, and only when the arc count actually changes: the orb wants
            // 30Hz to look smooth, but re-assigning an NSImage that often just to redraw the
            // same glyph is pure waste.
            tick += 1;
            if tick % 4 == 0 {
                let arcs = arcs_for_level(level);
                if arcs != shown_arcs {
                    shown_arcs = arcs;
                    set_tray_icon(&app, TRAY_LEVELS[arcs]);
                }
            }
            let elapsed = now_ms().saturating_sub(st.armed_at.load(Ordering::SeqCst));
            if elapsed >= deadline_ms {
                let _ = app.emit("error", "הגיע לזמן ההקלטה המרבי - נעצר".to_string());
                stop_recording(&app, &st);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        let _ = app.emit("level", 0.0f32);
    });
}

fn stop_recording(app: &AppHandle, st: &Arc<AppState>) {
    if !st.recording.swap(false, Ordering::SeqCst) {
        return; // wasn't recording
    }
    st.processing.store(true, Ordering::SeqCst);
    let samples = st.audio.stop();
    // Cue AFTER the stream is closed, so the tone can never bleed into the captured clip.
    st.cue(Cue::Stop);

    // Sweep the arcs while the models work. Whisper and DictaLM take seconds, and a frozen
    // menu-bar glyph during that is indistinguishable from a hung app.
    {
        let app = app.clone();
        let st = st.clone();
        std::thread::spawn(move || {
            let mut i = 1usize;
            while st.processing.load(Ordering::SeqCst) {
                set_tray_icon(&app, TRAY_LEVELS[i]);
                i = if i >= 3 { 1 } else { i + 1 };
                std::thread::sleep(std::time::Duration::from_millis(280));
            }
        });
    }

    let app = app.clone();
    let st = st.clone();
    std::thread::spawn(move || run_pipeline(app, st, samples));
}

fn run_pipeline(app: AppHandle, st: Arc<AppState>, samples: Vec<f32>) {
    let finish = |app: &AppHandle, st: &Arc<AppState>| {
        st.processing.store(false, Ordering::SeqCst);
        set_tray(app, TrayState::Idle);
    };
    let speech_ms = (samples.len() as f32 / SAMPLE_RATE * 1000.0) as u64;

    if samples.len() < MIN_SAMPLES {
        emit_error(&app, &st, "קצר מדי, נסה שוב", "short");
        finish(&app, &st);
        return;
    }
    if rms(&samples) < RMS_FLOOR {
        emit_error(&app, &st, "לא נשמע דיבור", "silent");
        finish(&app, &st);
        return;
    }

    let cfg = st.settings();
    let lang = cfg.speech_lang.as_str();
    set_tray(&app, TrayState::Transcribing);
    let asr_start = Instant::now();
    let hebrew = match whisper_engine() {
        Ok(engine) => match engine.transcribe(&samples, lang, &st.prompt_for(lang)) {
            Ok(t) => t,
            Err(e) => {
                emit_error(&app, &st, &format!("תמלול נכשל: {e}"), "asr");
                set_tray(&app, TrayState::Error);
                finish(&app, &st);
                return;
            }
        },
        Err(e) => {
            emit_error(&app, &st, &format!("טעינת מודל נכשלה: {e}"), "asr");
            set_tray(&app, TrayState::Error);
            finish(&app, &st);
            return;
        }
    };
    let asr_ms = asr_start.elapsed().as_millis() as u64;
    let (detected_lang, confidence) = (hebrew.lang.clone(), hebrew.confidence);
    let hebrew = clean_transcript(&hebrew.text);
    if hebrew.is_empty() {
        emit_error(&app, &st, "לא זוהה טקסט", "empty");
        finish(&app, &st);
        return;
    }
    // Confirmed mishearings are repaired BEFORE the model sees the text. These came from Orel
    // correcting the same word himself, so there is nothing left to decide - and fixing them
    // up front also stops the model from building a wrong sentence around a wrong word.
    let (hebrew, auto_fixed) = if cfg.dictionary {
        st.store.apply_known(&hebrew)
    } else {
        (hebrew, 0)
    };

    // What came back decides the route, not what was requested: in auto mode the clip may have
    // been English all along, and there is nothing to translate about English - but plenty to
    // repair, because Hebrew-accented English breaks in a small, predictable set of ways.
    let spoke_english = is_latin_script(&hebrew);
    let mut llm_ms = 0u64;
    let mut mode = "raw";
    let needs_llm = if spoke_english {
        cfg.accent_repair
    } else {
        cfg.translate || cfg.polish
    };
    let mut hints_used = 0usize;
    let english = if needs_llm {
        set_tray(&app, TrayState::Translating);
        st.cue(Cue::Working);
        // The learned dictionary, narrowed to the terms this sentence actually contains.
        let hints = if cfg.dictionary {
            st.store.hints_for(&hebrew)
        } else {
            Hints::default()
        };
        hints_used = hints.count();
        let llm_start = Instant::now();
        let result = if spoke_english {
            mode = "repair";
            translate::repair_english(&hebrew, &st.ollama_model, &hints)
        } else if cfg.translate {
            mode = "translate";
            translate::to_english(&hebrew, &st.ollama_model, &hints)
        } else {
            // Hebrew-out mode: same model cleans the transcript instead of translating it.
            mode = "polish";
            translate::polish_hebrew(&hebrew, &st.ollama_model, &hints)
        };
        llm_ms = llm_start.elapsed().as_millis() as u64;
        match result {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => hebrew.clone(),
            Err(e) => {
                // Don't strand the user: fall back to pasting the raw Hebrew, but tell them why.
                st.store.note_rejection("llm", now_ms());
                let _ = app.emit("error", format!("עיבוד נכשל (הודבק עברית גולמית): {e}"));
                mode = "raw";
                hebrew.clone()
            }
        }
    } else {
        hebrew.clone()
    };

    let pasted = paste::paste_text(&english);
    if let Err(e) = &pasted {
        emit_error(&app, &st, &format!("הדבקה נכשלה: {e}"), "paste");
    }

    let entry = LogEntry {
        at: now_ms(),
        hebrew: hebrew.clone(),
        english: english.clone(),
        corrected: None,
        speech_ms,
        asr_ms,
        llm_ms,
        mode: mode.to_string(),
        lang: detected_lang,
        confidence,
        hints_used,
        auto_fixed,
    };
    st.store.append_log(entry.clone());
    // Vocabulary is built from what was ACCEPTED, never from raw ASR - otherwise the first
    // mishearing enters the vocabulary and starts attracting correct words toward itself.
    st.store.note_vocab(&english);
    // Every produced pair feeds the aligner. It stays silent until a pairing repeats, so a
    // single bad translation cannot teach the dictionary anything (store.rs MIN_HITS).
    if cfg.dictionary && mode == "translate" {
        st.store.observe(&hebrew, &english);
    }
    if pasted.is_ok() {
        st.cue(Cue::Done);
    }
    let _ = app.emit("result", entry);
    finish(&app, &st);
}

// ---- dashboard commands ----

#[tauri::command]
fn get_state(state: State<'_, Arc<AppState>>) -> Snapshot {
    Snapshot {
        recording: state.recording.load(Ordering::SeqCst),
        processing: state.processing.load(Ordering::SeqCst),
        model_ready: ENGINE.get().map(|r| r.is_ok()).unwrap_or(false),
        model: state.ollama_model.clone(),
        settings: state.settings(),
        logs: state.store.logs(),
        rejections: state.store.rejections(),
        glossary: state.store.glossary(),
        mishearings: state.store.mishearings(),
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, Arc<AppState>>, settings: Settings) {
    state.sound.set_enabled(settings.sounds);
    state.sound.set_volume(settings.sound_volume);
    state.store.save_settings(settings.clone());
    let _ = app.emit("settings", settings);
}

/// Kept as its own command because the tray menu and the pill both flip only this one flag.
#[tauri::command]
fn set_translate(app: AppHandle, state: State<'_, Arc<AppState>>, enabled: bool) {
    let mut cfg = state.settings();
    cfg.translate = enabled;
    state.store.save_settings(cfg.clone());
    let _ = app.emit("settings", cfg);
}

#[tauri::command]
fn preview_sound(state: State<'_, Arc<AppState>>, cue: String) {
    let cue = match cue.as_str() {
        "start" => Cue::Start,
        "stop" => Cue::Stop,
        "working" => Cue::Working,
        "error" => Cue::Error,
        _ => Cue::Done,
    };
    // Preview must be audible even while cues are switched off - it is how you decide.
    state.sound.set_enabled(true);
    state.sound.play(cue);
    state.sound.set_enabled(state.settings().sounds);
}

/// Orel rewrote a translation. This is the only ground truth the dictionary ever gets.
#[tauri::command]
fn correct_entry(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    at: u64,
    corrected: String,
) -> bool {
    let ok = state.store.correct(at, corrected.trim());
    if ok {
        let _ = app.emit("glossary", state.store.glossary());
        let _ = app.emit("mishearings", state.store.mishearings());
    }
    ok
}

#[tauri::command]
fn set_term(app: AppHandle, state: State<'_, Arc<AppState>>, he: String, en: String) {
    state.store.set_term(he.trim(), en.trim());
    let _ = app.emit("glossary", state.store.glossary());
}

#[tauri::command]
fn forget_term(app: AppHandle, state: State<'_, Arc<AppState>>, he: String) {
    state.store.forget_term(he.trim());
    let _ = app.emit("glossary", state.store.glossary());
}

#[tauri::command]
fn clear_logs(state: State<'_, Arc<AppState>>) {
    state.store.clear_logs();
}

#[tauri::command]
fn forget_mishearing(app: AppHandle, state: State<'_, Arc<AppState>>, heard: String) {
    state.store.forget_mishearing(heard.trim());
    let _ = app.emit("mishearings", state.store.mishearings());
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
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            set_translate,
            preview_sound,
            correct_entry,
            set_term,
            forget_term,
            forget_mishearing,
            clear_logs,
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

            let state = build_state(app)?;
            app.manage(state.clone());
            build_tray(app)?;
            wire_hotkey(app, &state);

            // The pill is permanent: show it once the webview is up.
            let h = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(800));
                show_pill(&h);
            });

            // Preload the whisper model so the first dictation isn't a multi-second stall.
            let h = app.handle().clone();
            std::thread::spawn(move || {
                let ok = whisper_engine().is_ok();
                let _ = h.emit("model-ready", ok);
            });

            // Pre-warm DictaLM too: the first Ollama call otherwise pays a ~5s cold model
            // load exactly when the user is waiting for their first paste.
            let model = state.ollama_model.clone();
            std::thread::spawn(move || {
                if let Err(e) = translate::to_english("בדיקה", &model, &Hints::default()) {
                    eprintln!("[warm] dictalm pre-warm failed (first paste will be slow): {e}");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Build app state around the on-disk store. Env vars still win where they exist, so a debug
/// launch can override the model or the whisper prompt without touching saved settings.
fn build_state(app: &tauri::App) -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    let store = Arc::new(Store::load(dir));

    // A hotkey passed by env is a one-off override; persist it so the dashboard shows the truth.
    if let Ok(hk) = std::env::var("OZEN_HOTKEY") {
        let mut cfg = store.settings();
        cfg.hotkey = hk;
        store.save_settings(cfg);
    }
    if let Ok(v) = std::env::var("OZEN_POLISH") {
        let mut cfg = store.settings();
        cfg.polish = v != "0";
        store.save_settings(cfg);
    }

    let cfg = store.settings();
    Ok(Arc::new(AppState {
        audio: AudioHandle::spawn(),
        sound: SoundHandle::spawn(cfg.sounds, cfg.sound_volume),
        store,
        recording: AtomicBool::new(false),
        processing: AtomicBool::new(false),
        armed_at: AtomicU64::new(0),
        ollama_model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
        whisper_prompt: std::env::var("OZEN_PROMPT").ok(),
    }))
}

fn wire_hotkey(app: &tauri::App, state: &Arc<AppState>) {
    let handle = app.handle().clone();
    let press = {
        let h = handle.clone();
        let s = state.clone();
        move || on_press(&h, &s)
    };
    let release = {
        let h = handle.clone();
        let s = state.clone();
        move |clean_tap: bool| on_release(&h, &s, clean_tap)
    };
    let Err(e) = hotkey::install_hotkey(&state.settings().hotkey, press, release) else {
        return;
    };
    // First run before Accessibility is granted: prompt and tell the dashboard.
    hotkey::request_accessibility();
    // Emit a touch later so the webview has a listener attached.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = handle.emit("needs-accessibility", e);
    });
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

    let icon = tauri::image::Image::from_bytes(TRAY_IDLE)?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Ozen - מוכן")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_dashboard(app),
            "quit" => app.exit(0),
            "translate" => {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    let mut cfg = st.settings();
                    cfg.translate = !cfg.translate;
                    st.store.save_settings(cfg.clone());
                    let _ = app.emit("settings", cfg);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The live case, verbatim from the utterance log on 2026-08-06. 22 of 66 transcripts began
    /// with ", " and 7 of those commas reached the pasted English - which is what "it doesn't
    /// capitalize the first letter" actually is: the first character was never a letter.
    #[test]
    fn leading_comma_from_whisper_is_removed() {
        let raw = ", ואולי במקום להשתמש בפרומפטים עצמם, אפשר לעשות";
        let out = clean_transcript(raw);
        assert!(out.starts_with('ו'), "got {out:?}");
        // The comma INSIDE the sentence is real punctuation and must survive.
        assert!(out.contains("עצמם,"), "got {out:?}");
    }

    /// Fillers are dropped with the punctuation they drag along, and the clause they were sitting
    /// in does not end up with an orphaned comma.
    #[test]
    fn hesitations_are_dropped_without_breaking_punctuation() {
        let raw = "לגבי הפורינג, אה... תבנה את זה מחדש. אה... לא הבנתי";
        let out = clean_transcript(raw);
        assert!(!out.contains("אה"), "got {out:?}");
        assert!(!out.contains(".."), "got {out:?}");
        assert!(out.contains("הפורינג, תבנה"), "got {out:?}");
    }

    /// A word that merely CONTAINS a filler's letters is not a filler. "אהבתי" starts with אה.
    #[test]
    fn only_whole_filler_tokens_are_dropped() {
        let out = clean_transcript("אהבתי את זה, המשך");
        assert_eq!(out, "אהבתי את זה, המשך");
    }

    /// The hallucination blocklist still fires - this pass must not have widened a gate.
    #[test]
    fn silence_hallucinations_still_blocked() {
        assert_eq!(clean_transcript("תודה רבה."), "");
        assert_eq!(clean_transcript(", תודה רבה."), "");
    }
}
