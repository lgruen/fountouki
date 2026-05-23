// Service worker registration + orientation lock. App-wide; lives in shared
// because it runs once at boot regardless of which game is active.

declare const __BUILD_ID__: string;

export function buildId(): string {
  return __BUILD_ID__;
}

export function registerServiceWorker(): void {
  if (!('serviceWorker' in navigator)) return;

  const params = new URLSearchParams(location.search);
  // Escape hatch: ?nosw unregisters and clears caches.
  if (params.has('nosw')) {
    void navigator.serviceWorker.getRegistrations().then(async (regs) => {
      await Promise.all(regs.map((r) => r.unregister()));
      const cs = await caches.keys();
      await Promise.all(cs.map((k) => caches.delete(k)));
      location.replace(location.pathname);
    });
    return;
  }
  // Skip on localhost unless explicitly forced — keeps dev iteration fast.
  const host = location.hostname;
  const isLocal = host === 'localhost' || host === '127.0.0.1' || host === '';
  if (isLocal && params.get('sw') !== 'force') return;

  window.addEventListener('load', () => {
    const hadController = navigator.serviceWorker.controller !== null;
    let reloaded = false;
    if (hadController) {
      navigator.serviceWorker.addEventListener('controllerchange', () => {
        if (reloaded) return;
        reloaded = true;
        location.reload();
      });
    }
    void navigator.serviceWorker
      .register('./sw.js')
      .then((reg) => {
        void reg.update();
      })
      .catch(() => {
        /* offline-only registration failure is fine */
      });
  });
}

export function tryLockLandscape(): void {
  const standalone =
    window.matchMedia?.('(display-mode: standalone)').matches ??
    (window.navigator as unknown as { standalone?: boolean }).standalone ??
    false;
  if (!standalone) return;
  const orient = (screen as unknown as { orientation?: { lock?: (o: string) => Promise<void> } })
    .orientation;
  if (orient?.lock) {
    void orient.lock('landscape').catch(() => {
      /* iOS rejects this; fine */
    });
  }
}
