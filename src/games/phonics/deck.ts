// Phonics deck. Each lowercase letter has a canonical exemplar (shown
// as the miss-hint cue early on) and an optional variants list (used
// when the letter graduates past Leitner box 2 to support generalization).

export interface Exemplar {
  /** Single emoji; kept tight, no decoration around the target stimulus. */
  emoji: string;
  /** Word for parent reference; not displayed automatically. */
  word: string;
}

export interface LetterCard {
  letter: string; // lowercase a–z
  canonical: Exemplar;
  variants?: Exemplar[];
}

// Canonical exemplars chosen for clean phonemic match (short vowel sounds
// where applicable) and concrete, kid-recognizable imagery.
export const DECK: LetterCard[] = [
  { letter: 'a', canonical: { emoji: '🐜', word: 'ant' }, variants: [
      { emoji: '🍎', word: 'apple' }, { emoji: '🐊', word: 'alligator' } ] },
  { letter: 'b', canonical: { emoji: '🐻', word: 'bear' }, variants: [
      { emoji: '🦋', word: 'butterfly' }, { emoji: '🎈', word: 'balloon' } ] },
  { letter: 'c', canonical: { emoji: '🐱', word: 'cat' }, variants: [
      { emoji: '🥕', word: 'carrot' }, { emoji: '🐄', word: 'cow' } ] },
  { letter: 'd', canonical: { emoji: '🐕', word: 'dog' }, variants: [
      { emoji: '🦆', word: 'duck' }, { emoji: '🦖', word: 'dinosaur' } ] },
  { letter: 'e', canonical: { emoji: '🐘', word: 'elephant' }, variants: [
      { emoji: '🥚', word: 'egg' } ] },
  { letter: 'f', canonical: { emoji: '🐟', word: 'fish' }, variants: [
      { emoji: '🐸', word: 'frog' }, { emoji: '🌸', word: 'flower' } ] },
  { letter: 'g', canonical: { emoji: '🐐', word: 'goat' }, variants: [
      { emoji: '🍇', word: 'grapes' }, { emoji: '🎁', word: 'gift' } ] },
  { letter: 'h', canonical: { emoji: '🐴', word: 'horse' }, variants: [
      { emoji: '🏠', word: 'house' }, { emoji: '🎩', word: 'hat' } ] },
  { letter: 'i', canonical: { emoji: '🐛', word: 'insect' }, variants: [
      { emoji: '🪻', word: 'iris' } ] },
  { letter: 'j', canonical: { emoji: '🪼', word: 'jellyfish' }, variants: [
      { emoji: '🎷', word: 'jazz' }, { emoji: '🃏', word: 'joker' } ] },
  { letter: 'k', canonical: { emoji: '🦘', word: 'kangaroo' }, variants: [
      { emoji: '🗝️', word: 'key' }, { emoji: '🪁', word: 'kite' } ] },
  { letter: 'l', canonical: { emoji: '🦁', word: 'lion' }, variants: [
      { emoji: '🍋', word: 'lemon' }, { emoji: '🐞', word: 'ladybug' } ] },
  { letter: 'm', canonical: { emoji: '🐵', word: 'monkey' }, variants: [
      { emoji: '🌙', word: 'moon' }, { emoji: '🍄', word: 'mushroom' } ] },
  { letter: 'n', canonical: { emoji: '🪺', word: 'nest' }, variants: [
      { emoji: '👃', word: 'nose' }, { emoji: '🥜', word: 'nut' } ] },
  { letter: 'o', canonical: { emoji: '🐙', word: 'octopus' }, variants: [
      { emoji: '🦉', word: 'owl' }, { emoji: '🍊', word: 'orange' } ] },
  { letter: 'p', canonical: { emoji: '🐼', word: 'panda' }, variants: [
      { emoji: '🍍', word: 'pineapple' }, { emoji: '🐧', word: 'penguin' } ] },
  { letter: 'q', canonical: { emoji: '👸', word: 'queen' }, variants: [
      { emoji: '🪶', word: 'quill' }, { emoji: '❓', word: 'question' } ] },
  { letter: 'r', canonical: { emoji: '🌈', word: 'rainbow' }, variants: [
      { emoji: '🐰', word: 'rabbit' }, { emoji: '🤖', word: 'robot' } ] },
  { letter: 's', canonical: { emoji: '☀️', word: 'sun' }, variants: [
      { emoji: '🐍', word: 'snake' }, { emoji: '⭐', word: 'star' } ] },
  { letter: 't', canonical: { emoji: '🐢', word: 'turtle' }, variants: [
      { emoji: '🐅', word: 'tiger' }, { emoji: '🌳', word: 'tree' } ] },
  { letter: 'u', canonical: { emoji: '☂️', word: 'umbrella' }, variants: [
      { emoji: '🆙', word: 'up' } ] },
  { letter: 'v', canonical: { emoji: '🚐', word: 'van' }, variants: [
      { emoji: '🎻', word: 'violin' }, { emoji: '🌋', word: 'volcano' } ] },
  { letter: 'w', canonical: { emoji: '🐳', word: 'whale' }, variants: [
      { emoji: '🌊', word: 'wave' }, { emoji: '🍉', word: 'watermelon' } ] },
  { letter: 'x', canonical: { emoji: '🩻', word: 'x-ray' }, variants: [
      { emoji: '📦', word: 'box' }, { emoji: '6️⃣', word: 'six' } ] },
  { letter: 'y', canonical: { emoji: '🪀', word: 'yo-yo' }, variants: [
      { emoji: '🟡', word: 'yellow' } ] },
  { letter: 'z', canonical: { emoji: '🦓', word: 'zebra' }, variants: [
      { emoji: '0️⃣', word: 'zero' }, { emoji: '💤', word: 'zzz' } ] },
];

export const LETTERS: string[] = DECK.map((c) => c.letter);

const BY_LETTER = new Map(DECK.map((c) => [c.letter, c]));
export function getCard(letter: string): LetterCard | undefined {
  return BY_LETTER.get(letter);
}

/** Pick an exemplar for a letter at the given Leitner box.
 *  Variants unlock at box >= 2 so the kid actually sees some variety in
 *  normal play (box 3 = 24h interval means most letters never reach it
 *  in early sessions). */
export function pickExemplar(letter: string, box: number, rng = Math.random): Exemplar {
  const card = BY_LETTER.get(letter);
  if (!card) throw new Error(`unknown letter: ${letter}`);
  if (box < 2 || !card.variants?.length) return card.canonical;
  const pool = [card.canonical, ...card.variants];
  return pool[Math.floor(rng() * pool.length)] ?? card.canonical;
}
