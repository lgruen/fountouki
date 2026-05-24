// Take screenshots of the built site at several states so we can visually
// review the game without running it on a real device.
//
// Usage: node tools/screenshots.mjs
// Requires: npm run build first (or it will run it automatically).

import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname } from 'node:path';
import { launchChromium } from './_chrome.mjs';

const root = new URL('..', import.meta.url).pathname;
const dist = join(root, 'dist');
const shotsDir = join(root, 'screenshots');
await mkdir(shotsDir, { recursive: true });

// Build first.
await new Promise((resolve, reject) => {
  const child = spawn('node', ['build.mjs'], { cwd: root, stdio: 'inherit' });
  child.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`build failed: ${code}`))));
});

// Serve dist on a free port.
const mime = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
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
console.log('Serving', dist, 'at', url);

const browser = await launchChromium();
const ctx = await browser.newContext({
  // Default to landscape since that's how the game is meant to be played
  // (a rotate-me overlay covers the UI when the viewport is portrait on a
  // phone-sized screen).
  viewport: { width: 844, height: 390 },
  deviceScaleFactor: 1,
});
const page = await ctx.newPage();

// Use a stable RNG by overriding Math.random before any module loads.
await page.addInitScript(() => {
  let seed = 1234567;
  // Mulberry32 — deterministic PRNG.
  Math.random = () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
});

async function snap(name) {
  const file = join(shotsDir, `${name}.png`);
  await page.screenshot({ path: file, fullPage: false });
  console.log('saved', file);
}

// Patterns settings live behind a long-press on the in-game ← (same as
// phonics). Helpers below open / close the parent settings panel that
// hosts them.
async function openPatternsSettings() {
  await page.locator('.home-btn').click({ delay: 700 });
  await page.waitForSelector('.parent-settings-card');
}
async function closePatternsSettings() {
  await page.click('.parent-close');
  await page.waitForSelector('.parent-settings-panel', { state: 'detached' });
}

await page.goto(url);
await page.waitForSelector('.picker-card');
await snap('00-picker');

// Navigate to patterns and wait for the first round to render.
await page.click('.picker-card[data-game="patterns"]');
await page.waitForSelector('.cell');
await snap('01-initial');

// Open settings (long-press on ← in patterns opens the parent settings
// panel; pattern-specific controls live in there now).
await openPatternsSettings();
await snap('02-settings');
await closePatternsSettings();

// Try the "shapes" theme.
await openPatternsSettings();
await page.selectOption('#ptn-theme', 'shapes');
await closePatternsSettings();
await page.waitForSelector('.cell.shape');
await snap('03-shapes');

// Letters lowercase.
await openPatternsSettings();
await page.selectOption('#ptn-theme', 'letters-lower');
await closePatternsSettings();
await page.waitForTimeout(50);
await snap('04-letters-lower');

// Numbers.
await openPatternsSettings();
await page.selectOption('#ptn-theme', 'numbers');
await closePatternsSettings();
await page.waitForTimeout(50);
await snap('05-numbers');

// Construction.
await openPatternsSettings();
await page.selectOption('#ptn-theme', 'emoji-construction');
await closePatternsSettings();
await page.waitForSelector('.cell');
await snap('05a-construction');

// Dinosaurs.
await openPatternsSettings();
await page.selectOption('#ptn-theme', 'emoji-dinosaurs');
await closePatternsSettings();
await page.waitForSelector('.cell');
await snap('05b-dinosaurs');

// Pick a wrong answer to see the "try again" state.
await openPatternsSettings();
await page.selectOption('#ptn-theme', 'emoji-animals');
await closePatternsSettings();
await page.waitForSelector('.cell');
// Click any non-correct choice (the first one — may be right; try them all until wrong).
const choices = await page.$$('.choice');
for (const c of choices) {
  const id = await c.getAttribute('data-id');
  // Heuristic: click first one; if it was correct the page advances, so just snap.
  if (id) {
    await c.click();
    break;
  }
}
await page.waitForTimeout(120);
await snap('06-after-click');

// Find-the-piece mode.
await openPatternsSettings();
await page.selectOption('#ptn-mode', 'unit');
await closePatternsSettings();
await page.waitForTimeout(80);
await snap('07-unit-mode');

