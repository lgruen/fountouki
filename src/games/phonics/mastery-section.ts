// Phonics mastery section for the parent-settings panel. Reads the
// current letter state and renders a summary + per-letter dot grid.

import { load } from '../../shared/storage.js';
import type { ParentSection } from '../../shared/parent-settings.js';

interface LetterStat {
  box: number;
  lastSeen: number;
}
interface PhonicsBlob {
  letters?: Record<string, LetterStat>;
}

const MASTERED_BOX = 4;
const STRONG_MIN_BOX = 3;

function statsHTML(): string {
  const state = load<PhonicsBlob>('phonics', 'state');
  if (!state || !state.letters) {
    return `<p class="hint">No phonics play yet.</p>`;
  }
  const entries = Object.entries(state.letters).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0) {
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
  return `
    <div class="parent-stats-summary">
      <span><strong>${mastered}</strong> mastered</span>
      <span><strong>${strong}</strong> strong</span>
      <span><strong>${learning}</strong> learning</span>
      ${unseen ? `<span><strong>${unseen}</strong> new</span>` : ''}
    </div>
    <div class="mastery-grid" aria-label="Per-letter mastery">${dots}</div>
    <p class="hint">Each dot = one letter, colored by Leitner box (gray = new, gold = mastered).</p>`;
}

export function buildPhonicsMasterySection(): ParentSection {
  const section = document.createElement('section');
  section.className = 'parent-section';
  section.innerHTML = `<h3>Phonics mastery</h3>${statsHTML()}`;
  return { element: section };
}
