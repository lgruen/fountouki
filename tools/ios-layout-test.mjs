// iOS layout regression test.
//
// Two bugs slipped through to a real iPad PWA before we caught them:
//   - The back-button SVG (chevron) sized via `width: 55%` rendered tiny
//     and non-square in WebKit's flex-item sizing, then disappeared
//     entirely on some iOS versions — back button was an empty circle.
//   - The X / ✓ row used plain flex, so the smaller miss and bigger got
//     buttons were laid out flush-left and flush-right of the row. The
//     row was geometrically centered but the button CENTERS were
//     ~10px off the card axis — visibly skewed.
//
// Neither was visible in chromium screenshots, neither caused a test
// failure. This file is the second line of defence: under WebKit-with-
// real-iOS-device-profile, assert the layout invariants we care about.
//
// Runs under WebKit only — there's nothing to gain from re-running it on
// chromium. `npm test:webkit` includes it; the chromium-only flow skips.

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import assert from 'node:assert/strict';
import { webkit, devices } from 'playwright';

const BROWSER = (process.env.BROWSER ?? 'chromium').toLowerCase();
if (BROWSER !== 'webkit') {
  console.log(`[ios-layout-test] BROWSER=${BROWSER}; iOS layout test only meaningful under WebKit. Skipping.`);
  process.exit(0);
}

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

const PROFILES = [
  { id: 'iPad Pro 11 landscape', minSize: 56 },     // tablet-landscape
  { id: 'iPad Pro 11',           minSize: 44 },     // tablet-portrait
  { id: 'iPhone 13 landscape',   minSize: 44 },     // phone-landscape
  { id: 'iPhone 15 Pro Max landscape', minSize: 44 },
];

const browser = await webkit.launch();
let failures = 0;
for (const p of PROFILES) {
  const desc = devices[p.id];
  if (!desc) {
    console.log(`  skip ${p.id}: no device descriptor`);
    continue;
  }
  console.log(`\n[${p.id}]`);
  const ctx = await browser.newContext({ ...desc });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => {
    console.error(`  PAGE ERROR: ${e.message}`);
    failures += 1;
  });
  await page.goto(`${url}#/phonics`);
  await page.waitForSelector('.phonics-letter');
  await page.waitForTimeout(120);

  const info = await page.evaluate(() => {
    const r = (sel) => {
      const el = document.querySelector(sel);
      return el ? el.getBoundingClientRect() : null;
    };
    return {
      vp: { w: window.innerWidth, h: window.innerHeight },
      home: r('.home-btn'),
      svg: r('.home-btn > svg'),
      mute: r('.mute-btn'),
      card: r('.phonics-card'),
      actions: r('.phonics-actions'),
      miss: r('.phonics-miss'),
      got: r('.phonics-got'),
    };
  });

  const check = (cond, msg) => {
    if (cond) {
      console.log(`  ok: ${msg}`);
    } else {
      console.error(`  FAIL: ${msg}`);
      failures += 1;
    }
  };

  // SVG renders, square, reasonable size.
  check(info.svg !== null, 'back-button SVG present');
  if (info.svg) {
    check(info.svg.width >= 14, `SVG width >=14 (got ${info.svg.width})`);
    check(info.svg.height >= 14, `SVG height >=14 (got ${info.svg.height})`);
    // Aspect ratio square within rounding.
    const ratio = info.svg.width / info.svg.height;
    check(Math.abs(ratio - 1) < 0.1, `SVG aspect ~ 1:1 (got ${ratio.toFixed(3)})`);
    // SVG should fit inside the home button (not overflow).
    check(info.svg.width <= info.home.width, `SVG width <= home button width`);
  }

  // Home button fully visible and at expected min size.
  check(info.home.width >= p.minSize, `home button width >= ${p.minSize} (got ${info.home.width})`);

  // X / ✓ pair: midpoint of button centers must match the card's center
  // within a tight tolerance. The earlier flex layout missed this by
  // ~10px on iPad because the smaller miss button sat flush-left of the
  // row instead of centered in an equal-width slot.
  const cardCenter = info.card.x + info.card.width / 2;
  const missCenter = info.miss.x + info.miss.width / 2;
  const gotCenter = info.got.x + info.got.width / 2;
  const pairCenter = (missCenter + gotCenter) / 2;
  const off = Math.abs(pairCenter - cardCenter);
  check(off < 2, `X/✓ pair centered under card (off by ${off.toFixed(2)}px)`);

  // miss must be left of got (visual order matters for muscle memory).
  check(missCenter < gotCenter, `miss left of got`);

  await ctx.close();
}

await browser.close();
server.close();

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log('\nOK — iOS layout invariants hold across iPad / iPhone profiles.');
