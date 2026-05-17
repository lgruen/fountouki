// Shared Chromium launch helper.
//
// In the sandboxed dev environment, Playwright's own browser download is
// blocked, so we fall back to a pre-installed Chromium binary if it
// exists. In CI (and any other env where `npx playwright install
// chromium` has been run), we let Playwright pick its default.

import { existsSync } from 'node:fs';
import { chromium } from 'playwright';

const FALLBACK = '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

export async function launchChromium(opts = {}) {
  const envPath = process.env.CHROME_PATH;
  const execPath = envPath ?? (existsSync(FALLBACK) ? FALLBACK : undefined);
  return chromium.launch({ ...opts, ...(execPath ? { executablePath: execPath } : {}) });
}
