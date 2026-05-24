// Parent-only settings panel. Opened via long-press on the in-game ←.
// Always shows the sync controls; each game can pass its own section
// (mastery dots, theme/difficulty pickers, …) to slot in above sync.

import { loadShared, saveShared } from './settings.js';
import { generateToken, DEFAULT_ENDPOINT } from './sync.js';

export interface ParentSection {
  /** DOM to slot in above the sync controls. */
  element: HTMLElement;
  /** Called once mounted, with a way to dismiss the panel (used by
   *  reset / start-over buttons that should close on click). */
  onMount?: (api: { close: () => void }) => void;
}

export interface ParentSettingsOpts {
  section?: ParentSection;
}

let openPanel: HTMLElement | null = null;

export function openParentSettings(opts: ParentSettingsOpts = {}): void {
  if (openPanel) return;
  const s = loadShared();

  const panel = document.createElement('div');
  panel.className = 'parent-settings-panel';
  panel.innerHTML = `
    <div class="parent-settings-card" role="dialog" aria-label="Parent settings">
      <h2>Parent settings</h2>

      <div class="parent-section-slot"></div>

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

  const sectionSlot = panel.querySelector<HTMLDivElement>('.parent-section-slot')!;
  if (opts.section) {
    sectionSlot.append(opts.section.element);
  } else {
    sectionSlot.remove();
  }

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

  opts.section?.onMount?.({ close });
}
