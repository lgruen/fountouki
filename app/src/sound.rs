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
    train_whistle: Option<Sound>,
    finale: Option<Sound>,
    hammer: Option<Sound>,
    digger: Option<Sound>,
    truck_beep: Option<Sound>,
    doorbell: Option<Sound>,
    twinkle: Option<Sound>,
    trace_ticks: Vec<Sound>, // index = pentatonic step
    memory_tones: Vec<Sound>, // index = Sing Back step (rising pitch)
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
            train_whistle: None,
            finale: None,
            hammer: None,
            digger: None,
            truck_beep: None,
            doorbell: None,
            twinkle: None,
            trace_ticks: Vec::new(),
            memory_tones: Vec::new(),
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
        let mut trace_ticks = Vec::with_capacity(synth::TRACE_TICK_STEPS as usize);
        for s in 0..synth::TRACE_TICK_STEPS {
            trace_ticks.push(load_pcm(&synth::trace_tick(s)).await);
        }
        let mut memory_tones = Vec::with_capacity(synth::MEMORY_TONES as usize);
        for s in 0..synth::MEMORY_TONES {
            memory_tones.push(load_pcm(&synth::memory_tone(s)).await);
        }
        Audio {
            correct,
            incorrect: Some(load_pcm(&synth::incorrect()).await),
            level_up: Some(load_pcm(&synth::level_up()).await),
            tap: Some(load_pcm(&synth::tap()).await),
            frog: Some(load_pcm(&synth::frog()).await),
            train_whistle: Some(load_pcm(&synth::train_whistle()).await),
            finale: Some(load_pcm(&synth::finale()).await),
            hammer: Some(load_pcm(&synth::hammer()).await),
            digger: Some(load_pcm(&synth::digger()).await),
            truck_beep: Some(load_pcm(&synth::truck_beep()).await),
            doorbell: Some(load_pcm(&synth::doorbell()).await),
            twinkle: Some(load_pcm(&synth::twinkle()).await),
            trace_ticks,
            memory_tones,
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
    pub fn train_whistle(&self) {
        self.play(&self.train_whistle);
    }
    pub fn finale(&self) {
        self.play(&self.finale);
    }
    pub fn hammer(&self) {
        self.play(&self.hammer);
    }
    pub fn digger(&self) {
        self.play(&self.digger);
    }
    pub fn truck_beep(&self) {
        self.play(&self.truck_beep);
    }
    pub fn doorbell(&self) {
        self.play(&self.doorbell);
    }
    pub fn twinkle(&self) {
        self.play(&self.twinkle);
    }
    pub fn trace_tick(&self, step: u32) {
        if self.silent || self.muted.get() {
            return;
        }
        let i = (step as usize).min(self.trace_ticks.len().saturating_sub(1));
        if let Some(s) = self.trace_ticks.get(i) {
            play_sound(s, PlaySoundParams { looped: false, volume: 1.0 });
        }
    }
    pub fn memory_tone(&self, step: u32) {
        if self.silent || self.muted.get() {
            return;
        }
        let i = (step as usize).min(self.memory_tones.len().saturating_sub(1));
        if let Some(s) = self.memory_tones.get(i) {
            play_sound(s, PlaySoundParams { looped: false, volume: 1.0 });
        }
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
