// Web Audio sounds — no external files, all synthesized.
// Keeping it tiny: a single AudioContext, a couple of preset chimes.

let ctx: AudioContext | null = null;
let muted = false;

function ensureCtx(): AudioContext | null {
  if (muted) return null;
  if (!ctx) {
    const Ctor =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext })
        .webkitAudioContext;
    if (!Ctor) return null;
    ctx = new Ctor();
  }
  // iOS Safari starts the context suspended until a user gesture; resume on demand.
  if (ctx.state === 'suspended') void ctx.resume();
  return ctx;
}

export function setMuted(value: boolean): void {
  muted = value;
}

export function isMuted(): boolean {
  return muted;
}

interface NoteSpec {
  /** Frequency in Hz. */
  freq: number;
  /** Start offset in seconds. */
  start: number;
  /** Duration in seconds. */
  dur: number;
  /** Peak gain (0..1). Defaults to 0.18. */
  gain?: number;
  /** Oscillator type. Defaults to 'sine'. */
  type?: OscillatorType;
}

function playNotes(notes: NoteSpec[]): void {
  const c = ensureCtx();
  if (!c) return;
  const now = c.currentTime;
  const master = c.createGain();
  master.gain.value = 1;
  master.connect(c.destination);

  for (const n of notes) {
    const osc = c.createOscillator();
    const gain = c.createGain();
    osc.type = n.type ?? 'sine';
    osc.frequency.value = n.freq;
    const peak = n.gain ?? 0.18;
    const start = now + n.start;
    const end = start + n.dur;
    gain.gain.setValueAtTime(0, start);
    gain.gain.linearRampToValueAtTime(peak, start + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, end);
    osc.connect(gain).connect(master);
    osc.start(start);
    osc.stop(end + 0.02);
  }
}

/** Pleasant ascending triad — "you got it!". Optional streak (0+)
 *  pitches the chime up a semitone per step (capped at +5) so a hot
 *  streak feels like it's building. */
export function playCorrect(streak = 0): void {
  const shift = Math.pow(2, Math.min(streak, 5) / 12);
  playNotes([
    { freq: 523.25 * shift, start: 0.0, dur: 0.18 },
    { freq: 659.25 * shift, start: 0.09, dur: 0.18 },
    { freq: 783.99 * shift, start: 0.18, dur: 0.28 },
  ]);
}

/** Gentle two-note descent — "try again". Never harsh. */
export function playIncorrect(): void {
  playNotes([
    { freq: 392.0, start: 0.0, dur: 0.16, type: 'triangle', gain: 0.14 }, // G4
    { freq: 329.63, start: 0.12, dur: 0.22, type: 'triangle', gain: 0.14 }, // E4
  ]);
}

/** Cheery fanfare on level-up. */
export function playLevelUp(): void {
  playNotes([
    { freq: 523.25, start: 0.0, dur: 0.14 },
    { freq: 659.25, start: 0.1, dur: 0.14 },
    { freq: 783.99, start: 0.2, dur: 0.14 },
    { freq: 1046.5, start: 0.32, dur: 0.32 },
  ]);
}

/** Soft tick used when the player taps a choice. */
export function playTap(): void {
  playNotes([{ freq: 660, start: 0, dur: 0.05, type: 'sine', gain: 0.08 }]);
}

/** Two-syllable "ri-bbit" for the phonics rainbow-done frog mascot.
 *  Low triangle waves with a rapid up-bend per syllable. Kept at or
 *  below the level-up ceiling so the modal doesn't outshine in-game wins. */
export function playFrog(): void {
  playNotes([
    { freq: 220, start: 0.0,  dur: 0.09, type: 'triangle', gain: 0.16 },
    { freq: 300, start: 0.05, dur: 0.08, type: 'triangle', gain: 0.14 },
    { freq: 200, start: 0.18, dur: 0.10, type: 'triangle', gain: 0.16 },
    { freq: 280, start: 0.22, dur: 0.09, type: 'triangle', gain: 0.14 },
  ]);
}
