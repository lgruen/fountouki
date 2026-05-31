// Service worker for the fountouki PWA. Network-first (so a deploy lands as soon
// as the device is online) with a cache fallback (so it works offline + installs
// cleanly to the home screen). The fonts + emoji are baked into the .wasm, so the
// shell to cache is just index.html + the JS bundle + the wasm + manifest/icons.
const CACHE = 'fountouki-v1';
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
    caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting())
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
    fetch(e.request)
      .then((resp) => {
        const copy = resp.clone();
        caches.open(CACHE).then((c) => c.put(e.request, copy)).catch(() => {});
        return resp;
      })
      .catch(() => caches.match(e.request))
  );
});
