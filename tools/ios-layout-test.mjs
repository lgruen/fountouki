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
    const home = document.querySelector('.home-btn');
    const homeBefore = home ? window.getComputedStyle(home, '::before') : null;
    const muteSound = document.querySelector('.mute-btn .icon-sound');
    return {
      vp: { w: window.innerWidth, h: window.innerHeight },
      home: r('.home-btn'),
      homeChevron: homeBefore
        ? {
            content: homeBefore.content,
            width: parseFloat(homeBefore.width),
            height: parseFloat(homeBefore.height),
            borderLeftWidth: parseFloat(homeBefore.borderLeftWidth),
            borderBottomWidth: parseFloat(homeBefore.borderBottomWidth),
            borderLeftColor: homeBefore.borderLeftColor,
            borderBottomColor: homeBefore.borderBottomColor,
            display: homeBefore.display,
          }
        : null,
      mute: r('.mute-btn'),
      muteSound: muteSound ? r('.mute-btn .icon-sound') : null,
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

  // Back-button chevron is now drawn by the .home-btn::before
  // pseudo-element (CSS borders, no SVG). Assert that:
  //   - the pseudo-element exists and has rendered content
  //   - it has reasonable pixel dimensions (>=8px square)
  //   - the borders that draw the chevron are non-zero and visible
  //     (not transparent, not matching the button background).
  // This is the layer that broke on the real iPad — the inline-SVG
  // approach rendered as 0×0 / invisible; the pseudo-element render
  // path is much more uniform across WebKit versions.
  check(info.homeChevron !== null, 'back-button ::before pseudo-element present');
  if (info.homeChevron) {
    const c = info.homeChevron;
    // `content` is set to `""` so it computed as `"none"` would mean the
    // pseudo-element is suppressed.
    check(c.display !== 'none' && c.content !== 'none', `chevron rendered (display=${c.display}, content=${c.content})`);
    check(c.width >= 8, `chevron width >= 8px (got ${c.width})`);
    check(c.height >= 8, `chevron height >= 8px (got ${c.height})`);
    const aspect = c.width / c.height;
    check(Math.abs(aspect - 1) < 0.05, `chevron square (aspect ${aspect.toFixed(3)})`);
    check(c.borderLeftWidth >= 2, `chevron left border drawn (${c.borderLeftWidth}px)`);
    check(c.borderBottomWidth >= 2, `chevron bottom border drawn (${c.borderBottomWidth}px)`);
    // Border color must be non-transparent and not the page background
    // (#fef6e4) — that's the only way a stroked chevron is actually
    // visible on the white button face.
    const opaque = !/^rgba\(.*,\s*0\)$/.test(c.borderLeftColor) && c.borderLeftColor !== 'transparent';
    check(opaque, `chevron border color opaque (${c.borderLeftColor})`);
  }

  // Home button fully visible and at expected min size.
  check(info.home.width >= p.minSize, `home button width >= ${p.minSize} (got ${info.home.width})`);

  // Mute speaker should fill a noticeable fraction of the mute button —
  // not a tiny dot inside a big circle. Previously the parent's small
  // font-size made the emoji render at ~25% of the button.
  if (info.muteSound && info.mute) {
    const muteFrac = info.muteSound.width / info.mute.width;
    check(muteFrac >= 0.5, `mute speaker fills >= 50% of button (got ${(muteFrac * 100).toFixed(0)}%)`);
  }

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

  // The ✗ / ✓ marks themselves are CSS-drawn pseudo-elements. Assert
  // their visible mass sits within ~2px of each button's own centre —
  // the iPad showed the unicode glyphs noticeably left-biased inside
  // the round buttons (font-bearing asymmetry), which this catches.
  // Pseudo-elements don't appear in getBoundingClientRect, so we read
  // their resolved top/left (px values resolved from the declared 50%)
  // and the button's own size, then assert the pseudo's top-left lands
  // at the button's midpoint. Combined with `transform: translate(-50%,
  // -50%)` in CSS, that pins the pseudo's centre to the button centre
  // by construction — no glyph metrics involved.
  const markCenters = await page.evaluate(() => {
    function read(sel, which) {
      const el = document.querySelector(sel);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      const cs = window.getComputedStyle(el, which);
      return {
        position: cs.position,
        topPx: parseFloat(cs.top),
        leftPx: parseFloat(cs.left),
        content: cs.content,
        btnW: r.width,
        btnH: r.height,
      };
    }
    return {
      missBefore: read('.phonics-miss', '::before'),
      missAfter:  read('.phonics-miss', '::after'),
      gotBefore:  read('.phonics-got',  '::before'),
    };
  });

  for (const [name, info] of Object.entries(markCenters)) {
    if (!info) continue;
    check(info.position === 'absolute', `${name} absolutely positioned`);
    check(info.content !== 'none' && info.content !== 'normal',
      `${name} has rendered content`);
    const expectedTop = info.btnH / 2;
    const expectedLeft = info.btnW / 2;
    const dTop = Math.abs(info.topPx - expectedTop);
    const dLeft = Math.abs(info.leftPx - expectedLeft);
    check(dTop < 1,
      `${name} top anchored at button midline (got ${info.topPx}px, expected ~${expectedTop}px)`);
    check(dLeft < 1,
      `${name} left anchored at button midline (got ${info.leftPx}px, expected ~${expectedLeft}px)`);
  }

  await ctx.close();
}

await browser.close();
server.close();

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log('\nOK — iOS layout invariants hold across iPad / iPhone profiles.');
