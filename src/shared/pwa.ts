// Service worker registration + orientation lock. App-wide; lives in shared
// because it runs once at boot regardless of which game is active.

declare const __BUILD_ID__: string;

export function buildId(): string {
  return __BUILD_ID__;
}

let swRegistration: ServiceWorkerRegistration | null = null;

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
        swRegistration = reg;
        void reg.update();
      })
      .catch(() => {
        /* offline-only registration failure is fine */
      });
  });
}

export type UpdateCheck =
  | { state: 'unsupported' }
  | { state: 'no-registration' }
  | { state: 'error' }
  | { state: 'current' }
  | { state: 'updating' };

// Force a fresh check against the server for a new service worker. Resolves
// once `registration.update()` settles; if the SW byte-diffs, the install →
// activate → `controllerchange` flow already wired up in
// `registerServiceWorker` will reload the page on its own.
export async function checkForUpdate(): Promise<UpdateCheck> {
  if (!('serviceWorker' in navigator)) return { state: 'unsupported' };
  const reg = swRegistration ?? (await navigator.serviceWorker.getRegistration()) ?? null;
  if (!reg) return { state: 'no-registration' };
  try {
    await reg.update();
  } catch {
    return { state: 'error' };
  }
  // After update(): `installing` is set if a byte-diff was found and the new
  // worker is downloading/installing; `waiting` is set if one was already
  // installed but parked behind the active SW. Either means an update is in
  // flight and our controllerchange listener will reload shortly.
  if (reg.installing || reg.waiting) return { state: 'updating' };
  return { state: 'current' };
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
