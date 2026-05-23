// Render the SVG icon to the PNGs needed for PWA / iOS home-screen.
// Outputs into public/ so the regular build copies them to dist/.
//
// Usage: node tools/icons.mjs

import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { launchChromium } from './_chrome.mjs';

const root = new URL('..', import.meta.url).pathname;
const publicDir = join(root, 'public');
const svg = await readFile(join(publicDir, 'icon.svg'), 'utf8');

// The "any" icon fills the viewport; the "maskable" icon is the same art
// scaled to 70% so it survives Android's circular / rounded-square crops.
function pageHtml(svgText, size, maskable) {
  const inner = maskable
    ? `<div style="width:${size}px;height:${size}px;background:#fef6e4;display:flex;align-items:center;justify-content:center"><div style="width:${Math.round(size * 0.7)}px;height:${Math.round(size * 0.7)}px">${svgText}</div></div>`
    : `<div style="width:${size}px;height:${size}px">${svgText}</div>`;
  return `<!doctype html><html><body style="margin:0;padding:0;background:transparent">${inner}</body></html>`;
}

const targets = [
  { name: 'icon-180.png', size: 180, maskable: false },
  { name: 'icon-192.png', size: 192, maskable: false },
  { name: 'icon-512.png', size: 512, maskable: false },
  { name: 'icon-maskable-512.png', size: 512, maskable: true },
];

// Use the full Chromium (color-emoji fonts) via the shared launcher —
// the hazelnut icon is rendered as a 🌰 text element in the SVG.
const browser = await launchChromium();
try {
  const ctx = await browser.newContext({ deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  for (const t of targets) {
    await page.setViewportSize({ width: t.size, height: t.size });
    await page.setContent(pageHtml(svg, t.size, t.maskable), { waitUntil: 'load' });
    const png = await page.screenshot({ omitBackground: false, type: 'png' });
    const out = join(publicDir, t.name);
    await writeFile(out, png);
    console.log('wrote', out, `(${t.size}x${t.size})`);
  }
} finally {
  await browser.close();
}
