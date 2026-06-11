//! Pure PCM synthesis for the app's sound effects, ported from
//! `src/shared/sounds.ts` (see `docs/port-spec/audio-fx.md` §1).
//!
//! Each sound is a sum of independent oscillator "notes" (sine or triangle),
//! every note shaped by the same envelope: 0.01 s linear attack to `gain`,
//! then an exponential decay to `0.0001` ending exactly at the note's end.
//! Notes are summed (no master compression in the JS source); we soft-clamp
//! the final mix to `[-1, 1]` so a native backend can't hard-clip.
//!
//! No wall-clock, no I/O — these functions just return mono `Vec<f32>`. The
//! macroquad layer wraps the samples into a `Sound`.

/// Output sample rate. Matches the rendered PCM the app expects.
pub const SAMPLE_RATE: u32 = 44100;

/// Default per-note envelope peak (`NoteSpec.gain` default in the spec).
const DEFAULT_GAIN: f32 = 0.18;
/// Attack time: linear ramp 0 -> peak over this many seconds.
const ATTACK_S: f32 = 0.01;
/// Exponential decay floor (Web Audio `exponentialRampToValueAtTime` can't
/// reach 0, so the source uses this target).
const DECAY_FLOOR: f32 = 0.0001;

/// Oscillator waveform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
}

/// One scheduled oscillator + its envelope, relative to the call's `t0 = 0`.
#[derive(Clone, Copy, Debug)]
struct Note {
    /// Oscillator frequency in Hz (constant for the note's life; no glide).
    freq: f32,
    /// Onset offset in seconds, relative to the call start.
    start: f32,
    /// Duration in seconds.
    dur: f32,
    /// Envelope peak amplitude.
    gain: f32,
    /// Oscillator waveform.
    waveform: Waveform,
}

impl Note {
    /// A note with the spec defaults: sine, gain 0.18.
    fn new(freq: f32, start: f32, dur: f32) -> Self {
        Self { freq, start, dur, gain: DEFAULT_GAIN, waveform: Waveform::Sine }
    }
}

/// Evaluate an oscillator at phase `phi` (in turns: `phi = (freq * t) mod 1`).
///
/// - sine: `sin(2π·φ)`
/// - triangle: `4·|φ − 0.5| − 1` (peaks at φ=0, troughs at φ=0.5)
#[inline]
fn osc(waveform: Waveform, phi: f32) -> f32 {
    match waveform {
        Waveform::Sine => (std::f32::consts::TAU * phi).sin(),
        Waveform::Triangle => 4.0 * (phi - 0.5).abs() - 1.0,
    }
}

/// Envelope gain at absolute time `t` (seconds) for a note running
/// `[start, end]` with peak `gain`. Returns 0 outside the note.
///
/// 1. linear 0 -> peak over `[start, start + ATTACK_S]`
/// 2. exponential peak -> `DECAY_FLOOR` over `[start + ATTACK_S, end]`:
///    `g(t) = peak · (floor/peak)^((t − knee) / (end − knee))`
#[inline]
fn envelope(t: f32, start: f32, dur: f32, gain: f32) -> f32 {
    let end = start + dur;
    if t < start || t > end {
        return 0.0;
    }
    let knee = start + ATTACK_S;
    if t <= knee {
        // Linear attack. (If dur < attack the note ends before the knee, but
        // the spec guarantees dur >= 0.05 so this branch always completes.)
        let frac = (t - start) / ATTACK_S;
        gain * frac
    } else {
        // Exponential decay from peak (at knee) to DECAY_FLOOR (at end).
        let span = end - knee;
        if span <= 0.0 {
            return DECAY_FLOOR;
        }
        let frac = (t - knee) / span;
        gain * (DECAY_FLOOR / gain).powf(frac)
    }
}

