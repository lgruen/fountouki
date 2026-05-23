// Home-screen game picker. Hazelnut at top, big tappable cards below.

import type { GameDef } from './games/registry.js';
import { makeMuteButton, makeHomeButton } from './shared/chrome.js';

export function mount(
  container: HTMLElement,
  games: GameDef[],
  onPick: (id: string) => void,
): () => void {
  container.innerHTML = '';

  const view = document.createElement('div');
  view.className = 'picker';

  // Topbar: hazelnut on left (no-op tap on the picker; long-press reserved
  // for parent settings later); mute on right.
  const top = document.createElement('header');
  top.className = 'topbar picker-topbar';
  top.append(
    makeHomeButton({ onHome: () => {} }),
    document.createElement('div'), // flex spacer
    makeMuteButton(),
  );
  const spacer = top.children[1] as HTMLDivElement;
  spacer.style.flex = '1';
  view.append(top);

  // Big centered brand mark.
  const brand = document.createElement('div');
  brand.className = 'picker-brand';
  brand.textContent = '🌰';
  brand.setAttribute('aria-label', 'fountouki');
  view.append(brand);

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
    icon.textContent = g.emoji;
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
