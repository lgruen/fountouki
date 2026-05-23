// Parent-only settings panel. Opened via long-press on the in-game ←.
// Holds sync-token + sync-endpoint inputs and per-game mastery stats.

import { loadShared, saveShared } from './settings.js';
import { generateToken, DEFAULT_ENDPOINT } from './sync.js';
import { load } from './storage.js';

interface LetterStat {
  box: number;
  lastSeen: number;
}
interface PhonicsBlob {
  letters?: Record<string, LetterStat>;
}

const MASTERED_BOX = 4;
const STRONG_MIN_BOX = 3;

let openPanel: HTMLElement | null = null;

function phonicsStatsHTML(): string {
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

export function openParentSettings(): void {
  if (openPanel) return;
  const s = loadShared();

  const panel = document.createElement('div');
  panel.className = 'parent-settings-panel';
  panel.innerHTML = `
    <div class="parent-settings-card" role="dialog" aria-label="Parent settings">
      <h2>Parent settings</h2>

      <section class="parent-section">
        <h3>Phonics mastery</h3>
        ${phonicsStatsHTML()}
      </section>

      <section class="parent-section">
        <h3>Sync</h3>
        <div class="setting-row">
          <label for="parent-token">Token</label>
          <input id="parent-token" type="text" autocomplete="off"
            autocapitalize="off" autocorrect="off" spellcheck="false" />
          <p class="hint">Same token on every device. Empty = no sync.</p>
          <div class="parent-token-actions">
            <button class="secondary parent-generate">Generate new</button>
            <button class="secondary parent-clear">Clear</button>
          </div>
        </div>

        <div class="setting-row">
          <label for="parent-endpoint">Endpoint</label>
          <input id="parent-endpoint" type="text" autocomplete="off"
            autocapitalize="off" autocorrect="off" spellcheck="false"
            placeholder="${DEFAULT_ENDPOINT}" />
          <p class="hint">Override only if you've moved the worker. Empty = default.</p>
        </div>
      </section>

      <div class="setting-actions">
        <button class="primary parent-close">Done</button>
      </div>
    </div>`;
  document.body.append(panel);
  openPanel = panel;

  const tokenInput = panel.querySelector<HTMLInputElement>('#parent-token')!;
  const endpointInput = panel.querySelector<HTMLInputElement>('#parent-endpoint')!;
  const genBtn = panel.querySelector<HTMLButtonElement>('.parent-generate')!;
  const clearBtn = panel.querySelector<HTMLButtonElement>('.parent-clear')!;
  const closeBtn = panel.querySelector<HTMLButtonElement>('.parent-close')!;

  tokenInput.value = s.syncToken ?? '';
  endpointInput.value = s.syncEndpoint ?? '';

  genBtn.addEventListener('click', () => {
    tokenInput.value = generateToken();
    tokenInput.focus();
    tokenInput.select();
  });
  clearBtn.addEventListener('click', () => {
    tokenInput.value = '';
  });

  function close(): void {
    if (!openPanel) return;
    const token = tokenInput.value.trim();
    const endpoint = endpointInput.value.trim();
    saveShared({
      syncToken: token === '' ? null : token,
      syncEndpoint: endpoint === '' ? null : endpoint,
    });
    openPanel.remove();
    openPanel = null;
  }

  closeBtn.addEventListener('click', close);
  panel.addEventListener('click', (e) => {
    if (e.target === panel) close();
  });
  window.addEventListener(
    'keydown',
    function onKey(e) {
      if (e.key === 'Escape') {
        window.removeEventListener('keydown', onKey);
        close();
      }
    },
  );
}