/// Render one note additively into `buf` (mono, `SAMPLE_RATE`). Buffer index
/// is absolute time `i / SAMPLE_RATE`; only samples within the note's span are
/// touched. The note's contribution is summed onto whatever is already there.
fn render_note(buf: &mut [f32], note: &Note) {
    let sr = SAMPLE_RATE as f32;
    let start = note.start;
    let end = note.start + note.dur;
    let i_start = (start * sr).floor().max(0.0) as usize;
    // Inclusive upper bound; clamp to buffer.
    let i_end = ((end * sr).ceil() as usize).min(buf.len().saturating_sub(1));
    if i_start >= buf.len() {
        return;
    }
    for (i, sample) in buf.iter_mut().enumerate().take(i_end + 1).skip(i_start) {
        let t = i as f32 / sr;
        let env = envelope(t, note.start, note.dur, note.gain);
        if env == 0.0 {
            continue;
        }
        // Phase in turns, wrapped to [0,1) via fract() of a non-negative value.
        let phi = (note.freq * t).fract();
        *sample += env * osc(note.waveform, phi);
    }
}

/// Mix a set of notes into a fresh buffer sized to cover the latest note end,
/// then soft-clamp the result to `[-1, 1]`.
fn mix(notes: &[Note]) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let mut max_end = 0.0f32;
    for n in notes {
        let end = n.start + n.dur;
        if end > max_end {
            max_end = end;
        }
    }
    // +1 sample of slack so the ceil()-indexed final sample always fits.
    let len = ((max_end * sr).ceil() as usize) + 1;
    let mut buf = vec![0.0f32; len.max(1)];
    for n in notes {
        render_note(&mut buf, n);
    }
    soft_clamp(&mut buf);
    buf
}

/// Soft-clamp every sample to `[-1, 1]` with `tanh`. The JS source neither
/// clamps nor compresses; chord overlaps can momentarily exceed a single
/// note's peak. `tanh` is ~linear in the small-amplitude range we live in
/// (all gains <= 0.18) and only bends the rare overshoot, preserving the
/// "feel" while guaranteeing the buffer is in range for a native backend.
fn soft_clamp(buf: &mut [f32]) {
    for s in buf.iter_mut() {
        *s = s.tanh();
    }
}

/// `playCorrect(streak)` — ascending major triad C5–E5–G5 with a streak
/// pitch-shift. `shift = 2^(min(streak, 5) / 12)` is applied to every freq;
/// streak is capped at 5 (+5 semitones, ~1.33484). Default sine / gain 0.18.
pub fn correct(streak: u32) -> Vec<f32> {
    let shift = 2f32.powf(streak.min(5) as f32 / 12.0);
    let notes = [
        Note::new(523.25 * shift, 0.00, 0.18), // C5
        Note::new(659.25 * shift, 0.09, 0.18), // E5
        Note::new(783.99 * shift, 0.18, 0.28), // G5
    ];
    mix(&notes)
}

/// `playIncorrect()` — gentle two-note descent G4→E4. Triangle, gain 0.14.
pub fn incorrect() -> Vec<f32> {
    let notes = [
        Note { freq: 392.0, start: 0.00, dur: 0.16, gain: 0.14, waveform: Waveform::Triangle }, // G4
        Note { freq: 329.63, start: 0.12, dur: 0.22, gain: 0.14, waveform: Waveform::Triangle }, // E4
    ];
    mix(&notes)
}

/// `playLevelUp()` — four-note rising fanfare C5–E5–G5–C6. Sine, gain 0.18,
/// no streak shift.
pub fn level_up() -> Vec<f32> {
    let notes = [
        Note::new(523.25, 0.00, 0.14), // C5
        Note::new(659.25, 0.10, 0.14), // E5
        Note::new(783.99, 0.20, 0.14), // G5
        Note::new(1046.5, 0.32, 0.32), // C6
    ];
    mix(&notes)
}

/// `playTap()` — single soft tick. One sine note, gain 0.08, dur 0.05.
pub fn tap() -> Vec<f32> {
    let notes = [Note { freq: 660.0, start: 0.00, dur: 0.05, gain: 0.08, waveform: Waveform::Sine }];
    mix(&notes)
}

/// `playFrog()` — two-syllable "ri-bbit": four triangle notes in two pairs.
pub fn frog() -> Vec<f32> {
    let notes = [
        Note { freq: 220.0, start: 0.00, dur: 0.09, gain: 0.16, waveform: Waveform::Triangle },
        Note { freq: 300.0, start: 0.05, dur: 0.08, gain: 0.14, waveform: Waveform::Triangle },
        Note { freq: 200.0, start: 0.18, dur: 0.10, gain: 0.16, waveform: Waveform::Triangle },
        Note { freq: 280.0, start: 0.22, dur: 0.09, gain: 0.14, waveform: Waveform::Triangle },
    ];
    mix(&notes)
}

