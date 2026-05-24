// Home-screen game picker. Hazelnut at top, big tappable cards below.

import type { GameDef } from './games/registry.js';
import { makeMuteButton } from './shared/chrome.js';
import { buildId } from './shared/pwa.js';

export function mount(
  container: HTMLElement,
  games: GameDef[],
  onPick: (id: string) => void,
): () => void {
  container.innerHTML = '';

  const view = document.createElement('div');
  view.className = 'picker';

  // Topbar: just mute (right-aligned). Parent settings access lives on
  // the in-game ← back button's long-press, not here.
  const top = document.createElement('header');
  top.className = 'topbar picker-topbar';
  const spacer = document.createElement('div');
  spacer.style.flex = '1';
  top.append(spacer, makeMuteButton());
  view.append(top);

  // Card grid.
  const grid = document.createElement('div');
  grid.className = 'picker-grid';
  for (const g of games) {
    const card = document.createElement('button');
    card.className = 'picker-card';
    card.setAttribute('data-game', g.id);
    card.setAttribute('aria-label', g.label);

    const icon = document.createElement('div');
    icon.className = 'picker-icon';
    if (g.renderIcon) g.renderIcon(icon);
    else icon.textContent = g.emoji;
    card.append(icon);

    const label = document.createElement('div');
    label.className = 'picker-label';
    label.textContent = g.label;
    card.append(label);

    card.addEventListener('click', () => onPick(g.id));
    grid.append(card);
  }
  view.append(grid);

  // Discreet build stamp at the bottom — quick eyeball check that the
  // device is actually running the newest deploy. Rendered in the
  // device's local time so what you see matches "when did I push?".
  const version = document.createElement('div');
  version.className = 'picker-version';
  version.textContent = formatBuildStamp(buildId());
  view.append(version);

  container.append(view);

  return () => {
    container.innerHTML = '';
  };
}

// Build id format: compact UTC ISO "YYYYMMDDTHHMMSS" (see build.mjs).
// Render as "YYYY-MM-DD HH:mm" in local time. Returns the raw id on
// any parse failure rather than throwing — diagnostics shouldn't crash
// the home screen.
function formatBuildStamp(id: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})$/.exec(id);
  if (!m) return id;
  const [, y, mo, d, h, mi, s] = m;
  const iso = `${y}-${mo}-${d}T${h}:${mi}:${s}Z`;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return id;
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}
