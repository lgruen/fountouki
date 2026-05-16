// Service worker for Pattern Play. Cache-first for static assets, network-first
// for the HTML shell so updates are picked up promptly.
//
// The build id and precache list are stamped into BUILD_ID / PRECACHE below
// by build.mjs. Editing the file by hand is fine — just leave those two
// constants assignable.

/// <reference lib="webworker" />
/* eslint-disable no-undef */

const BUILD_ID = '__BUILD_ID__';
const CACHE_NAME = `patternplay-${BUILD_ID}`;
const PRECACHE = __PRECACHE__;

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(PRECACHE)).then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names.filter((n) => n.startsWith('patternplay-') && n !== CACHE_NAME).map((n) => caches.delete(n)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;

  const url = new URL(req.url);
  // Only handle same-origin requests.
  if (url.origin !== self.location.origin) return;

  // For navigations and the HTML shell, prefer the network so we pick up
  // a redeploy on the next launch, falling back to cache when offline.
  if (req.mode === 'navigate' || url.pathname.endsWith('.html') || url.pathname === '/' || url.pathname.endsWith('/index.html')) {
    event.respondWith(
      (async () => {
        try {
          const fresh = await fetch(req);
          const cache = await caches.open(CACHE_NAME);
          cache.put(req, fresh.clone());
          return fresh;
        } catch {
          const cached = await caches.match(req, { ignoreSearch: true });
          if (cached) return cached;
          const fallback = await caches.match('./index.html');
          if (fallback) return fallback;
          return new Response('offline', { status: 503, statusText: 'offline' });
        }
      })(),
    );
    return;
  }

  // Assets: cache-first, populate on miss.
  event.respondWith(
    (async () => {
      const cached = await caches.match(req);
      if (cached) return cached;
      try {
        const fresh = await fetch(req);
        if (fresh.ok) {
          const cache = await caches.open(CACHE_NAME);
          cache.put(req, fresh.clone());
        }
        return fresh;
      } catch {
        return new Response('', { status: 504, statusText: 'gateway timeout' });
      }
    })(),
  );
});