/// `trainWhistle()` — a cheerful "toot-toot". Each toot is a perfect-fifth steam
/// chord (root + fifth + octave) on reedy triangles, with a slightly detuned
/// partner on the fifth for the shimmery beat of a real steam whistle. The frog
/// equivalent for the patterns finale's tap reaction.
pub fn train_whistle() -> Vec<f32> {
    use Waveform::{Sine, Triangle};
    // root G4, fifth D5, octave G5 — the classic open, bright whistle interval.
    let toot = |start: f32| {
        [
            Note { freq: 392.00, start, dur: 0.24, gain: 0.12, waveform: Triangle }, // G4
            Note { freq: 587.33, start, dur: 0.24, gain: 0.12, waveform: Triangle }, // D5
            Note { freq: 783.99, start, dur: 0.24, gain: 0.10, waveform: Triangle }, // G5
            Note { freq: 590.50, start, dur: 0.24, gain: 0.05, waveform: Sine },     // detune shimmer
        ]
    };
    let mut notes = Vec::with_capacity(8);
    notes.extend(toot(0.00));
    notes.extend(toot(0.30));
    mix(&notes)
}

/// `finale()` — the grand "you made it to the end!" flourish for the patterns
/// finale: a quick rising run, then a held C-major chord with a high sparkle on
/// top. Grander and longer than [`level_up`].
pub fn finale() -> Vec<f32> {
    use Waveform::Sine;
    let chord = 0.15;
    let notes = [
        // ascending run
        Note::new(523.25, 0.00, 0.12), // C5
        Note::new(659.25, 0.10, 0.12), // E5
        Note::new(783.99, 0.20, 0.12), // G5
        Note::new(1046.50, 0.30, 0.16), // C6
        Note::new(1318.51, 0.40, 0.18), // E6
        // held triumphant C-major chord
        Note { freq: 523.25, start: 0.52, dur: 0.72, gain: chord, waveform: Sine }, // C5
        Note { freq: 659.25, start: 0.52, dur: 0.72, gain: chord, waveform: Sine }, // E5
        Note { freq: 783.99, start: 0.52, dur: 0.72, gain: chord, waveform: Sine }, // G5
        Note { freq: 1046.50, start: 0.52, dur: 0.72, gain: chord, waveform: Sine }, // C6
        // sparkle on top
        Note { freq: 1567.98, start: 0.62, dur: 0.30, gain: 0.09, waveform: Sine }, // G6
        Note { freq: 2093.00, start: 0.74, dur: 0.34, gain: 0.07, waveform: Sine }, // C7
    ];
    mix(&notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sample finite (no NaN / inf) and within `[-1, 1]`.
    fn assert_clean(buf: &[f32]) {
        assert!(!buf.is_empty(), "buffer must be non-empty");
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "sample {i} is not finite: {s}");
            assert!(s.abs() <= 1.0, "sample {i} out of range: {s}");
        }
    }

    fn max_abs(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
    }

    /// Count zero-crossings: a cheap proxy for dominant frequency. More
    /// crossings per second ⇒ higher pitch.
    fn zero_crossings(buf: &[f32]) -> usize {
        let mut count = 0;
        for w in buf.windows(2) {
            // Sign change (ignore exact-zero plateaus on either side).
            if w[0] != 0.0 && w[1] != 0.0 && (w[0] < 0.0) != (w[1] < 0.0) {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn all_sounds_clean() {
        assert_clean(&correct(0));
        assert_clean(&correct(3));
        assert_clean(&correct(5));
        assert_clean(&correct(10));
        assert_clean(&incorrect());
        assert_clean(&level_up());
        assert_clean(&tap());
        assert_clean(&frog());
        assert_clean(&train_whistle());
        assert_clean(&finale());
    }

    #[test]
    fn all_sounds_nonempty_and_in_range() {
        for buf in [correct(0), incorrect(), level_up(), tap(), frog(), train_whistle(), finale()] {
            assert!(!buf.is_empty());
            assert!(max_abs(&buf) <= 1.0);
        }
    }

    #[test]
    fn train_whistle_is_two_toots() {
        // Second toot starts at 0.30, dur 0.24 -> ends 0.54 s.
        let buf = train_whistle();
        let expected = ((0.54f32 * SAMPLE_RATE as f32).ceil() as usize) + 1;
        assert_eq!(buf.len(), expected);
        assert!(max_abs(&buf) > 0.0, "whistle must produce sound");
    }

    #[test]
    fn finale_is_grander_than_level_up() {
        // The finale's held chord + sparkle runs to 1.08 s — longer than the
        // four-note level-up fanfare, so it reads as a bigger moment.
        let f = finale();
        let lu = level_up();
        assert!(f.len() > lu.len(), "finale ({}) should outlast level_up ({})", f.len(), lu.len());
        assert!(max_abs(&f) > 0.0, "finale must produce sound");
    }

    #[test]
    fn correct_streak_caps_at_five() {
        // streak 5 and streak 10 both clamp to shift 2^(5/12) -> identical.
        assert_eq!(correct(5), correct(10));
    }

    #[test]
    fn correct_higher_streak_is_higher_pitched() {
        // Same waveform/duration, only freq scales by the streak shift, so a
        // higher streak yields more zero-crossings in the same buffer length.
        let c0 = correct(0);
        let c5 = correct(5);
        assert_eq!(c0.len(), c5.len(), "shift must not change buffer length");
        let zc0 = zero_crossings(&c0);
        let zc5 = zero_crossings(&c5);
        assert!(
            zc5 > zc0,
            "streak-5 should be higher pitched: zc0={zc0}, zc5={zc5}"
        );
    }

    #[test]
    fn correct_length_matches_latest_note_end() {
        // G5 starts at 0.18 with dur 0.28 -> ends at 0.46 s.
        let buf = correct(0);
        let expected = ((0.46f32 * SAMPLE_RATE as f32).ceil() as usize) + 1;
        assert_eq!(buf.len(), expected);
    }

    #[test]
    fn frog_buffer_covers_full_ribbit() {
        // Latest end: note 4 starts 0.22, dur 0.09 -> 0.31 s.
        let buf = frog();
        let expected = ((0.31f32 * SAMPLE_RATE as f32).ceil() as usize) + 1;
        assert_eq!(buf.len(), expected);
        assert!(max_abs(&buf) > 0.0, "frog must actually produce sound");
    }

    #[test]
    fn envelope_attack_and_decay_shape() {
        // At onset: 0. At the attack knee (start + 0.01): peak. Just before
        // end: near the decay floor (well below peak).
        let (start, dur, gain) = (0.0f32, 0.18f32, 0.18f32);
        assert!((envelope(start, start, dur, gain) - 0.0).abs() < 1e-6);
        let knee = start + ATTACK_S;
        assert!((envelope(knee, start, dur, gain) - gain).abs() < 1e-4);
        let near_end = start + dur - 1e-4;
        let g = envelope(near_end, start, dur, gain);
        assert!(g < gain * 0.01, "decay should be near floor by end: {g}");
        // Outside the note window: silent.
        assert_eq!(envelope(start - 0.01, start, dur, gain), 0.0);
        assert_eq!(envelope(start + dur + 0.01, start, dur, gain), 0.0);
    }

    #[test]
    fn waveform_definitions() {
        // sine: sin(2π·φ)
        assert!((osc(Waveform::Sine, 0.0) - 0.0).abs() < 1e-6);
        assert!((osc(Waveform::Sine, 0.25) - 1.0).abs() < 1e-6);
        // triangle: 4·|φ−0.5|−1; peak +1 at φ=0, trough −1 at φ=0.5.
        assert!((osc(Waveform::Triangle, 0.0) - 1.0).abs() < 1e-6);
        assert!((osc(Waveform::Triangle, 0.5) - (-1.0)).abs() < 1e-6);
        assert!((osc(Waveform::Triangle, 0.25) - 0.0).abs() < 1e-6);
    }
}
