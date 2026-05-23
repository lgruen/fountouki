// Smoke for the parent settings flow: long-press hazelnut on the picker,
// generate a token, close, reload, confirm it persists.

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import assert from 'node:assert/strict';
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
const ctx = await browser.newContext({ viewport: { width: 844, height: 390 } });
const page = await ctx.newPage();
page.on('pageerror', (err) => {
  console.error('PAGE ERROR:', err.message);
  process.exitCode = 1;
});

console.log('1) load picker; no token in storage');
await page.goto(url);
await page.waitForSelector('.picker-card');
const tokenBefore = await page.evaluate(() => {
  const raw = localStorage.getItem('fountouki.shared.settings.v1');
  return raw ? JSON.parse(raw).syncToken : null;
});
assert.equal(tokenBefore, null, 'fresh storage should have no syncToken');

console.log('2) long-press hazelnut → parent settings appears');
// Hold the mouse down for > 500ms (long-press threshold).
await page.locator('.home-btn').click({ delay: 700 });
await page.waitForSelector('.parent-settings-panel');

console.log('3) generate token');
await page.click('.parent-generate');
const generated = await page.inputValue('#parent-token');
assert(/^[a-z0-9]{16}$/.test(generated), `expected 16-char a-z0-9 token, got: ${generated}`);

console.log('4) close → token persists in localStorage');
await page.click('.parent-close');
await page.waitForSelector('.parent-settings-panel', { state: 'detached' });
const stored = await page.evaluate(() => {
  const raw = localStorage.getItem('fountouki.shared.settings.v1');
  return raw ? JSON.parse(raw).syncToken : null;
});
assert.equal(stored, generated, 'token should persist');

console.log('5) reload → re-opening shows persisted token');
await page.reload();
await page.waitForSelector('.picker-card');
await page.locator('.home-btn').click({ delay: 700 });
await page.waitForSelector('.parent-settings-panel');
const reloaded = await page.inputValue('#parent-token');
assert.equal(reloaded, generated, 'reload should show persisted token');

console.log('6) clear → empty token, persists null');
await page.click('.parent-clear');
const cleared = await page.inputValue('#parent-token');
assert.equal(cleared, '', 'cleared field is empty');
await page.click('.parent-close');
const afterClear = await page.evaluate(() => {
  const raw = localStorage.getItem('fountouki.shared.settings.v1');
  return raw ? JSON.parse(raw).syncToken : 'missing';
});
assert.equal(afterClear, null, 'cleared token should persist as null');

await browser.close();
server.close();
console.log('\nOK — parent settings flow persists token across reload.');
