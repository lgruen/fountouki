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
    const homeRect = home ? home.getBoundingClientRect() : null;
    const readArm = (which) => {
      if (!home) return null;
      const cs = window.getComputedStyle(home, which);
      return {
        content: cs.content,
        position: cs.position,
        width: parseFloat(cs.width),
        height: parseFloat(cs.height),
        topPx: parseFloat(cs.top),
        leftPx: parseFloat(cs.left),
        marginLeft: parseFloat(cs.marginLeft),
        marginTop: parseFloat(cs.marginTop),
        background: cs.backgroundColor,
        transform: cs.transform,
        transformOrigin: cs.transformOrigin,
      };
    };
    const muteSound = document.querySelector('.mute-btn .icon-sound');
    return {
      vp: { w: window.innerWidth, h: window.innerHeight },
      home: homeRect,
      homeColor: home ? window.getComputedStyle(home).color : null,
      homeChevronBefore: readArm('::before'),
      homeChevronAfter:  readArm('::after'),
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

  // Back chevron is drawn by .home-btn::before + ::after — two
  // absolutely-positioned bars meeting at a left-side tip. Round two
  // (border-left + border-bottom on a rotated square) rendered as a
  // single diagonal line — i.e. a slash — on the maintainer's iPad,
  // because iOS Safari dropped one of the two adjacent borders. Two
  // independent bars means there are no two-borders-sharing-a-corner
  // for iOS Safari to mishandle. Assert both arms render with a
  // non-zero filled bar shape anchored at the button centre.
  const arms = [
    ['::before', info.homeChevronBefore],
    ['::after',  info.homeChevronAfter ],
  ];
  for (const [which, c] of arms) {
    check(c !== null, `back-button ${which} pseudo-element present`);
    if (!c) continue;
    // `content: ""` computes to a non-`none`/non-`normal` value when
    // the pseudo-element is actually generated.
    check(c.content !== 'none' && c.content !== 'normal',
      `chevron ${which} rendered (content=${c.content})`);
    check(c.position === 'absolute',
      `chevron ${which} absolutely positioned (got ${c.position})`);
    // Bar is a thin horizontal rectangle (~12×3px on phones, ~14×3 on
    // tablets) — assert it's a clearly-non-square filled bar, not a
    // collapsed 0×0 box.
    check(c.width >= 10,  `chevron ${which} bar width >= 10px (got ${c.width})`);
    check(c.height >= 2,  `chevron ${which} bar height >= 2px (got ${c.height})`);
    check(c.width > c.height * 2,
      `chevron ${which} reads as a bar, not a square (${c.width}×${c.height})`);
    // Bar is filled with currentColor — must be opaque and not match
    // the page background (otherwise it'd be invisible on the button).
    const opaque = !/^rgba\(.*,\s*0\)$/.test(c.background) && c.background !== 'transparent';
    check(opaque, `chevron ${which} bar background opaque (${c.background})`);
    // The bar's anchor (top/left, with margin offsets) places the
    // ROTATION PIVOT at (centre − ~4px, centre). transform-origin is
    // `0 50%` of the bar — i.e. the bar's left-centre. Verify the
    // resolved pivot lands at the button's vertical midline (margin-top
    // = -height/2) and just left of horizontal midline (margin-left
    // negative).
    const pivotY = c.topPx + c.marginTop + c.height / 2;
    const pivotX = c.leftPx + c.marginLeft;
    const dPivotY = Math.abs(pivotY - info.home.height / 2);
    check(dPivotY < 1,
      `chevron ${which} pivot on button vertical midline (got ${pivotY}px, expected ~${info.home.height / 2}px)`);
    check(pivotX < info.home.width / 2 && pivotX > info.home.width / 2 - 8,
      `chevron ${which} pivot just left of button centre (got ${pivotX}px, button half ${info.home.width / 2}px)`);
    // Transform must include a rotation — without it the bar would
    // render as a flat horizontal stripe, not an arrow arm.
    check(c.transform !== 'none' && c.transform !== '',
      `chevron ${which} has rotation transform (got ${c.transform})`);
  }
  // Sanity: the two arms must have DIFFERENT transforms — one rotates
  // +45°, the other -45°. Identical transforms would collapse the two
  // bars into a single visible stripe (still not an arrow).
  if (info.homeChevronBefore && info.homeChevronAfter) {
    check(info.homeChevronBefore.transform !== info.homeChevronAfter.transform,
      `chevron arms have distinct transforms (before=${info.homeChevronBefore.transform}, after=${info.homeChevronAfter.transform})`);
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
