// Phonics mastery section for the parent-settings panel. Reads the
// current letter state and renders a summary + per-letter dot grid.

import { load } from '../../shared/storage.js';
import { INTRODUCED_BOX_MIN, activeLetters, emptyState, ensureLetters, validate } from './srs.js';
import type { ParentSection } from '../../shared/parent-settings.js';

const MASTERED_BOX = 4;
const STRONG_MIN_BOX = 3;

function statsHTML(): string {
  const state = validate(load<unknown>('phonics', 'state')) ?? emptyState();
  ensureLetters(state);
  const entries = Object.entries(state.letters).sort(([a], [b]) => a.localeCompare(b));
  if (entries.every(([, s]) => s.lastSeen === 0)) {
    return `<p class="hint">No phonics play yet.</p>`;
  }
  let mastered = 0;
  let strong = 0;
  let learning = 0;
  let unseen = 0;
  for (const [, s] of entries) {
    if (s.lastSeen === 0) {
      unseen += 1;
      continue;
    }
    if (s.box >= MASTERED_BOX) mastered += 1;
    else if (s.box >= STRONG_MIN_BOX) strong += 1;
    else learning += 1;
  }
  const dots = entries
    .map(
      ([l, s]) =>
        `<span class="mastery-dot box-${s.box}" title="${l} · box ${s.box}" aria-label="${l}: box ${s.box}"></span>`,
    )
    .join('');
  const nextUp = activeLetters(state).filter(
    (l) => (state.letters[l]?.box ?? 0) < INTRODUCED_BOX_MIN,
  );
  const nextLine = nextUp.length
    ? `<p class="hint">In rotation now: <strong>${nextUp.join(' · ')}</strong>. The next letter unlocks when one of these is graded correct.</p>`
    : `<p class="hint">All 26 letters in rotation.</p>`;
  return `
    <div class="parent-stats-summary">
      <span><strong>${mastered}</strong> mastered</span>
      <span><strong>${strong}</strong> strong</span>
      <span><strong>${learning}</strong> learning</span>
      ${unseen ? `<span><strong>${unseen}</strong> new</span>` : ''}
    </div>
    <div class="mastery-grid" aria-label="Per-letter mastery">${dots}</div>
    <p class="hint">Each dot = one letter, colored by Leitner box (gray = new, gold = mastered).</p>
    ${nextLine}`;
}

export function buildPhonicsMasterySection(): ParentSection {
  const section = document.createElement('section');
  section.className = 'parent-section';
  section.innerHTML = `<h3>Phonics mastery</h3>${statsHTML()}`;
  return { element: section };
}
