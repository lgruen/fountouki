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
  // Explicit width/height attributes on the SVG — without them, WebKit
  // in a flex container sizes the SVG from its (undefined) intrinsic
  // dimensions instead of the CSS percentage, producing a tiny non-square
  // chevron on iPad / iOS Safari. The CSS still scales it via 1em.
  btn.innerHTML =
    '<svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true" focusable="false">' +
    '<path d="M14 6l-6 6 6 6" fill="none" stroke="currentColor" ' +
    'stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>' +
    '</svg>';
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
