// App entry. Hash-routed: #/ -> picker, #/<game-id> -> that game.

import { parseHash, navigate } from './router.js';
import { GAMES } from './games/registry.js';
import { mount as mountPicker } from './picker.js';
import { applyOnBoot } from './shared/settings.js';
import { migrateLegacy } from './shared/storage.js';
import { registerServiceWorker, tryLockLandscape, buildId } from './shared/pwa.js';

const appEl = document.getElementById('app');
if (!(appEl instanceof HTMLElement)) throw new Error('missing #app');
const app: HTMLElement = appEl;

let unmount: (() => void) | null = null;

function render(): void {
  if (unmount) unmount();
  unmount = null;
  const r = parseHash(location.hash);
  if (r.name === 'picker') {
    unmount = mountPicker(app, GAMES, (id) => navigate({ name: 'game', id }));
    return;
  }
  const game = GAMES.find((g) => g.id === r.id);
  if (!game) {
    navigate({ name: 'picker' });
    return;
  }
  unmount = game.mount(app, { onHome: () => navigate({ name: 'picker' }) });
}

migrateLegacy();
applyOnBoot();
window.addEventListener('hashchange', render);
render();
tryLockLandscape();
registerServiceWorker();

// Build id on window for adhoc debugging.
(window as unknown as { __fountouki_build?: string }).__fountouki_build = buildId();
