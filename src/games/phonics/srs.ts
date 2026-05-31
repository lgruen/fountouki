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
 *  Walks INTRO_ORDER from the start and stops at the frontier — the point
 *  where NEW_LETTER_BUFFER not-yet-introduced letters have been gathered.
 *  Everything up to and including that frontier is active:
 *   - already-introduced letters (box >= INTRODUCED_BOX_MIN) in the prefix
 *     stay in rotation for spaced review;
 *   - plus the first NEW_LETTER_BUFFER not-yet-introduced letters, so the
 *     unfamiliar surface stays small. A relapsed letter (box dropped back
 *     to 0) falls into this bucket too, consuming a slot until recovered.
 *
 *  Crucially we STOP at the frontier rather than scanning the whole order.
 *  An introduced letter that sits *beyond* the frontier — e.g. one polluted
 *  to box >= 1 by an older, ungated build that flashed the whole alphabet —
 *  is parked (its box is retained, never erased) until the kid drips far
 *  enough down INTRO_ORDER to reach it. Without the stop, a single stray
 *  late-letter grade (x, v, q…) leaks back into a fresh learner's rotation
 *  forever, which is exactly the drip-in this gate exists to prevent. */
export function activeLetters(state: PhonicsState): string[] {
  const active: string[] = [];
  let unsettled = 0;
  for (const letter of INTRO_ORDER) {
    // Frontier reached: stop. Everything past the NEW_LETTER_BUFFER-th
    // not-yet-introduced letter is parked, whether or not it's already
    // introduced. (A fully-polluted tail from an older ungated build has
    // no box-0 letter left to stop on, so the cut-off must gate the
    // introduced branch too — not just new letters.)
    if (unsettled >= NEW_LETTER_BUFFER) break;
    const s = state.letters[letter];
    const box = s?.box ?? 0;
    active.push(letter);
    if (box < INTRODUCED_BOX_MIN) unsettled += 1;
  }
  return active;
}

/** Fisher–Yates shuffle in place. rng injectable for deterministic tests. */
function shuffle<T>(arr: T[], rng: () => number): T[] {
  for (let i = arr.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [arr[i], arr[j]] = [arr[j]!, arr[i]!];
  }
  return arr;
}

/** Build a session queue from the active set.
 *
 *  A queue is a *permutation* of the relevant active letters, so every
 *  pass shows each letter once before any repeats — coverage + spacing
 *  for free. Order is shuffled (not due-sorted) so consecutive sessions
 *  don't replay the same recency-ordered sequence, which reads as
 *  mechanical rather than playful.
 *
 *  Letters with due <= now are preferred (genuine SRS spacing across a
 *  day). When none are due — the impatient same-session case — fall back
 *  to all active letters so there's never dead air, lightly biased toward
 *  weaker (lower-box) letters via a stable sort that keeps within-box
 *  order shuffled. Avoiding the *same letter twice in a row* across queue
 *  rebuilds is the caller's job (it knows the last card shown). */
export function buildQueue(
  state: PhonicsState,
  now = Date.now(),
  rng: () => number = Math.random,
): string[] {
  const active = activeLetters(state).filter((l) => state.letters[l]);
  const due = active.filter((l) => state.letters[l]!.due <= now);
  if (due.length > 0) return shuffle(due, rng);
  shuffle(active, rng);
  active.sort((a, b) => state.letters[a]!.box - state.letters[b]!.box);
  return active;
}
