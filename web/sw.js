// Service worker for the fountouki PWA. Network-first (so a deploy lands as soon
// as the device is online) with a cache fallback (so it works offline + installs
// cleanly to the home screen). The fonts + emoji are baked into the .wasm, so the
// shell to cache is just index.html + the JS bundle + the wasm + manifest/icons.
//
// IMPORTANT: the network fetches deliberately bypass the *browser HTTP cache*
// (`cache: 'reload'`/`'no-cache'`). GitHub Pages serves assets with
// `Cache-Control: max-age=600`, so a plain `fetch()` here would keep returning a
// stale `fountouki.wasm` for up to ~10 min after a deploy — even in a normal
// browser tab, since the SW controls every page in scope. Revalidating against
// the origin makes a deploy land on the very next load. Bump CACHE whenever this
// file changes so a new SW activates and purges the old precache.
const CACHE = 'fountouki-v2';
const SHELL = [
  './',
  './index.html',
  './mq_js_bundle.js',
  './sapp_jsutils.js',
  './quad-net.js',
  './text_input.js',
  './storage.js',
  './fountouki.wasm',
  './manifest.webmanifest',
  './icon-180.png',
  './icon-192.png',
  './icon-512.png',
  './icon.svg',
];

self.addEventListener('install', (e) => {
  e.waitUntil(
    caches
      .open(CACHE)
      // `cache: 'reload'` → fetch each shell entry from the network, ignoring the
      // browser HTTP cache, so the precache can't pin a stale build.
      .then((c) => c.addAll(SHELL.map((u) => new Request(u, { cache: 'reload' }))))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (e) => {
  e.respondWith(
    // `cache: 'no-cache'` → always revalidate with the origin (a 304 reuses the
    // HTTP-cached body for free; a changed wasm comes back as a fresh 200). This
    // is what makes a deploy visible immediately instead of after the max-age
    // window. Offline still works via the cache fallback below.
    fetch(e.request, { cache: 'no-cache' })
      .then((resp) => {
        const copy = resp.clone();
        caches.open(CACHE).then((c) => c.put(e.request, copy)).catch(() => {});
        return resp;
      })
      .catch(() => caches.match(e.request))
  );
});
