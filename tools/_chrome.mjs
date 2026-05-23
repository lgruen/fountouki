// Shared Chromium launch helper.
//
// By default Playwright's `chromium.launch()` picks chromium-headless-shell,
// a minimal build without color-emoji fonts — so screenshots render emoji
// as boxes. The full Chromium build (installed by `npx playwright install
// chromium`) bundles the fonts. Prefer it for screenshots if available,
// otherwise fall back to default or a sandbox-env binary.

import { existsSync } from 'node:fs';
import { chromium } from 'playwright';

const SANDBOX_FALLBACK = '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

function findFullChromium() {
  const home = process.env.HOME ?? '';
  const candidates = [
    `${home}/Library/Caches/ms-playwright/chromium-1223/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`,
    `${home}/Library/Caches/ms-playwright/chromium-1223/chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`,
  ];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  return null;
}

export async function launchChromium(opts = {}) {
  const envPath = process.env.CHROME_PATH;
  const execPath =
    envPath ??
    findFullChromium() ??
    (existsSync(SANDBOX_FALLBACK) ? SANDBOX_FALLBACK : undefined);
  return chromium.launch({ ...opts, ...(execPath ? { executablePath: execPath } : {}) });
}
