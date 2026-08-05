//! audio.rs: native push-to-talk microphone capture via cpal, decoupled from the webview.
//! Public surface: AudioHandle::spawn() -> AudioHandle; .start(); .stop() -> Vec<f32> (16k mono).
//! Why this file (vs capturing in the webview like the old orellius-voice): a menu-bar push-to-talk
//!   tool records while OTHER apps are focused and the dashboard window is hidden, so the audio path
//!   must not depend on a focused/alive webview holding a getUserMedia stream.
//! NOT responsible for: transcription, the hotkey that triggers it, or resampling quality beyond a
//!   plain averaging downsample (whisper is forgiving on short clips).
//! Test strategy: start(), speak, stop(); assert a non-empty 16k buffer with plausible length.
//!
//! cpal's Stream is !Send on macOS (it wraps an AudioUnit), so the stream is created, owned, and
//! dropped entirely on one dedicated audio thread. Other threads talk to it only over a channel.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

const TARGET_RATE: u32 = 16_000;

enum Cmd {
    Start,
    Stop(Sender<Vec<f32>>),
}

pub struct AudioHandle {
    tx: Sender<Cmd>,
    /// Live capture level (RMS of the latest callback chunk), f32 stored as bits.
    /// Written by the audio callback, read by the UI level emitter - no locks.
    level: Arc<AtomicU32>,
}

impl AudioHandle {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let level = Arc::new(AtomicU32::new(0));
        let level_loop = level.clone();
        thread::spawn(move || audio_loop(rx, level_loop));
        AudioHandle { tx, level }
    }

    pub fn start(&self) {
        let _ = self.tx.send(Cmd::Start);
    }

    /// Stop capture and return the recorded audio resampled to 16k mono f32.
    pub fn stop(&self) -> Vec<f32> {
        self.level.store(0, Ordering::Relaxed);
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.tx.send(Cmd::Stop(reply_tx)).is_err() {
            return Vec::new();
        }
        reply_rx.recv().unwrap_or_default()
    }

    /// Instantaneous mic level (0.0..~1.0) while recording; 0.0 when idle.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// Live capture state held only on the audio thread (the !Send Stream never leaves it).
struct Active {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

fn audio_loop(rx: Receiver<Cmd>, level: Arc<AtomicU32>) {
    let mut active: Option<Active> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Start => {
                if active.is_none() {
                    match build_stream(level.clone()) {
                        Ok(a) => active = Some(a),
                        Err(e) => log(&format!("start failed: {e}")),
                    }
                }
            }
            Cmd::Stop(reply) => {
                let samples = match active.take() {
                    Some(a) => {
                        // Dropping the stream stops capture; clone the buffer before it is freed.
                        let raw = a.buf.lock().map(|b| b.clone()).unwrap_or_default();
                        drop(a._stream);
                        resample_to_16k(&raw, a.rate)
                    }
                    None => Vec::new(),
                };
                let _ = reply.send(samples);
            }
        }
    }
}

fn build_stream(level: Arc<AtomicU32>) -> Result<Active, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let rate = config.sample_rate.0;

    let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
    let err_fn = |e| log(&format!("stream error: {e}"));

    let stream = match sample_format {
        SampleFormat::F32 => {
            let buf = buf.clone();
            device.build_input_stream(
                &config,
                move |data: &[f32], _| push_mono(&buf, &level, data, channels, |s| s),
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let buf = buf.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    push_mono(&buf, &level, data, channels, |s| s as f32 / 32768.0)
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let buf = buf.clone();
            device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    push_mono(&buf, &level, data, channels, |s| (s as f32 - 32768.0) / 32768.0)
                },
                err_fn,
                None,
            )
        }
        other => return Err(format!("unsupported sample format: {other:?}")),
    }
    .map_err(|e| format!("build input stream: {e}"))?;

    stream.play().map_err(|e| format!("stream play: {e}"))?;
    Ok(Active {
        _stream: stream,
        buf,
        rate,
    })
}

/// Downmix interleaved frames to mono, append to the shared buffer, and publish the
/// chunk's RMS as the live level (read lock-free by the pill's equalizer).
fn push_mono<T: Copy>(
    buf: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<AtomicU32>,
    data: &[T],
    channels: usize,
    conv: impl Fn(T) -> f32,
) {
    if channels == 0 {
        return;
    }
    let mut sum_sq = 0.0f32;
    let mut n = 0usize;
    if let Ok(mut b) = buf.lock() {
        for frame in data.chunks(channels) {
            let sum: f32 = frame.iter().map(|&s| conv(s)).sum();
            let mono = sum / frame.len() as f32;
            sum_sq += mono * mono;
            n += 1;
            b.push(mono);
        }
    }
    if n > 0 {
        level.store((sum_sq / n as f32).sqrt().to_bits(), Ordering::Relaxed);
    }
}

/// Downsample to 16k: average each source window (the anti-aliasing low-pass), then
/// linear-interpolate between window centers. The previous plain block-average produced
/// staircase artifacts the Electron path never had (OfflineAudioContext resamples with
/// proper filtering); this stays dependency-free while removing the worst of the aliasing.
fn resample_to_16k(input: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == TARGET_RATE || input.is_empty() {
        return input.to_vec();
    }
    let ratio = src_rate as f64 / TARGET_RATE as f64;
    let out_len = (input.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    let window = |i: usize| -> f32 {
        let start = (i as f64 * ratio).floor() as usize;
        let end = ((((i + 1) as f64) * ratio).floor() as usize)
            .min(input.len())
            .max(start + 1);
        let slice = &input[start..end.min(input.len())];
        if slice.is_empty() {
            0.0
        } else {
            slice.iter().sum::<f32>() / slice.len() as f32
        }
    };
    for i in 0..out_len {
        let a = window(i);
        let b = if i + 1 < out_len { window(i + 1) } else { a };
        out.push(a * 0.75 + b * 0.25);
    }
    out
}

fn log(msg: &str) {
    eprintln!("[audio] {msg}");
}
