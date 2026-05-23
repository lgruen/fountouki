// Home-screen game picker. Hazelnut at top, big tappable cards below.

import type { GameDef } from './games/registry.js';
import { makeMuteButton } from './shared/chrome.js';

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

  container.append(view);

  return () => {
    container.innerHTML = '';
  };
}
