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
    ) -> Result<String, String> {
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
        // Treat each push-to-talk clip as one utterance, and drop whisper's noise behaviors:
        params.set_single_segment(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true); // suppress non-speech tokens (music/noise/timestamp junk)
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0); // no fallback ladder -> no multi-second retries on hard/long audio
        params.set_no_speech_thold(0.6);

        state
            .full(params, samples)
            .map_err(|e| format!("whisper inference failed: {e}"))?;

        let mut text = String::new();
        for i in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };
            if segment.no_speech_probability() > NO_SPEECH_MAX {
                continue; // silence/noise segment - whisper would hallucinate here
            }
            let piece = segment
                .to_str_lossy()
                .map_err(|e| format!("segment {i} read failed: {e}"))?;
            text.push_str(&piece);
        }
        Ok(text.trim().to_string())
    }
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
