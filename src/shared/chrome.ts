// Shared header buttons. Each game builds its own topbar but reuses these.

import { loadShared, toggleMuted } from './settings.js';
import { openParentSettings } from './parent-settings.js';

const LONG_PRESS_MS = 500;

export interface HomeOpts {
  onHome: () => void;
  /** Long-press handler; defaults to opening parent settings. */
  onLongPress?: () => void;
}

export function makeHomeButton(opts: HomeOpts): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.className = 'icon-btn home-btn';
  btn.textContent = '←';
  btn.setAttribute('aria-label', 'Home');

  let pressTimer: number | null = null;
  let longFired = false;

  const longPress = opts.onLongPress ?? openParentSettings;
  const start = () => {
    longFired = false;
    pressTimer = window.setTimeout(() => {
      longFired = true;
      longPress();
    }, LONG_PRESS_MS);
  };
  const end = () => {
    if (pressTimer !== null) {
      clearTimeout(pressTimer);
      pressTimer = null;
    }
  };

  btn.addEventListener('pointerdown', start);
  btn.addEventListener('pointerup', end);
  btn.addEventListener('pointercancel', end);
  btn.addEventListener('pointerleave', end);
  btn.addEventListener('click', () => {
    if (!longFired) opts.onHome();
  });

  return btn;
}

export function makeMuteButton(): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.className = 'icon-btn mute-btn';
  btn.setAttribute('aria-label', 'Mute sounds');
  btn.setAttribute('aria-pressed', 'false');

  const sound = document.createElement('span');
  sound.className = 'icon-sound';
  sound.textContent = '🔊';
  const muted = document.createElement('span');
  muted.className = 'icon-muted';
  muted.textContent = '🔇';
  muted.hidden = true;
  btn.append(sound, muted);

  const paint = (m: boolean) => {
    sound.hidden = m;
    muted.hidden = !m;
    btn.setAttribute('aria-pressed', String(m));
  };
  paint(loadShared().muted);

  btn.addEventListener('click', () => paint(toggleMuted()));

  return btn;
}
