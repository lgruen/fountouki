// Automated play-test: run N rounds, always picking the correct answer,
// and log the difficulty curve (level transitions, templates seen,
// answer-mode used).
//
// Usage: node tools/playtest.mjs [rounds]

import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { chromium } from 'playwright';

const root = new URL('..', import.meta.url).pathname;
const dist = join(root, 'dist');
const shotsDir = join(root, 'screenshots', 'playtest');
await mkdir(shotsDir, { recursive: true });
const ROUNDS = Number(process.argv[2] ?? 40);

// Build first.
await new Promise((resolve, reject) => {
  const child = spawn('node', ['build.mjs'], { cwd: root, stdio: 'inherit' });
  child.on('exit', (c) => (c === 0 ? resolve() : reject(new Error(`build failed: ${c}`))));
});

// Serve dist.
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

const execPath = process.env.CHROME_PATH ?? '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';
const browser = await chromium.launch({ executablePath: execPath });
const ctx = await browser.newContext({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
});
// Deterministic PRNG so runs are repeatable.
await ctx.addInitScript(() => {
  let seed = 0xC0FFEE;
  Math.random = () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
});
const page = await ctx.newPage();
page.on('pageerror', (err) => console.error('PAGE ERROR:', err.message));

await page.goto(url);
await page.waitForFunction(() => Boolean(window.__patternplay?.answerId));

const rows = []; // { round, level, stars, template, themeId, answerId, choiceCount }
let lastLevel = 0;
const levelChanges = []; // { atRound, fromLevel, toLevel }
const screenshotsTaken = new Set();

for (let r = 1; r <= ROUNDS; r++) {
  const snap = await page.evaluate(() => window.__patternplay);
  if (!snap?.answerId) {
    console.error(`round ${r}: no answer exposed`);
    break;
  }
  // Count rendered choices.
  const choiceCount = await page.locator('.choice').count();
  rows.push({
    round: r,
    level: snap.level,
    stars: snap.stars,
    streak: snap.streak,
    template: snap.template,
    themeId: snap.themeId,
    answerId: snap.answerId,
    visibleLen: snap.visibleIds.length,
    choiceCount,
  });

  if (snap.level !== lastLevel) {
    levelChanges.push({ atRound: r, fromLevel: lastLevel, toLevel: snap.level });
    if (!screenshotsTaken.has(snap.level)) {
      const file = join(shotsDir, `level-${snap.level}-round-${r}.png`);
      await page.screenshot({ path: file });
      screenshotsTaken.add(snap.level);
    }
    lastLevel = snap.level;
  }

  // Click the choice with the matching data-id.
  await page.locator(`.choice[data-id="${snap.answerId}"]`).click();
  // Wait for the next round to render (the round object's answer changes).
  const prevAnswer = snap.answerId;
  const prevRound = r;
  try {
    await page.waitForFunction(
      ([prev, _round]) => {
        const s = window.__patternplay;
        return Boolean(s && s.answerId && s.answerId !== prev);
      },
      [prevAnswer, prevRound],
      { timeout: 4000 },
    );
  } catch {
    // The level-up path adds an extra delay; just try once more.
    await page.waitForTimeout(800);
  }
}

await browser.close();
server.close();

// Summarize.
const byLevel = new Map();
for (const row of rows) {
  if (!byLevel.has(row.level)) byLevel.set(row.level, []);
  byLevel.get(row.level).push(row);
}

const lines = [];
lines.push(`PLAYTEST: ${ROUNDS} rounds, seed=0xC0FFEE`);
lines.push('');
lines.push('Level transitions:');
for (const c of levelChanges) {
  lines.push(`  round ${c.atRound}: L${c.fromLevel} -> L${c.toLevel}`);
}
lines.push('');
lines.push('Per-level summary:');
for (const [level, lvlRows] of [...byLevel.entries()].sort((a, b) => a[0] - b[0])) {
  const templates = new Map();
  const themes = new Map();
  const choiceCounts = new Map();
  const visibleLens = new Map();
  for (const row of lvlRows) {
    templates.set(row.template, (templates.get(row.template) ?? 0) + 1);
    themes.set(row.themeId, (themes.get(row.themeId) ?? 0) + 1);
    choiceCounts.set(row.choiceCount, (choiceCounts.get(row.choiceCount) ?? 0) + 1);
    visibleLens.set(row.visibleLen, (visibleLens.get(row.visibleLen) ?? 0) + 1);
  }
  const tmpl = [...templates.entries()].map(([t, n]) => `${t}×${n}`).join(' ');
  const thm = [...themes.entries()].map(([t, n]) => `${t}×${n}`).join(' ');
  const ch = [...choiceCounts.entries()].map(([c, n]) => `${c}ch×${n}`).join(' ');
  const vl = [...visibleLens.entries()].map(([v, n]) => `${v}len×${n}`).join(' ');
  lines.push(`  L${level}: ${lvlRows.length} rounds`);
  lines.push(`    templates : ${tmpl}`);
  lines.push(`    themes    : ${thm}`);
  lines.push(`    choices   : ${ch}`);
  lines.push(`    visible   : ${vl}`);
}
console.log(lines.join('\n'));
