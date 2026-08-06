//! sound.rs: short synthesized cues so the pipeline can be followed by ear alone.
//! Public surface: SoundHandle::spawn(), play(Cue), set_volume(f32), set_enabled(bool).
//! Why this file: the orb and the tray only help if you are looking at them. Orel works in a
//!   terminal, so each stage boundary (recording on, recording off, now translating, pasted,
//!   failed) gets a distinct two-note motif instead of a glance. Tones are generated here
//!   rather than played from files: no assets to ship, no `afplay` subprocess per cue (~40ms
//!   and a Dock bounce), and the volume is a plain multiplier we control.
//! NOT responsible for: deciding WHEN a cue fires (lib.rs sequences the pipeline).
//! Test strategy: call play() for each Cue with volume 1.0 and listen - each must be audible,
//!   under ~200ms, and distinguishable from the others without looking at the screen.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Attack/release ramp. Without it a raw sine start/stop is a click, which reads as a glitch
/// rather than a cue - the one thing a subtle sound must not do.
const RAMP_MS: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// Recording opened - rising, "we are listening".
    Start,
    /// Recording closed, audio captured - a single settled note.
    Stop,
    /// Handed to the translator - a quiet tick, deliberately the least intrusive of the five.
    Working,
    /// Text is on the clipboard and pasted - rising resolve, the "you can look now" cue.
    Done,
    /// Something failed - falling, the only descending motif so it can never be mistaken.
    Error,
}

impl Cue {
    /// (frequency Hz, duration ms, gain) per note. Kept in one table so the whole sound design
    /// of the app is readable at a glance and stays mutually distinguishable.
    fn notes(self) -> &'static [(f32, f32, f32)] {
        match self {
            Cue::Start => &[(587.33, 60.0, 1.0), (880.00, 70.0, 1.0)],
            Cue::Stop => &[(493.88, 90.0, 0.9)],
            Cue::Working => &[(1174.66, 28.0, 0.35), (1174.66, 28.0, 0.35)],
            Cue::Done => &[(880.00, 55.0, 0.9), (1174.66, 90.0, 1.0)],
            Cue::Error => &[(392.00, 90.0, 0.9), (261.63, 130.0, 0.9)],
        }
    }
}

/// Cues are pushed here as ready-made samples; the output callback drains it and writes
/// silence when it is empty, so the stream can stay open and a cue starts instantly.
type Queue = Arc<Mutex<VecDeque<f32>>>;

pub struct SoundHandle {
    queue: Queue,
    sample_rate: Arc<Mutex<f32>>,
    enabled: Arc<AtomicBool>,
    volume: Arc<Mutex<f32>>,
}

impl SoundHandle {
    pub fn spawn(enabled: bool, volume: f32) -> Self {
        let handle = Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            sample_rate: Arc::new(Mutex::new(48_000.0)),
            enabled: Arc::new(AtomicBool::new(enabled)),
            volume: Arc::new(Mutex::new(volume.clamp(0.0, 1.0))),
        };
        let queue = handle.queue.clone();
        let rate = handle.sample_rate.clone();
        // cpal's Stream is !Send on macOS, so it is created, owned and dropped on this one
        // thread and never crosses a boundary (same rule as the capture stream in audio.rs).
        std::thread::spawn(move || run_output(queue, rate));
        handle
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    pub fn set_volume(&self, v: f32) {
        if let Ok(mut vol) = self.volume.lock() {
            *vol = v.clamp(0.0, 1.0);
        }
    }

    /// Render a cue into the queue. Non-blocking: returns before a single sample is heard.
    pub fn play(&self, cue: Cue) {
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let volume = self.volume.lock().map(|v| *v).unwrap_or(0.35);
        if volume <= 0.0 {
            return;
        }
        let rate = self.sample_rate.lock().map(|r| *r).unwrap_or(48_000.0);
        let samples = render(cue, rate, volume);
        if let Ok(mut q) = self.queue.lock() {
            // A cue that arrives while another is still sounding replaces it: stage changes can
            // land back to back, and overlapping motifs are less legible than the newest one.
            q.clear();
            q.extend(samples);
        }
    }
}

/// Build the waveform for one cue: sine notes, each with a raised-cosine attack and release.
fn render(cue: Cue, rate: f32, volume: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (freq, ms, gain) in cue.notes() {
        let total = ((ms / 1000.0) * rate) as usize;
        let ramp = ((RAMP_MS / 1000.0) * rate).min(total as f32 / 2.0) as usize;
        for i in 0..total {
            let phase = std::f32::consts::TAU * freq * (i as f32 / rate);
            let envelope = if i < ramp {
                i as f32 / ramp as f32
            } else if i >= total - ramp {
                (total - i) as f32 / ramp as f32
            } else {
                1.0
            };
            // Smoothstep the ramp so the onset is a swell, not a linear edge.
            let envelope = envelope * envelope * (3.0 - 2.0 * envelope);
            out.push(phase.sin() * envelope * gain * volume * 0.25);
        }
    }
    out
}

fn run_output(queue: Queue, rate_slot: Arc<Mutex<f32>>) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        eprintln!("[sound] no output device; cues disabled");
        return;
    };
    let Ok(config) = device.default_output_config() else {
        eprintln!("[sound] no default output config; cues disabled");
        return;
    };
    let channels = config.channels() as usize;
    if let Ok(mut r) = rate_slot.lock() {
        *r = config.sample_rate().0 as f32;
    }

    let err_fn = |e| eprintln!("[sound] output stream error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| fill(data, channels, &queue),
            err_fn,
            None,
        ),
        other => {
            // The default output on this machine is f32; anything else stays silent rather
            // than guessing a conversion and emitting noise into Orel's speakers.
            eprintln!("[sound] unsupported sample format {other:?}; cues disabled");
            return;
        }
    };
    let Ok(stream) = stream else {
        eprintln!("[sound] failed to build output stream; cues disabled");
        return;
    };
    if let Err(e) = stream.play() {
        eprintln!("[sound] failed to start output stream: {e}");
        return;
    }
    // Hold the stream open for the life of the app: re-opening per cue costs tens of
    // milliseconds and can click on the device transition.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

/// Drain the queue into the device buffer, duplicating each mono sample across channels.
fn fill(data: &mut [f32], channels: usize, queue: &Queue) {
    let mut q = match queue.lock() {
        Ok(q) => q,
        Err(_) => {
            data.fill(0.0);
            return;
        }
    };
    for frame in data.chunks_mut(channels) {
        let sample = q.pop_front().unwrap_or(0.0);
        frame.fill(sample);
    }
}
