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
}

impl AudioHandle {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || audio_loop(rx));
        AudioHandle { tx }
    }

    pub fn start(&self) {
        let _ = self.tx.send(Cmd::Start);
    }

    /// Stop capture and return the recorded audio resampled to 16k mono f32.
    pub fn stop(&self) -> Vec<f32> {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.tx.send(Cmd::Stop(reply_tx)).is_err() {
            return Vec::new();
        }
        reply_rx.recv().unwrap_or_default()
    }
}

/// Live capture state held only on the audio thread (the !Send Stream never leaves it).
struct Active {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

fn audio_loop(rx: Receiver<Cmd>) {
    let mut active: Option<Active> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Start => {
                if active.is_none() {
                    match build_stream() {
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

fn build_stream() -> Result<Active, String> {
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
                move |data: &[f32], _| push_mono(&buf, data, channels, |s| s),
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let buf = buf.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _| push_mono(&buf, data, channels, |s| s as f32 / 32768.0),
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let buf = buf.clone();
            device.build_input_stream(
                &config,
                move |data: &[u16], _| {
                    push_mono(&buf, data, channels, |s| (s as f32 - 32768.0) / 32768.0)
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

/// Downmix interleaved frames to mono and append to the shared buffer.
fn push_mono<T: Copy>(buf: &Arc<Mutex<Vec<f32>>>, data: &[T], channels: usize, conv: impl Fn(T) -> f32) {
    if channels == 0 {
        return;
    }
    if let Ok(mut b) = buf.lock() {
        for frame in data.chunks(channels) {
            let sum: f32 = frame.iter().map(|&s| conv(s)).sum();
            b.push(sum / frame.len() as f32);
        }
    }
}

/// Averaging downsample to 16k. Whisper wants 16k mono f32; input is whatever the device gave us.
fn resample_to_16k(input: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == TARGET_RATE || input.is_empty() {
        return input.to_vec();
    }
    let ratio = src_rate as f64 / TARGET_RATE as f64;
    let out_len = (input.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = (i as f64 * ratio).floor() as usize;
        let end = (((i + 1) as f64) * ratio).floor() as usize;
        let end = end.min(input.len()).max(start + 1);
        let slice = &input[start..end.min(input.len())];
        let sum: f32 = slice.iter().sum();
        out.push(if slice.is_empty() { 0.0 } else { sum / slice.len() as f32 });
    }
    out
}

fn log(msg: &str) {
    eprintln!("[audio] {msg}");
}
