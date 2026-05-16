// Pattern generation.
//
// A "template" is a short string of placeholder letters like 'AB', 'AAB',
// 'ABC', 'ABBC' etc. The placeholders are filled with distinct Items from
// the active theme to make a "unit". The unit is repeated to form the
// visible sequence. The next item the player must identify is the item
// that would follow the visible part if the unit kept repeating.

import type { Item } from './themes.js';

export interface PatternRound {
  /** The chosen template, e.g. 'AAB'. */
  template: string;
  /** Items in unit order — index 0 corresponds to placeholder 'A'. */
  unitItems: Item[];
  /** The fully-rendered visible sequence (without the missing slot). */
  visible: Item[];
  /** The correct next item. */
  answer: Item;
  /** How many full repetitions of the unit are visible at the start. */
  fullReps: number;
  /** Length of the partial repetition at the tail (0..template.length-1). */
  partialLen: number;
}

export interface GenerateOptions {
  /** Items available in the active theme. */
  pool: Item[];
  /** 1-based difficulty level. */
  level: number;
  /** Random source, defaults to Math.random for testability. */
  rng?: () => number;
}

const TEMPLATES_BY_LEVEL: string[][] = [
  // Level 1: simplest — two items alternating.
  ['AB'],
  // Level 2: still 2 items, but with a doubled element.
  ['AB', 'AAB'],
  // Level 3: 3 items, period 3.
  ['AAB', 'ABB', 'ABC'],
  // Level 4: period 3 with 3 items, plus a slight twist.
  ['ABC', 'AAB', 'ABB'],
  // Level 5: period 4 mostly.
  ['AABB', 'ABAC', 'ABCB'],
  // Level 6+: longer / trickier.
  ['ABBC', 'AABC', 'ABCD'],
];

function pickRng<T>(arr: readonly T[], rng: () => number): T {
  if (arr.length === 0) throw new Error('pickRng: empty array');
  const i = Math.floor(rng() * arr.length);
  // Clamp in case rng() returns exactly 1.
  const idx = Math.min(arr.length - 1, Math.max(0, i));
  return arr[idx] as T;
}

function shuffle<T>(arr: T[], rng: () => number): T[] {
  const out = arr.slice();
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    const a = out[i] as T;
    const b = out[j] as T;
    out[i] = b;
    out[j] = a;
  }
  return out;
}

/** Number of distinct placeholders in a template, e.g. 'ABBC' -> 3. */
export function distinctCount(template: string): number {
  return new Set(template.split('')).size;
}

/** Convert a template letter ('A','B','C'...) to a 0-based index. */
function letterIndex(ch: string): number {
  return ch.charCodeAt(0) - 'A'.charCodeAt(0);
}

/** Pick a template appropriate for the given level. */
function chooseTemplate(level: number, rng: () => number): string {
  const idx = Math.min(level - 1, TEMPLATES_BY_LEVEL.length - 1);
  const tier = TEMPLATES_BY_LEVEL[Math.max(0, idx)] ?? ['AB'];
  return pickRng(tier, rng);
}

/** Build a single round given pool + level. */
export function generateRound(opts: GenerateOptions): PatternRound {
  const rng = opts.rng ?? Math.random;
  const template = chooseTemplate(opts.level, rng);
  const needed = distinctCount(template);

  if (opts.pool.length < needed) {
    throw new Error(
      `theme pool has ${opts.pool.length} items but template '${template}' needs ${needed}`,
    );
  }

  const unitItems = shuffle(opts.pool.slice(), rng).slice(0, needed);

  // Show enough cells that the repetition is obvious without the row
  // wrapping on a phone screen. For period-2 patterns we want 3 full reps
  // so AB doesn't look like a one-off pair. Period-3 and 4 are clearer at
  // 2 reps already.
  const period = template.length;
  const fullReps = period === 2 ? 3 : 2;
  // Partial tail (cells beyond the full reps) lets the answer land
  // mid-cycle; only allow this from level 2 onward.
  const tailMax = period === 2 ? 1 : period === 3 ? 2 : 0;
  const allowPartial = opts.level >= 2 ? tailMax : 0;
  const partialLen = allowPartial > 0 ? Math.floor(rng() * (allowPartial + 1)) : 0;

  const visible: Item[] = [];
  for (let r = 0; r < fullReps; r++) {
    for (const ch of template) {
      const item = unitItems[letterIndex(ch)];
      if (!item) throw new Error('template references missing placeholder');
      visible.push(item);
    }
  }
  for (let i = 0; i < partialLen; i++) {
    const ch = template[i];
    if (!ch) break;
    const item = unitItems[letterIndex(ch)];
    if (!item) throw new Error('template references missing placeholder');
    visible.push(item);
  }

  // Answer = the item that would come at position (visible.length) in the
  // infinite repetition.
  const nextCh = template[visible.length % template.length];
  if (!nextCh) throw new Error('cannot determine next char');
  const answer = unitItems[letterIndex(nextCh)];
  if (!answer) throw new Error('answer item missing');

  return { template, unitItems, visible, answer, fullReps, partialLen };
}

/** Pick 2–3 choices that always include the correct answer. */
export function buildChoices(
  round: PatternRound,
  mode: 'easy' | 'hard',
  pool: Item[],
  rng: () => number = Math.random,
): Item[] {
  const correct = round.answer;
  const count = mode === 'easy' ? Math.min(3, round.unitItems.length) : 4;

  const distractorSource =
    mode === 'easy'
      ? round.unitItems.filter((it) => it.id !== correct.id)
      : pool.filter((it) => it.id !== correct.id);

  const distractors = shuffle(distractorSource, rng).slice(0, count - 1);
  return shuffle([correct, ...distractors], rng);
}
