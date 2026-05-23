// Parent-only settings panel. Opened via long-press on the hazelnut.
// Holds sync-token + sync-endpoint inputs. Not a kid-facing UI.

import { loadShared, saveShared } from './settings.js';
import { generateToken, DEFAULT_ENDPOINT } from './sync.js';

let openPanel: HTMLElement | null = null;

export function openParentSettings(): void {
  if (openPanel) return;
  const s = loadShared();

  const panel = document.createElement('div');
  panel.className = 'parent-settings-panel';
  panel.innerHTML = `
    <div class="parent-settings-card" role="dialog" aria-label="Parent settings">
      <h2>Parent settings</h2>

      <div class="setting-row">
        <label for="parent-token">Sync token</label>
        <input id="parent-token" type="text" autocomplete="off"
          autocapitalize="off" autocorrect="off" spellcheck="false" />
        <p class="hint">Same token on every device. Empty = no sync.</p>
        <div class="parent-token-actions">
          <button class="secondary parent-generate">Generate new</button>
          <button class="secondary parent-clear">Clear</button>
        </div>
      </div>

      <div class="setting-row">
        <label for="parent-endpoint">Sync endpoint</label>
        <input id="parent-endpoint" type="text" autocomplete="off"
          autocapitalize="off" autocorrect="off" spellcheck="false"
          placeholder="${DEFAULT_ENDPOINT}" />
        <p class="hint">Override only if you've moved the worker. Empty = default.</p>
      </div>

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
