// Take screenshots of the built site at several states so we can visually
// review the game without running it on a real device.
//
// Runs the full scene walk on BOTH browser engines:
//   - chromium  → screenshots/chromium/        (desktop / Android Chrome)
//   - webkit    → screenshots/webkit/          (iOS / macOS Safari engine)
//
// On top of the synthetic viewport sizes, the WebKit pass also emulates a
// few real iOS device profiles (proper UA, devicePixelRatio, isMobile) so
// iPad / iPhone layout regressions show up before deploy. Output for those
// lands under screenshots/webkit-ios/<device>/.
//
// Usage: node tools/screenshots.mjs
//        ONLY=chromium node tools/screenshots.mjs   # skip webkit
//        ONLY=webkit   node tools/screenshots.mjs   # skip chromium

import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname } from 'node:path';
import { launchChromium, launchWebkit, devices } from './_browser.mjs';

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

// Stable RNG so each engine sees the same sequence of rounds — makes
// chromium vs webkit screenshots directly comparable.
async function applySeed(ctxOrPage) {
  await ctxOrPage.addInitScript(() => {
    let seed = 1234567;
    Math.random = () => {
      seed |= 0;
      seed = (seed + 0x6d2b79f5) | 0;
      let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  });
}

// The bulk of the scene walk: every gameplay state we want to eyeball,
// captured at the viewport sizes the kids' devices actually use. Driven
// by setViewportSize so it composes with any base context (synthetic
// landscape, or an iOS device profile that supplies UA + DPR).
async function runScenes(page, snapTo) {
  async function snap(name) {
    await snapTo(name);
  }

  async function openPatternsSettings() {
    await page.locator('.home-btn').click({ delay: 700 });
    await page.waitForSelector('.parent-settings-card');
  }
  async function closePatternsSettings() {
    await page.click('.parent-close');
    await page.waitForSelector('.parent-settings-panel', { state: 'detached' });
  }

  await page.setViewportSize({ width: 844, height: 390 });
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
  const choices = await page.$$('.choice');
  for (const c of choices) {
    const id = await c.getAttribute('data-id');
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

  // Switch back to "next" + a fresh theme so 08 mirrors normal play.
  await openPatternsSettings();
  await page.selectOption('#ptn-mode', 'next');
  await page.selectOption('#ptn-theme', 'emoji-fruit');
  await closePatternsSettings();
  await snap('08-level1-fruit');

  // Tablet portrait (e.g. iPad held vertically, ~820x1180). Width > 540px
  // so the rotate-to-landscape overlay stays hidden.
  await page.setViewportSize({ width: 820, height: 1180 });
  await page.waitForTimeout(80);
  await snap('09-tablet-portrait');

  // Tablet landscape (e.g. iPad in landscape, ~1180x820).
  await page.setViewportSize({ width: 1180, height: 820 });
  await page.waitForTimeout(80);
  await snap('10-tablet-landscape');

  // Confetti positioning check: trigger a burst at each device class and
  // capture a frame mid-animation so we can eyeball whether particles
  // land near the play area instead of off-screen / in wrong half.
  async function clickCorrectThenSnap(name) {
    await page.waitForTimeout(1200);
    await page.evaluate(() => {
      const w = window;
      const ans = w.__patterns?.answerId;
      if (!ans) return;
      const btn = document.querySelector(`.choice[data-id="${ans}"]`);
      if (btn) btn.click();
    });
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
  await page.waitForTimeout(320);
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
  await page.waitForTimeout(1800);
  await snap('22-phonics-rainbow-done');
  // Force-click bypasses the frog's idle animation stability wait.
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
  await page.goto(`${url}#/phonics`);
  await page.waitForSelector('.phonics-letter');
  await snap('25-phonics-iphone-promax-landscape');

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
  await page.waitForTimeout(320);
  await snap('26-phonics-miss-promax');

  // Parent settings panel with mastery stats — long-press the in-game ←.
  await page.setViewportSize({ width: 844, height: 390 });
  const advanceVisible = await page.locator('.phonics-advance:not([hidden])').count();
  if (advanceVisible > 0) await page.click('.phonics-advance');
  await page.locator('.home-btn').click({ delay: 700 });
  await page.waitForSelector('.parent-settings-panel');
  await snap('27-parent-mastery');
}

// A small subset of scenes captured under real iOS device emulation
// (proper UA + devicePixelRatio + isMobile). The full scene walk runs
// at synthetic viewport sizes; these add the iOS-device-context layer
// where Safari quirks (e.g. 100dvh, safe-area, momentum scroll) tend to
// show up.
async function runIosDeviceScenes(page, snap) {
  await page.goto(url);
  await page.waitForSelector('.picker-card');
  await snap('00-picker');

  await page.click('.picker-card[data-game="patterns"]');
  await page.waitForSelector('.cell');
  await snap('01-patterns-initial');

  await page.goto(`${url}#/phonics`);
  await page.waitForSelector('.phonics-letter');
  await snap('20-phonics-card');

  // Drive to rainbow-done so we eyeball the celebration on the real
  // device aspect (the iPad bug the user hit was on this screen).
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
  await snap('22-phonics-rainbow-done');
}

function makeSnapper(subdir) {
  return async (page, name) => {
    const dir = join(shotsDir, subdir);
    await mkdir(dir, { recursive: true });
    const file = join(dir, `${name}.png`);
    await page.screenshot({ path: file, fullPage: false });
    console.log('saved', file);
  };
}

async function runEngine(label, launch) {
  console.log(`\n=== ${label} ===`);
  const browser = await launch();
  try {
    const ctx = await browser.newContext({
      // Default to landscape (the play orientation); a rotate-me overlay
      // hides the game in portrait on phone-sized screens.
      viewport: { width: 844, height: 390 },
      deviceScaleFactor: 1,
    });
    await applySeed(ctx);
    const page = await ctx.newPage();
    const snapTo = makeSnapper(label);
    await runScenes(page, (name) => snapTo(page, name));
    await ctx.close();
  } finally {
    await browser.close();
  }
}

// iOS device profiles to exercise on WebKit. Each profile maps to a
// Playwright device descriptor that sets UA / DPR / isMobile correctly.
const IOS_PROFILES = [
  { id: 'iphone-13-landscape', device: 'iPhone 13 landscape' },
  { id: 'iphone-15-pro-max-landscape', device: 'iPhone 15 Pro Max landscape' },
  { id: 'ipad-pro-11-portrait', device: 'iPad Pro 11' },
  { id: 'ipad-pro-11-landscape', device: 'iPad Pro 11 landscape' },
];

async function runIosDevices() {
  console.log('\n=== webkit-ios (device profiles) ===');
  const browser = await launchWebkit();
  try {
    for (const profile of IOS_PROFILES) {
      const descriptor = devices[profile.device];
      if (!descriptor) {
        console.warn(`  skip ${profile.id}: device "${profile.device}" not in Playwright devices`);
        continue;
      }
      const ctx = await browser.newContext({ ...descriptor });
      await applySeed(ctx);
      const page = await ctx.newPage();
      const snapTo = makeSnapper(join('webkit-ios', profile.id));
      try {
        await runIosDeviceScenes(page, (name) => snapTo(page, name));
      } catch (err) {
        // One broken profile shouldn't stop the rest; the saved
        // screenshots still capture useful evidence.
        console.error(`  ${profile.id} FAILED:`, err.message);
        process.exitCode = 1;
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await browser.close();
  }
}

const only = (process.env.ONLY ?? '').toLowerCase();
const runChromium = !only || only === 'chromium';
const runWebkit = !only || only === 'webkit';

if (runChromium) await runEngine('chromium', launchChromium);
if (runWebkit) {
  await runEngine('webkit', launchWebkit);
  await runIosDevices();
}

server.close();
console.log('done');
