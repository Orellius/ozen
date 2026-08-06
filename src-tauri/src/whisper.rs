//! whisper.rs: on-device Hebrew speech-to-text via whisper-rs + Metal (flash attention).
//! Public surface: WhisperEngine::load() -> Result<Self>, transcribe(&self, samples, lang, prompt) -> Result<String>.
//! Why this file (vs inlining in lib.rs): isolates the whisper.cpp FFI plus the one-time model load so the
//!   Tauri command layer stays thin and the heavy WhisperContext is created exactly once and reused.
//! NOT responsible for: audio capture (audio.rs owns the native cpal path) or resampling (samples arrive 16k mono f32).
//! Test strategy: feed a known 16k clip's f32 samples, assert non-empty trimmed Hebrew text.

use std::path::PathBuf;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Above this per-segment no-speech probability the segment is treated as silence/noise,
/// killing whisper's stock hallucinations (תודה רבה / כתוביות) at the source instead of
/// string-matching them after the fact.
const NO_SPEECH_MAX: f32 = 0.5;

pub struct WhisperEngine {
    ctx: Mutex<WhisperContext>,
}

impl WhisperEngine {
    pub fn load() -> Result<Self, String> {
        // Route whisper.cpp + GGML logs into the `log` crate; with no logger configured
        // (we enable neither the `log` nor `tracing` feature) this silences their stderr spam.
        whisper_rs::install_logging_hooks();
        let path = resolve_model_path()?;
        let mut ctx_params = WhisperContextParameters::default();
        // Flash attention: faster Metal decode, which is what makes beam search affordable
        // on push-to-talk latency. Incompatible with DTW token timestamps (unused here).
        ctx_params.flash_attn(true);
        let ctx = WhisperContext::new_with_params(
            path.to_str().ok_or("model path is not valid UTF-8")?,
            ctx_params,
        )
        .map_err(|e| format!("whisper model load failed: {e}"))?;
        Ok(Self {
            ctx: Mutex::new(ctx),
        })
    }

    pub fn transcribe(
        &self,
        samples: &[f32],
        language: &str,
        initial_prompt: &str,
    ) -> Result<Transcript, String> {
        let ctx = self
            .ctx
            .lock()
            .map_err(|_| "whisper context lock poisoned".to_string())?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("whisper state create failed: {e}"))?;

        // Beam search over greedy: the standard accuracy bump for Hebrew, affordable on
        // short push-to-talk clips with flash attention on.
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });
        params.set_language(Some(language));
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8) as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);
        params.set_print_realtime(false);
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        // Bias decoding toward the operator's speech domain (Hebrew dev-speak laced with
        // English tech terms) - this is where mixed he/en clips otherwise get mangled.
        if !initial_prompt.is_empty() {
            params.set_initial_prompt(initial_prompt);
        }
        // Drop whisper's noise behaviors, but KEEP the default temperature fallback ladder:
        // measured 2026-08-06, the "precise" Electron build's whole decode config was whisper
        // defaults - the ladder retries low-confidence decodes at higher temperatures, and
        // disabling it (as this port originally did for latency) is a precision loss exactly
        // on the hard clips. Same reasoning for NOT forcing single_segment.
        params.set_suppress_blank(true);
        params.set_suppress_nst(true); // suppress non-speech tokens (music/noise/timestamp junk)
        params.set_temperature(0.0);
        params.set_no_speech_thold(0.6);

        // Trim leading/trailing silence BEFORE normalising: silence is where whisper invents
        // its stock phrases, so removing the habitat beats filtering the output. Peak-normalize
        // after, because the raw cpal path has no AGC (the old Electron pipeline got Chrome's
        // autoGainControl for free) and a quiet mic starves whisper.
        let trimmed = trim_silence(samples);
        let samples = normalize_peak(trimmed);

        state
            .full(params, &samples)
            .map_err(|e| format!("whisper inference failed: {e}"))?;

        let mut text = String::new();
        // Mean token probability across kept segments. whisper-rs does not expose avg_logprob,
        // but per-token probability carries the same information: a low mean means the decoder
        // was guessing, which is the signature of a misheard word or an invented one.
        let mut prob_sum = 0.0f64;
        let mut prob_n = 0usize;
        for i in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };
            if segment.no_speech_probability() > NO_SPEECH_MAX {
                continue; // silence/noise segment - whisper would hallucinate here
            }
            for k in 0..segment.n_tokens() {
                if let Some(tok) = segment.get_token(k) {
                    prob_sum += tok.token_probability() as f64;
                    prob_n += 1;
                }
            }
            let piece = segment
                .to_str_lossy()
                .map_err(|e| format!("segment {i} read failed: {e}"))?;
            text.push_str(&piece);
        }

        let lang_id = state.full_lang_id_from_state();
        Ok(Transcript {
            text: text.trim().to_string(),
            lang: whisper_rs::get_lang_str(lang_id)
                .unwrap_or("??")
                .to_string(),
            confidence: if prob_n > 0 {
                (prob_sum / prob_n as f64) as f32
            } else {
                0.0
            },
        })
    }
}

