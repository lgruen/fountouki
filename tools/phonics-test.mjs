// Smoke for the phonics game: card render, got it / missed flow,
// session-end celebration, state persistence across reload.

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

console.log(`[phonics-test] browser=${BROWSER}`);
const browser = await launchBrowser();
const ctx = await browser.newContext({ viewport: { width: 844, height: 390 } });
const page = await ctx.newPage();
page.on('pageerror', (err) => {
  console.error('PAGE ERROR:', err.message);
  process.exitCode = 1;
});

console.log('1) load phonics; card visible');
await page.goto(`${url}#/phonics`);
await page.waitForSelector('.phonics-letter');
const initialLetter = await page.locator('.phonics-letter').textContent();
assert(initialLetter && /^[a-z]$/.test(initialLetter), `expected single a-z letter, got: ${initialLetter}`);
const dbg = await page.evaluate(() => window.__phonics);
assert.equal(dbg.stars, 0, 'fresh session starts at 0 stars');
assert.equal(Object.keys(dbg.state.letters).length, 26, 'all 26 letters initialized');

console.log('1b) fresh learner: only first few intro letters in rotation');
// Drip-in: a never-played learner should be limited to the first three
// letters of INTRO_ORDER (s, a, t). Walk through a few cards and make
// sure nothing outside that set appears.
const FIRST_INTRO = new Set(['s', 'a', 't']);
assert(FIRST_INTRO.has(initialLetter), `first card should be in intro set, got ${initialLetter}`);
const seen = new Set([initialLetter]);
for (let i = 0; i < 5; i++) {
  await page.click('.phonics-miss');
  await page.waitForSelector('.phonics-hint:not([hidden])');
  await page.click('.phonics-advance');
  await page.waitForFunction(() => window.__phonics?.inMissReveal === false);
  const cur = await page.evaluate(() => window.__phonics?.letter);
  assert(
    FIRST_INTRO.has(cur),
    `letter ${cur} appeared before any unlock; expected only s/a/t in fresh rotation`,
  );
  seen.add(cur);
}
// We should have cycled through all three (with REQUEUE_GAP they rotate).
assert.equal(seen.size, 3, `expected to see all 3 starter letters, saw: ${[...seen].join(',')}`);
// Reset fresh state for the rest of the test so subsequent steps don't
// inherit the misses we just inflicted.
await page.evaluate(() => localStorage.clear());
await page.reload();
await page.waitForSelector('.phonics-letter');

console.log('1c) legacy-polluted state: out-of-order introduced letters stay parked');
// Regression for the "kid just started but keeps getting x, v, q" bug.
// An older, ungated build flashed the whole alphabet, so a real install
// can carry box>=1 on letters scattered across the order — including the
// whole tail. activeLetters must still drip in Jolly order and NOT surface
// any letter past the frontier (the 3rd not-yet-introduced letter), even
// when there's no box-0 letter left after it to stop on.
const INTRO_ORDER = [
  's','a','t','i','p','n','c','k','e','h','r','m','d',
  'g','o','u','l','f','b','j','z','w','v','y','x','q',
];
// New (box 0) only for i, h, m — every other letter introduced to box 2.
// Mirrors the reported screenshot (0 mastered, 1 strong, 22 learning, 3 new).
const NEW_NOW = new Set(['i', 'h', 'm']);
await page.evaluate(
  ({ order, neu }) => {
    const now = Date.now();
    const letters = {};
    for (const l of order) {
      letters[l] = neu.includes(l)
        ? { box: 0, due: now, lastSeen: 0 }
        : { box: 2, due: now, lastSeen: now };
    }
    localStorage.setItem(
      'fountouki.phonics.state.v1',
      JSON.stringify({ schemaVersion: 1, letters, version: 50 }),
    );
  },
  { order: INTRO_ORDER, neu: [...NEW_NOW] },
);
await page.reload();
await page.waitForSelector('.phonics-letter');
// Frontier: walk INTRO_ORDER, stop after the 3rd box-0 letter (i, h, m).
const frontierIdx = INTRO_ORDER.indexOf('m');
const ALLOWED = new Set(INTRO_ORDER.slice(0, frontierIdx + 1));
assert(!ALLOWED.has('x') && !ALLOWED.has('v'), 'sanity: x/v are past the frontier');
// Also doubles as the no-back-to-back check: never the same letter twice
// in a row. (Miss-walk re-queues each letter, so this churns the carousel
// — the tightest case for an adjacent-dup bug.)
const polluted = new Set();
let prev = null;
for (let i = 0; i < 24; i++) {
  const cur = await page.evaluate(() => window.__phonics?.letter);
  assert(
    ALLOWED.has(cur),
    `parked letter ${cur} surfaced; rotation should stay within the Jolly frontier ${[...ALLOWED].join('')}`,
  );
  assert(cur !== prev, `same letter "${cur}" twice in a row at card ${i}`);
  prev = cur;
  polluted.add(cur);
  await page.click('.phonics-miss');
  await page.waitForSelector('.phonics-hint:not([hidden])');
  await page.click('.phonics-advance');
  await page.waitForFunction(() => window.__phonics?.inMissReveal === false);
}
assert(!polluted.has('x') && !polluted.has('v'), 'x/v must never appear (legacy pollution parked)');