// Visualize a higher level (period-3 / period-4 patterns) on the same viewport.
await openPatternsSettings();
await page.selectOption('#ptn-mode', 'next');
await page.selectOption('#ptn-theme', 'emoji-fruit');
await closePatternsSettings();
await page.evaluate(() => {
  // The state object is module-scoped; bump level via the Start Over flow
  // doesn't help. Instead, simulate level-3 by tweaking the difficulty
  // selector + clicking through correct answers — easier to just inject.
  // We expose nothing; just trigger via DOM events for visual approximation.
});
// Click "Start over" through settings then nothing — keep at level 1.
// For a quick visual check at higher difficulty, manually craft a level-3
// round by calling the global hook if exposed. (Left as a TODO; the round
// generator is unit-testable directly.)
await snap('08-level1-fruit');

// Tablet viewport (portrait shouldn't trigger the rotate overlay because
// the width is > 540px).
await page.setViewportSize({ width: 820, height: 1180 });
await page.waitForTimeout(80);
await snap('09-tablet-portrait');

// Tablet landscape (e.g. iPad in landscape, ~1180x820).
await page.setViewportSize({ width: 1180, height: 820 });
await page.waitForTimeout(80);
await snap('10-tablet-landscape');

// Confetti positioning check: trigger a burst on tablet portrait and
// capture a frame mid-animation so we can eyeball whether particles
// land near the play area instead of off-screen / in wrong half.
async function clickCorrectThenSnap(name) {
  // Wait long enough for any previous round's lockout (1100ms in game.ts)
  // to clear so the click actually fires and the burst is fresh.
  await page.waitForTimeout(1200);
  await page.evaluate(() => {
    const w = window;
    const ans = w.__patterns?.answerId;
    if (!ans) return;
    const btn = document.querySelector(`.choice[data-id="${ans}"]`);
    if (btn) btn.click();
  });
  // Sample mid-animation (particles fly up then fall; ~200ms in they're
  // still clustered near the emit point).
  await page.waitForTimeout(220);
  await snap(name);
}

await page.setViewportSize({ width: 820, height: 1180 });
await clickCorrectThenSnap('10b-confetti-tablet-portrait');

await page.setViewportSize({ width: 1180, height: 820 });
await clickCorrectThenSnap('10d-confetti-tablet-landscape');

await page.setViewportSize({ width: 844, height: 390 });
await clickCorrectThenSnap('10c-confetti-phone-landscape');

// Phone in portrait should show the rotate overlay.
await page.setViewportSize({ width: 390, height: 844 });
await page.waitForTimeout(80);
await snap('11-rotate-overlay');

// iPhone Pro Max in landscape — taller status bar / safe-area, slightly
// taller viewport. Confirms the play area still has breathing room
// above the home indicator.
await page.setViewportSize({ width: 932, height: 430 });
await page.waitForTimeout(80);
await snap('12-iphone-promax-landscape');

// ---------- phonics ----------
await page.setViewportSize({ width: 844, height: 390 });
await page.goto(`${url}#/phonics`);
await page.waitForSelector('.phonics-letter');
await snap('20-phonics-phone-landscape');

// Drive partway through (4 stars) so the rainbow is mid-fill.
for (let i = 0; i < 4; i++) {
  await page.click('.phonics-got');
  await page.waitForTimeout(800);
}
await page.waitForTimeout(120);
await snap('20b-phonics-mid-rainbow');

// Trigger a miss to show the hint cue.
await page.click('.phonics-miss');
await page.waitForSelector('.phonics-hint:not([hidden])');
await page.waitForTimeout(320); // let hint-in animation complete
await snap('21-phonics-miss-hint');

// "Got it now" to clear miss state, then drive to a "session done" splash.
await page.click('.phonics-advance');
for (let i = 0; i < 8; i++) {
  const done = await page.locator('.phonics-done:not([hidden])').count();
  if (done > 0) break;
  const adv = await page.locator('.phonics-advance:not([hidden])').count();
  if (adv > 0) await page.click('.phonics-advance');
  else await page.click('.phonics-got');
  await page.waitForTimeout(800);
}
await page.waitForSelector('.phonics-done:not([hidden])');
// Scene settles ~1.6s after open (last plant sprouts then frog idles).
// Wait a beat so the screenshot shows the full garden + frog, not the
// half-drawn rainbow with empty ground.
await page.waitForTimeout(1800);
await snap('22-phonics-rainbow-done');
// Tap the frog so we capture a reaction frame in screenshots too.
// `force` skips the stability wait — the frog has a perpetual idle
// animation, so Playwright otherwise waits forever for it to settle.
await page.click('.phonics-frog', { force: true });
await page.waitForTimeout(220);
await snap('22b-phonics-rainbow-done-frog-tapped');
await page.click('.phonics-done-home');
await page.waitForSelector('.picker-card');

