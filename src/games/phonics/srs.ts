// Leitner SRS for the phonics deck. Boxes 0–4; intervals tuned for a
// 4yo doing short sessions 1–3x/day.

import { INTRO_ORDER, LETTERS } from './deck.js';

export const MAX_BOX = 4;
export const SCHEMA_VERSION = 1;

// At most this many "not-yet-settled" letters (box < INTRODUCED_BOX_MIN)
// can be in the active rotation at once. Keeps the unfamiliar surface
// small while a fresh learner is still pattern-matching letter shapes.
export const NEW_LETTER_BUFFER = 3;
// A letter is "introduced" once it's been graded correct at least once
// (box >= 1). Until then it counts against NEW_LETTER_BUFFER. A relapse
// back to box 0 also re-counts it as new — kid needs the breathing room.
export const INTRODUCED_BOX_MIN = 1;

export interface LetterState {
  /** 0 (new / missed) to 4 (mastered for a few days). */
  box: number;
  /** Epoch ms; ready when due <= Date.now(). */
  due: number;
  /** Epoch ms of last grade. */
  lastSeen: number;
}

export interface PhonicsState {
  schemaVersion: number;
  letters: Record<string, LetterState>;
  /** Bumped on every change so the sync client can sanity-check freshness. */
  version: number;
}

const MIN = 60 * 1000;
const HOUR = 60 * MIN;

/** Interval from grading a card to its next due time.
 *  The early ramp is short enough that a kid can see the same card 2-3×
 *  in one ~5-min session (massed practice for the new mapping), then a
 *  longer park before the next session. */
export function intervalFor(box: number): number {
  switch (box) {
    case 0:
      return 0;
    case 1:
      return 2 * MIN;
    case 2:
      return 15 * MIN;
    case 3:
      return 6 * HOUR;
    default:
      return 24 * HOUR;
  }
}

export function emptyState(): PhonicsState {
  return { schemaVersion: SCHEMA_VERSION, letters: {}, version: 0 };
}

/** Validate a loaded blob. Returns a fresh empty state on mismatch / bad
 *  shape so future schema drift can't corrupt gameplay. */
export function validate(raw: unknown): PhonicsState | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Partial<PhonicsState>;
  if (r.schemaVersion !== SCHEMA_VERSION) return null;
  if (typeof r.version !== 'number') return null;
  if (!r.letters || typeof r.letters !== 'object') return null;
  return { schemaVersion: SCHEMA_VERSION, letters: r.letters, version: r.version };
}

/** Ensure every letter in the deck has a state entry. Mutates. */
export function ensureLetters(state: PhonicsState, now = Date.now()): void {
  for (const l of LETTERS) {
    if (!state.letters[l]) {
      state.letters[l] = { box: 0, due: now, lastSeen: 0 };
    }
  }
}

/** Merge a remote state into local. Per-letter winner = larger lastSeen.
 *  Version is max(local, remote) so concurrent edits on two devices both
 *  survive even when their versions match. */
export function merge(local: PhonicsState, remote: PhonicsState | null): PhonicsState {
  if (!remote) return local;
  const letters: Record<string, LetterState> = {};
  const keys = new Set<string>([
    ...Object.keys(local.letters),
    ...Object.keys(remote.letters),
  ]);
  for (const k of keys) {
    const a = local.letters[k];
    const b = remote.letters[k];
    if (!a && b) {
      letters[k] = b;
    } else if (a && !b) {
      letters[k] = a;
    } else if (a && b) {
      letters[k] = b.lastSeen > a.lastSeen ? b : a;
    }
  }
  return {
    schemaVersion: SCHEMA_VERSION,
    letters,
    version: Math.max(local.version, remote.version),
  };
}

export function gotIt(state: PhonicsState, letter: string, now = Date.now()): void {
  const s = state.letters[letter];
  if (!s) return;
  s.box = Math.min(MAX_BOX, s.box + 1);
  s.due = now + intervalFor(s.box);
  s.lastSeen = now;
  state.version += 1;
}

/** Soft-decay on miss: drop one box, not all the way to 0. For an
 *  audience with memory challenges, a single wobble shouldn't blow away
 *  days of separation — and dropping 2 boxes was flooding subsequent
 *  sessions with the same letter. */
export function missed(state: PhonicsState, letter: string, now = Date.now()): void {
  const s = state.letters[letter];
  if (!s) return;
  s.box = Math.max(0, s.box - 1);
  s.due = now + intervalFor(s.box);
  s.lastSeen = now;
  state.version += 1;
}

/** Letters currently eligible to be queued.
 *
 *  Walks INTRO_ORDER and includes:
 *   - every already-introduced letter (box >= INTRODUCED_BOX_MIN), regardless
 *     of position — keeps prior progress in rotation even if INTRO_ORDER
 *     is reordered later, or the kid jumps around;
 *   - plus the first NEW_LETTER_BUFFER not-yet-introduced letters from
 *     INTRO_ORDER, so the unfamiliar surface stays small. A relapsed
 *     letter (box dropped back to 0) falls into this bucket too,
 *     consuming a slot until the kid recovers it. */
export function activeLetters(state: PhonicsState): string[] {
  const active: string[] = [];
  let unsettled = 0;
  for (const letter of INTRO_ORDER) {
    const s = state.letters[letter];
    const box = s?.box ?? 0;
    if (box >= INTRODUCED_BOX_MIN) {
      active.push(letter);
    } else if (unsettled < NEW_LETTER_BUFFER) {
      active.push(letter);
      unsettled += 1;
    }
  }
  return active;
}

/** Build a session queue from the active set: letters with due <= now,
 *  sorted by due asc. If none are due, fall back to all active letters
 *  sorted by box asc → due asc so the kid can always practice. */
export function buildQueue(state: PhonicsState, now = Date.now()): string[] {
  const active = activeLetters(state);
  const due: Array<[string, LetterState]> = [];
  for (const l of active) {
    const s = state.letters[l];
    if (s && s.due <= now) due.push([l, s]);
  }
  if (due.length > 0) {
    due.sort((a, b) => a[1].due - b[1].due);
    return due.map(([l]) => l);
  }
  const all: Array<[string, LetterState]> = [];
  for (const l of active) {
    const s = state.letters[l];
    if (s) all.push([l, s]);
  }
  all.sort((a, b) => a[1].box - b[1].box || a[1].due - b[1].due);
  return all.map(([l]) => l);
}