console.log('1d) rotation is shuffled, not a fixed recency-ordered sequence');
// Each fresh mount rebuilds the queue from a shuffle. Re-mounting the same
// seeded state many times must NOT always start on the same letter — the
// old code returned a deterministic due-sorted order every time. With ~12
// due letters the odds of an 8-way tie by chance are negligible.
const firstCards = new Set();
for (let i = 0; i < 8; i++) {
  await page.goto('about:blank');
  await page.goto(`${url}#/phonics`);
  await page.waitForSelector('.phonics-letter');
  firstCards.add(await page.evaluate(() => window.__phonics?.letter));
}
assert(
  firstCards.size >= 2,
  `rotation looks deterministic — 8 mounts all opened on the same letter(s): ${[...firstCards].join('')}`,
);

// Reset to truly fresh state for the remaining steps.
await page.evaluate(() => localStorage.clear());
await page.reload();
await page.waitForSelector('.phonics-letter');

console.log('2) "got it" → star += 1, next card');
const beforeGotLetter = await page.locator('.phonics-letter').textContent();
await page.click('.phonics-got');
await page.waitForFunction(() => (window.__phonics?.stars ?? 0) === 1);
const afterGot = await page.evaluate(() => window.__phonics);
assert.equal(afterGot.stars, 1);
assert.notEqual(afterGot.letter, beforeGotLetter, 'next card should be a different letter');
// State for the got-it letter should now be at box 1.
assert.equal(afterGot.state.letters[beforeGotLetter].box, 1, `${beforeGotLetter} should be in box 1`);

console.log('3) "missed" → hint shows, letter fades, advance button replaces grade buttons');
const beforeMissLetter = afterGot.letter;
await page.click('.phonics-miss');
await page.waitForSelector('.phonics-hint:not([hidden])');
const hintEmoji = await page.locator('.phonics-hint-emoji').textContent();
assert(hintEmoji && hintEmoji.length > 0, 'hint emoji rendered');
const advanceVisible = await page.locator('.phonics-advance').isVisible();
const gotHidden = !(await page.locator('.phonics-got').isVisible());
assert(advanceVisible && gotHidden, 'advance shown, grade buttons hidden');
const afterMiss = await page.evaluate(() => window.__phonics);
assert.equal(afterMiss.stars, 1, 'miss does NOT add a star (monotonic)');
assert.equal(afterMiss.state.letters[beforeMissLetter].box, 0, `${beforeMissLetter} reset to box 0`);

console.log('4) advance → next card, no star added');
await page.click('.phonics-advance');
await page.waitForFunction(() => window.__phonics?.inMissReveal === false);
const afterAdvance = await page.evaluate(() => window.__phonics);
assert.equal(afterAdvance.stars, 1, 'advance does NOT add a star');

