// Shared browser launcher.
//
// The harness runs each Playwright tool under one of two engines, selected
// by the BROWSER env var:
//   - chromium (default) — stands in for desktop / Android Chrome
//   - webkit              — stands in for iOS Safari (and macOS Safari)
//
// iOS-only layout regressions historically slipped through because every
// tool ran on Chromium. Routing tests + screenshots through both engines
// in CI catches WebKit-specific quirks (100dvh, safe-area, flexbox
// rounding) before they ship.
//
// `launchChromium` is kept for the icons tool, which needs the full
// Chromium build for color-emoji fonts and shouldn't be re-routed.

import { existsSync } from 'node:fs';
import { chromium, webkit, devices } from 'playwright';

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

export const BROWSER = (process.env.BROWSER ?? 'chromium').toLowerCase();

if (BROWSER !== 'chromium' && BROWSER !== 'webkit') {
  throw new Error(`Unsupported BROWSER=${BROWSER}; expected 'chromium' or 'webkit'.`);
}

export async function launchChromium(opts = {}) {
  const envPath = process.env.CHROME_PATH;
  const execPath =
    envPath ??
    findFullChromium() ??
    (existsSync(SANDBOX_FALLBACK) ? SANDBOX_FALLBACK : undefined);
  return chromium.launch({ ...opts, ...(execPath ? { executablePath: execPath } : {}) });
}

export async function launchWebkit(opts = {}) {
  return webkit.launch(opts);
}

// Honours BROWSER env var. Tests should use this so the same suite runs
// under both engines in CI.
export async function launchBrowser(opts = {}) {
  return BROWSER === 'webkit' ? launchWebkit(opts) : launchChromium(opts);
}

export { chromium, webkit, devices };