/// One decode, with the two numbers that say how much to trust it.
pub struct Transcript {
    pub text: String,
    /// What whisper decided the clip was, e.g. "he" or "en". In auto mode this is the only
    /// place the decision is visible at all - without it a misdetected clip silently gets the
    /// wrong downstream repair and there is no way to find out why the output was odd.
    pub lang: String,
    /// Mean per-token probability, 0..1. Low means the decoder was guessing.
    pub confidence: f32,
}

/// Drop leading and trailing near-silence, keeping a short pad so word onsets survive.
///
/// The threshold is derived from the clip's own quietest stretch rather than being a constant:
/// a fixed floor either eats speech in a quiet room or keeps noise in a loud one, and the whole
/// point is to work in both.
fn trim_silence(samples: &[f32]) -> &[f32] {
    const WIN: usize = 320; // 20ms at 16k
    const PAD_WINDOWS: usize = 5; // 100ms of context kept on each side
    if samples.len() < WIN * 4 {
        return samples;
    }
    let energies: Vec<f32> = samples
        .chunks(WIN)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak = energies.iter().fold(0.0f32, |m, e| m.max(*e));
    let floor = energies.iter().fold(f32::MAX, |m, e| m.min(*e));
    if peak <= 0.0 {
        return samples;
    }
    // Speech is anything meaningfully above the quietest window, but never below an absolute
    // floor - otherwise a clip of pure silence "finds speech" in its own noise.
    let thold = (floor + (peak - floor) * 0.12).max(0.006);
    let first = energies.iter().position(|e| *e > thold);
    let last = energies.iter().rposition(|e| *e > thold);
    let (Some(first), Some(last)) = (first, last) else {
        return samples;
    };
    let start = first.saturating_sub(PAD_WINDOWS) * WIN;
    let end = ((last + PAD_WINDOWS + 1) * WIN).min(samples.len());
    if end <= start {
        return samples;
    }
    &samples[start..end]
}

/// Scale the clip so its peak sits at ~0.95 - a poor man's AGC for the raw cpal path.
/// The silence guard keeps room noise from being amplified into fake speech; the RMS
/// floor in lib.rs already rejected silent clips on the RAW samples before this runs.
fn normalize_peak(samples: &[f32]) -> Vec<f32> {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak < 1e-3 || peak > 0.90 {
        return samples.to_vec();
    }
    let gain = 0.95 / peak;
    samples.iter().map(|s| s * gain).collect()
}

/// Locate the ivrit-ai GGML model. Override with WHISPER_MODEL_PATH; otherwise scan the
/// HuggingFace cache snapshot dirs for the first ggml-model.bin.
fn resolve_model_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("WHISPER_MODEL_PATH") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let base = PathBuf::from(home).join(
        ".cache/huggingface/hub/models--ivrit-ai--whisper-large-v3-turbo-ggml/snapshots",
    );
    let entries = std::fs::read_dir(&base).map_err(|e| format!("cannot read {base:?}: {e}"))?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("ggml-model.bin");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "ivrit ggml model not found under {base:?}; set WHISPER_MODEL_PATH"
    ))
}