console.log('5) state persists across reload');
const snapshotBefore = await page.evaluate(() =>
  JSON.parse(localStorage.getItem('fountouki.phonics.state.v1')),
);
await page.reload();
await page.waitForSelector('.phonics-letter');
const snapshotAfter = await page.evaluate(() =>
  JSON.parse(localStorage.getItem('fountouki.phonics.state.v1')),
);
assert.deepEqual(snapshotAfter, snapshotBefore, 'phonics state persists');
const dbgAfter = await page.evaluate(() => window.__phonics);
assert.equal(dbgAfter.stars, 0, 'session stars reset on reload (session-only)');

console.log('6) drive to 7 stars → rainbow done overlay');
// Click "got it" repeatedly until we hit SESSION_GOAL (7).
for (let i = 0; i < 12; i++) {
  const dbg2 = await page.evaluate(() => window.__phonics);
  if (dbg2.stars >= 7) break;
  // If we're stuck in a miss-reveal somehow, advance first.
  const adv = await page.locator('.phonics-advance').isVisible();
  if (adv) await page.click('.phonics-advance');
  else await page.click('.phonics-got');
  await page.waitForFunction(() => window.__phonics?.inMissReveal === false);
  await page.waitForTimeout(800); // wait past the 700ms next-card delay
}
await page.waitForSelector('.phonics-done:not([hidden])', { timeout: 5000 });
const stars7 = await page.evaluate(() => window.__phonics?.stars);
assert.equal(stars7, 7, 'session done at 7 stars');

console.log('6b) rainbow-done scene: frog + garden + no mastery grid');
// Frog mascot is present and tappable.
const frogVisible = await page.locator('.phonics-frog').isVisible();
assert(frogVisible, 'frog mascot rendered in rainbow-done scene');
// ONE hero plant per session — the reward is "what grew this time?".
const plantCount = await page.locator('.phonics-plant').count();
assert.equal(plantCount, 1, `expected 1 hero plant, got ${plantCount}`);
// Mastery dots removed from the modal (parent settings still has them).
const masteryInModal = await page.locator('.phonics-done .mastery-dot').count();
assert.equal(masteryInModal, 0, 'mastery dots no longer rendered inside rainbow-done');
// Big done-scene rainbow renders 7 arcs.
const doneArcs = await page.locator('.phonics-done-arcs .phonics-arc-path').count();
assert.equal(doneArcs, 7, 'done-scene rainbow has 7 arcs');

console.log('6c) tap frog → reaction class added, counter increments');
const beforeFrogTaps = await page.evaluate(() => window.__phonics?.frogTaps ?? 0);
// Force-click bypasses Playwright's stability wait (the frog's perpetual
// idle animation otherwise blocks it).
await page.click('.phonics-frog', { force: true });
await page.waitForFunction(
  (before) => (window.__phonics?.frogTaps ?? 0) > before,
  beforeFrogTaps,
);
const afterFrogTap = await page.evaluate(() => window.__phonics?.frogTaps);
assert.equal(afterFrogTap, beforeFrogTaps + 1, 'frog tap counter incremented');
const reactionClass = await page.evaluate(() => {
  const f = document.querySelector('.phonics-frog');
  return [...(f?.classList ?? [])].some((c) => c.startsWith('react-'));
});
assert(reactionClass, 'frog tap added a react-* class');

console.log('7) play again → stars reset, card visible, frog taps reset');
await page.click('.phonics-done-again');
await page.waitForSelector('.phonics-done', { state: 'hidden' });
const afterAgain = await page.evaluate(() => window.__phonics);
assert.equal(afterAgain.stars, 0, 'play again resets stars');
assert.equal(afterAgain.frogTaps, 0, 'play again resets frog tap counter');
const letterAgain = await page.locator('.phonics-letter').textContent();
assert(letterAgain && /^[a-z]$/.test(letterAgain), 'card visible again');

await browser.close();
server.close();
console.log('\nOK — phonics card flow, miss-reveal, persistence, session-done, rainbow-done scene.');
