// End-to-end smoke for the home picker.
//
// Verifies: picker renders, tapping a game card mounts the game, tapping
// the in-game home button returns to the picker.

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import assert from 'node:assert/strict';
import { launchBrowser, BROWSER } from './_browser.mjs';

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

console.log(`[picker-test] browser=${BROWSER}`);
const browser = await launchBrowser();
const ctx = await browser.newContext({ viewport: { width: 844, height: 390 }, deviceScaleFactor: 1 });
const page = await ctx.newPage();
page.on('pageerror', (err) => {
  console.error('PAGE ERROR:', err.message);
  process.exitCode = 1;
});

console.log('1) load picker');
await page.goto(url);
await page.waitForSelector('.picker-card');
const cards = await page.locator('.picker-card').count();
assert(cards >= 1, 'expected at least one game card');
// No in-app hazelnut (launcher icon only).
const noBrand = await page.locator('.picker-brand').count();
assert.equal(noBrand, 0, 'no hazelnut brand in-app');
const homeBtnOnPicker = await page.locator('.picker .home-btn').count();
assert.equal(homeBtnOnPicker, 0, 'no home button on the picker itself');

// Build stamp: present, non-empty, rendered inside the viewport, and far
// enough above the bottom edge to clear Android's gesture-nav strip in
// landscape phone mode (the bug this test was added for: at 10px / 0.5
// opacity / bottom 6px it was effectively invisible on a real device).
const stamp = await page.evaluate(() => {
  const el = document.querySelector('.picker-version');
  if (!el) return null;
  const r = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  return {
    text: el.textContent ?? '',
    rect: { x: r.x, y: r.y, width: r.width, height: r.height, bottom: r.bottom },
    style: { opacity: parseFloat(cs.opacity), fontSize: parseFloat(cs.fontSize) },
    vh: window.innerHeight,
  };
});
assert(stamp !== null, 'expected .picker-version element');
assert(stamp.text.length > 0, `expected non-empty build stamp (got "${stamp.text}")`);
assert(stamp.rect.width > 0 && stamp.rect.height > 0, 'build stamp has non-zero size');
assert(stamp.style.fontSize >= 12, `build stamp font-size >= 12 (got ${stamp.style.fontSize})`);
assert(stamp.style.opacity >= 0.7, `build stamp opacity >= 0.7 (got ${stamp.style.opacity})`);
const bottomGap = stamp.vh - stamp.rect.bottom;
assert(bottomGap >= 8, `build stamp clears bottom edge (gap=${bottomGap}px)`);
assert(stamp.rect.bottom <= stamp.vh, 'build stamp fully inside viewport');

console.log('2) tap patterns card → patterns mounts');
await page.click('.picker-card[data-game="patterns"]');
await page.waitForSelector('.cell');
const hash = await page.evaluate(() => location.hash);
assert.equal(hash, '#/patterns', 'expected #/patterns');
const debug = await page.evaluate(() => window.__patterns);
assert(debug && debug.answerId, 'patterns debug hook should expose a round');

console.log('3) home button → back to picker');
await page.click('.home-btn');
await page.waitForSelector('.picker-card');
const homeHash = await page.evaluate(() => location.hash);
assert.equal(homeHash, '#/', 'expected #/');

console.log('4) unknown game → bounces back to picker');
await page.goto(`${url}#/no-such-game`);
await page.waitForSelector('.picker-card');
const finalHash = await page.evaluate(() => location.hash);
assert.equal(finalHash, '#/', 'unknown game should fall back to picker');

await browser.close();
server.close();
console.log('\nOK — picker mounts, routes, and unmounts games.');
