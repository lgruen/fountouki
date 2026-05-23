// Minimal smoke test for the service worker.
//
// 1) Loads page, waits for game render
// 2) Waits for SW to take control
// 3) Reads cache contents
// 4) Sets browser offline, reloads, asserts game still renders
//
// Usage: node tools/sw-offline-test.mjs

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { launchChromium } from './_chrome.mjs';

const root = new URL('..', import.meta.url).pathname;
const dist = join(root, 'dist');

await new Promise((resolve, reject) => {
  const child = spawn('node', ['build.mjs'], { cwd: root, stdio: 'inherit' });
  child.on('exit', (c) => (c === 0 ? resolve() : reject(new Error(`build failed: ${c}`))));
});

const mime = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.webmanifest': 'application/manifest+json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.map': 'application/json',
};
const server = createServer(async (req, res) => {
  try {
    let url = (req.url ?? '/').split('?')[0] ?? '/';
    if (url.endsWith('/')) url += 'index.html';
    const data = await readFile(join(dist, url));
    res.writeHead(200, { 'Content-Type': mime[extname(url)] ?? 'application/octet-stream' });
    res.end(data);
  } catch {
    res.writeHead(404).end('not found');
  }
});
await new Promise((r) => server.listen(0, r));
const port = server.address().port;
const url = `http://127.0.0.1:${port}/`;

const browser = await launchChromium();
// Landscape viewport — the rotate-to-landscape overlay hides the game
// in portrait orientation.
const ctx = await browser.newContext({ viewport: { width: 844, height: 390 } });
const page = await ctx.newPage();
// The registration code skips SW install on localhost so dev iteration
// isn't blocked by cached assets. Opt in for this test with ?sw=force.
const testUrl = `${url}?sw=force#/patterns`;

page.on('console', (msg) => {
  const t = msg.type();
  if (t === 'error' || t === 'warning') console.log(`  [page ${t}]`, msg.text());
});
page.on('pageerror', (err) => console.error('  [page error]', err.message));
page.on('requestfailed', (req) => console.error('  [req failed]', req.url(), req.failure()?.errorText));

console.log('1) load with SW registration...');
await page.goto(testUrl, { waitUntil: 'load' });
await page.waitForSelector('.cell');
console.log('   game rendered');

// Wait until a SW is controlling this page. Native control via clients.claim()
// from sw.js happens after install + activate.
await page.waitForFunction(
  () => navigator.serviceWorker.controller !== null,
  null,
  { timeout: 10000 },
);
console.log('   SW controls page');

const cached = await page.evaluate(async () => {
  const names = await caches.keys();
  const out = {};
  for (const n of names) {
    const c = await caches.open(n);
    const keys = await c.keys();
    out[n] = keys.map((k) => new URL(k.url).pathname);
  }
  return out;
});
console.log('   cache contents:');
for (const [name, keys] of Object.entries(cached)) {
  console.log(`     ${name}: ${keys.length} entries`);
  for (const k of keys) console.log(`       ${k}`);
}

console.log('2) go offline, reload, expect game to still work...');
await ctx.setOffline(true);
// Reload without ?sw=force — already registered — but keep the hash so
// patterns mounts directly and exercises the bundled JS, not just the picker.
await page.goto(`${url}#/patterns`, { waitUntil: 'load' });
await page.waitForSelector('.cell', { timeout: 5000 });
const cellCount = await page.locator('.cell').count();
const choiceCount = await page.locator('.choice').count();
console.log(`   offline render OK — ${cellCount} cells, ${choiceCount} choices ✓`);

await browser.close();
server.close();
console.log('\nOK — service worker installs, precaches, and serves offline.');