// Tablet portrait + landscape for phonics too.
await page.setViewportSize({ width: 820, height: 1180 });
await page.goto(`${url}#/phonics`);
await page.waitForSelector('.phonics-letter');
await snap('23-phonics-tablet-portrait');

await page.setViewportSize({ width: 1180, height: 820 });
await page.waitForTimeout(80);
await snap('24-phonics-tablet-landscape');

// Drive to the rainbow done scene on tablet landscape (the biggest
// surface area) so we can eyeball the scene at a non-phone aspect.
for (let i = 0; i < 12; i++) {
  const done = await page.locator('.phonics-done:not([hidden])').count();
  if (done > 0) break;
  const adv = await page.locator('.phonics-advance:not([hidden])').count();
  if (adv > 0) await page.click('.phonics-advance');
  else await page.click('.phonics-got');
  await page.waitForTimeout(800);
}
await page.waitForSelector('.phonics-done:not([hidden])');
await page.waitForTimeout(1800);
await snap('24b-phonics-rainbow-done-tablet-landscape');
await page.click('.phonics-done-home');
await page.waitForSelector('.picker-card');

// Same on tablet portrait.
await page.setViewportSize({ width: 820, height: 1180 });
await page.goto(`${url}#/phonics`);
await page.waitForSelector('.phonics-letter');
for (let i = 0; i < 12; i++) {
  const done = await page.locator('.phonics-done:not([hidden])').count();
  if (done > 0) break;
  const adv = await page.locator('.phonics-advance:not([hidden])').count();
  if (adv > 0) await page.click('.phonics-advance');
  else await page.click('.phonics-got');
  await page.waitForTimeout(800);
}
await page.waitForSelector('.phonics-done:not([hidden])');
await page.waitForTimeout(1800);
await snap('23b-phonics-rainbow-done-tablet-portrait');
await page.click('.phonics-done-home');
await page.waitForSelector('.picker-card');

// iPhone Pro Max landscape — the tight-viewport edge case.
await page.setViewportSize({ width: 932, height: 430 });
// We're on the picker after the previous "go home" — bounce back into
// the phonics game so the miss-hint screenshot has something to land on.
await page.goto(`${url}#/phonics`);
await page.waitForSelector('.phonics-letter');
await snap('25-phonics-iphone-promax-landscape');

// Drive to the rainbow done on iPhone Pro Max so we can eyeball the
// scene on the tightest landscape viewport too (notch + safe-area).
for (let i = 0; i < 12; i++) {
  const done = await page.locator('.phonics-done:not([hidden])').count();
  if (done > 0) break;
  const adv = await page.locator('.phonics-advance:not([hidden])').count();
  if (adv > 0) await page.click('.phonics-advance');
  else await page.click('.phonics-got');
  await page.waitForTimeout(800);
}
await page.waitForSelector('.phonics-done:not([hidden])');
await page.waitForTimeout(1800);
await snap('25b-phonics-rainbow-done-promax-landscape');
await page.click('.phonics-done-home');
await page.waitForSelector('.picker-card');
await page.goto(`${url}#/phonics`);
await page.waitForSelector('.phonics-letter');

await page.click('.phonics-miss');
await page.waitForSelector('.phonics-hint:not([hidden])');
await page.waitForTimeout(320); // let hint-in animation complete
await snap('26-phonics-miss-promax');

// Parent settings panel with mastery stats — long-press the in-game ←.
await page.setViewportSize({ width: 844, height: 390 });
// Click advance to leave the miss state.
const advanceVisible = await page.locator('.phonics-advance:not([hidden])').count();
if (advanceVisible > 0) await page.click('.phonics-advance');
await page.locator('.home-btn').click({ delay: 700 });
await page.waitForSelector('.parent-settings-panel');
await snap('27-parent-mastery');

await browser.close();
server.close();
console.log('done');
