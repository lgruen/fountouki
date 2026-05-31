//! Audio: synthesize PCM in `fountouki-core`, encode to in-memory WAV, load as
//! macroquad Sounds. Mute is honored on play. Capture/CI uses `silent()` so
//! there's no audio-device dependency for golden rendering.
use fountouki_core::audio as synth;
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
use std::cell::Cell;

pub struct Audio {
    correct: Vec<Sound>, // index = streak.min(5)
    incorrect: Option<Sound>,
    level_up: Option<Sound>,
    tap: Option<Sound>,
    frog: Option<Sound>,
    muted: Cell<bool>,
    silent: bool,
}

impl Audio {
    /// No-op audio (capture / headless / CI) — never touches the audio device.
    pub fn silent() -> Audio {
        Audio {
            correct: Vec::new(),
            incorrect: None,
            level_up: None,
            tap: None,
            frog: None,
            muted: Cell::new(true),
            silent: true,
        }
    }

    /// Synthesize + load every sound. `correct` is pre-rendered for streak 0..=5.
    pub async fn load(muted: bool) -> Audio {
        let mut correct = Vec::with_capacity(6);
        for s in 0..=5u32 {
            correct.push(load_pcm(&synth::correct(s)).await);
        }
        Audio {
            correct,
            incorrect: Some(load_pcm(&synth::incorrect()).await),
            level_up: Some(load_pcm(&synth::level_up()).await),
            tap: Some(load_pcm(&synth::tap()).await),
            frog: Some(load_pcm(&synth::frog()).await),
            muted: Cell::new(muted),
            silent: false,
        }
    }

    pub fn set_muted(&self, m: bool) {
        self.muted.set(m);
    }
    pub fn muted(&self) -> bool {
        self.muted.get()
    }

    fn play(&self, s: &Option<Sound>) {
        if self.silent || self.muted.get() {
            return;
        }
        if let Some(s) = s {
            play_sound(s, PlaySoundParams { looped: false, volume: 1.0 });
        }
    }

    pub fn correct(&self, streak: u32) {
        if self.silent || self.muted.get() {
            return;
        }
        if let Some(s) = self.correct.get((streak.min(5)) as usize) {
            play_sound(s, PlaySoundParams { looped: false, volume: 1.0 });
        }
    }
    pub fn incorrect(&self) {
        self.play(&self.incorrect);
    }
    pub fn level_up(&self) {
        self.play(&self.level_up);
    }
    pub fn tap(&self) {
        self.play(&self.tap);
    }
    pub fn frog(&self) {
        self.play(&self.frog);
    }
}

async fn load_pcm(pcm: &[f32]) -> Sound {
    let wav = pcm_to_wav(pcm, synth::SAMPLE_RATE);
    load_sound_from_bytes(&wav)
        .await
        .expect("decode synthesized wav")
}

/// Encode mono f32 PCM as a 16-bit little-endian WAV (macroquad decodes WAV).
fn pcm_to_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let n = samples.len();
    let data_len = (n * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    let mut put = |bytes: &[u8]| v.extend_from_slice(bytes);
    put(b"RIFF");
    put(&(36 + data_len).to_le_bytes());
    put(b"WAVE");
    put(b"fmt ");
    put(&16u32.to_le_bytes()); // fmt chunk size
    put(&1u16.to_le_bytes()); // PCM
    put(&1u16.to_le_bytes()); // mono
    put(&rate.to_le_bytes());
    put(&(rate * 2).to_le_bytes()); // byte rate (mono * 2 bytes)
    put(&2u16.to_le_bytes()); // block align
    put(&16u16.to_le_bytes()); // bits per sample
    put(b"data");
    put(&data_len.to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * i16::MAX as f32) as i16;
        v.extend_from_slice(&i.to_le_bytes());
    }
    v
}
