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
  // Each level mixes templates of multiple periods so unit-mode actually
  // requires looking at each pattern — otherwise the kid would learn
  // "the answer for level N is always length K" and stop reading the row.
  // Level 1: foundation — period 2 + 3, 2 distinct items.
  ['AB', 'AAB', 'ABB'],
  // Level 2: add 3-item patterns; period 2 still appears occasionally.
  ['AB', 'AAB', 'ABB', 'ABC'],
  // Level 3: introduce period 4, keep some period 3.
  ['AAB', 'ABB', 'ABC', 'AABB'],
  // Level 4: period 4 dominates, with 3-item variants.
  ['ABC', 'AABB', 'AABC', 'ABBC', 'ABCB'],
  // Level 5: 4-distinct ABCD appears, with easier templates still in the
  // mix so it doesn't feel relentlessly hard.
  ['AB', 'AAB', 'ABC', 'AABB', 'AABC', 'ABBC', 'ABCB', 'ABCD'],
  // Level 6: hardest tier — only period 4-5 with mostly 4+ distinct
  // items. Introduces period 5 (ABCDE = 5 distinct items in a row).
  ['ABCD', 'AABCD', 'ABCBD', 'ABCDE'],
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

  // Show enough cells that the repetition is obvious. For period-2
  // patterns we want 3 full reps so AB doesn't look like a one-off
  // pair. Period-3 and 4 are clearer at 2 reps already.
  const period = template.length;
  const fullReps = period === 2 ? 3 : 2;

  // Partial tail (cells beyond the last full rep) lets the missing
  // slot land *mid-cycle* — pedagogically this is the harder and more
  // useful case: instead of always answering "the cycle starts over",
  // the child has to figure out where in the cycle the gap falls.
  // Bias strongly toward showing *some* partial: only 1 in 5 rounds
  // shows a clean cycle break, the rest end mid-cycle.
  const tailMax = period - 1;
  let partialLen: number;
  if (tailMax <= 0 || rng() < 0.2) {
    partialLen = 0;
  } else {
    partialLen = 1 + Math.floor(rng() * tailMax);
  }

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

/** Build the choice buttons. Every distinct item from the visible
 *  sequence is always included so the kid sees all the building blocks
 *  on the row as tappable options. In hard mode the choice list is
 *  padded to at least 4 with distractors from the wider theme pool. */
export function buildChoices(
  round: PatternRound,
  mode: 'easy' | 'hard',
  pool: Item[],
  rng: () => number = Math.random,
): Item[] {
  const correct = round.answer;
  const fromUnit = round.unitItems.filter((it) => it.id !== correct.id);
  if (mode === 'easy') {
    return shuffle([correct, ...fromUnit], rng);
  }
  const targetCount = Math.max(4, round.unitItems.length);
  const needed = targetCount - 1 - fromUnit.length;
  const unitIds = new Set(round.unitItems.map((it) => it.id));
  const extras = shuffle(
    pool.filter((it) => !unitIds.has(it.id)),
    rng,
  ).slice(0, Math.max(0, needed));
  return shuffle([correct, ...fromUnit, ...extras], rng);
}
