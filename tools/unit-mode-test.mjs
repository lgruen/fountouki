// Smoke test for the new "Find the repeating piece" mode.
//
// Verifies:
//  - The submit button is hidden until at least one cell is selected.
//  - Tapping cells builds a contiguous selection that can extend
//    from either end.
//  - Tapping a non-adjacent cell does NOT change the selection.
//  - A correct submission (length == period, any start) advances the round.
//  - A wrong submission (different length) doesn't advance, and the
//    selection resets after a brief red flash.

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { chromium } from 'playwright';
import assert from 'node:assert/strict';

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

const execPath = process.env.CHROME_PATH ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';
const browser = await chromium.launch({ executablePath: execPath });
const ctx = await browser.newContext({ viewport: { width: 844, height: 390 }, deviceScaleFactor: 1 });
await ctx.addInitScript(() => {
  let s = 9001;
  Math.random = () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
});
const page = await ctx.newPage();
page.on('pageerror', (err) => console.error('PAGE ERROR:', err.message));

console.log('1) load and switch to unit mode');
await page.goto(url);
await page.waitForSelector('.cell');
await page.click('#settings-btn');
await page.selectOption('#mode-select', 'unit');
await page.click('#close-settings');
await page.waitForSelector('.cell.selectable');

const period = await page.evaluate(() => window.__patternplay?.template?.length);
const visible = await page.evaluate(() => window.__patternplay?.visibleIds?.length);
console.log(`   period=${period}, visible=${visible}`);
assert(typeof period === 'number' && period >= 2, 'period missing');
assert(typeof visible === 'number' && visible > period, 'no visible cells');

// Submit button starts hidden.
let submitHidden = await page.evaluate(
  () => document.querySelector('.unit-submit')?.hasAttribute('hidden') ?? null,
);
assert.equal(submitHidden, true, 'submit should be hidden initially');

console.log('2) tap first cell, submit appears');
await page.locator('.cell.selectable').nth(0).click();
submitHidden = await page.evaluate(
  () => document.querySelector('.unit-submit')?.hasAttribute('hidden'),
);
assert.equal(submitHidden, false, 'submit should appear after first tap');

console.log('3) extend right; selection grows');
await page.locator('.cell.selectable').nth(1).click();
let selCount = await page.locator('.cell.unit-pick').count();
assert.equal(selCount, 2, 'should have 2 cells selected');

console.log('4) tap a non-adjacent cell; selection unchanged');
await page.locator('.cell.selectable').nth(4).click();
selCount = await page.locator('.cell.unit-pick').count();
assert.equal(selCount, 2, 'selection should not change on non-adjacent tap');

console.log('5) shrink from right edge');
await page.locator('.cell.selectable').nth(1).click();
selCount = await page.locator('.cell.unit-pick').count();
assert.equal(selCount, 1, 'selection should shrink back to 1');

console.log('6) submit a length-1 selection (wrong unless period === 1)');
const startStars = await page.evaluate(() => window.__patternplay?.stars ?? 0);
await page.locator('.unit-submit').click({ force: true });
await page.waitForTimeout(800);
const afterWrong = await page.evaluate(() => window.__patternplay?.stars ?? 0);
if (period === 1) {
  assert.equal(afterWrong, startStars + 1, 'period 1 means length 1 is correct');
} else {
  assert.equal(afterWrong, startStars, 'wrong submit should NOT award a star');
  // After the reset, no cells should be picked.
  const left = await page.locator('.cell.unit-pick').count();
  assert.equal(left, 0, 'selection should reset after wrong');
}

console.log('7) build a correct-length selection starting at a NON-zero offset');
// Pick cells [1, 1+period). Tap cell 1 to start, then extend right
// (period - 1) times.
await page.locator('.cell.selectable').nth(1).click();
for (let i = 0; i < period - 1; i++) {
  await page.locator('.cell.selectable').nth(2 + i).click();
}
selCount = await page.locator('.cell.unit-pick').count();
assert.equal(selCount, period, 'should have `period` cells selected');

const prevAnswerId = await page.evaluate(() => window.__patternplay?.answerId);
await page.locator('.unit-submit').click({ force: true });
await page.waitForFunction(
  (prev) => window.__patternplay?.answerId && window.__patternplay.answerId !== prev,
  prevAnswerId,
  { timeout: 4000 },
);
const finalStars = await page.evaluate(() => window.__patternplay?.stars ?? 0);
assert(finalStars > startStars, 'correct-length submission should earn a star');

await browser.close();
server.close();
console.log('\nOK — unit mode behaves as expected.');
