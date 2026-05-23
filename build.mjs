// Tiny build script: bundles src/game.ts into dist/, copies static assets,
// and stamps the service worker with the file list + build id.
//
// Usage: node build.mjs          -> one-shot build
//        node build.mjs --watch  -> rebuild on change
//        node build.mjs --serve  -> also run a local dev server on :5173
import { build, context } from 'esbuild';
import { cp, mkdir, readdir, rm, writeFile, readFile as fsReadFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { extname, join, relative } from 'node:path';

const root = new URL('.', import.meta.url).pathname;
const srcDir = join(root, 'src');
const publicDir = join(root, 'public');
const outDir = join(root, 'dist');

const args = new Set(process.argv.slice(2));
const watch = args.has('--watch');
const serve = args.has('--serve');

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });
await cp(publicDir, outDir, { recursive: true });

const buildOptions = {
  entryPoints: [join(srcDir, 'main.ts')],
  bundle: true,
  format: 'esm',
  target: ['es2022'],
  sourcemap: true,
  minify: !watch,
  outfile: join(outDir, 'main.js'),
  logLevel: 'info',
  // Stamp the build id into the bundle so the registration code can
  // reference it for diagnostics if needed.
  define: { __BUILD_ID__: JSON.stringify(buildId()) },
};

function buildId() {
  // Compact ISO timestamp; monotonic across rebuilds, easy to eyeball.
  return new Date().toISOString().replace(/[-:.]/g, '').slice(0, 15);
}

if (watch) {
  const ctx = await context(buildOptions);
  await ctx.watch();
  // Re-copy static assets when html/css change.
  const { watch: fsWatch } = await import('node:fs');
  fsWatch(publicDir, { recursive: true }, async (_evt, file) => {
    if (!file) return;
    try {
      await cp(join(publicDir, file), join(outDir, file));
      await stampServiceWorker();
      // eslint-disable-next-line no-console
      console.log(`[copy] ${file}`);
    } catch {
      /* file might be gone */
    }
  });
} else {
  await build(buildOptions);
}

// Write a tiny .nojekyll so GitHub Pages serves files as-is.
await writeFile(join(outDir, '.nojekyll'), '');

await stampServiceWorker();

/**
 * Rewrite dist/sw.js, replacing __BUILD_ID__ with a fresh stamp and
 * __PRECACHE__ with a JSON array of every same-origin asset to cache.
 * The sw file itself is excluded from the precache list — the browser
 * never caches a SW via the cache API.
 */
async function stampServiceWorker() {
  const swPath = join(outDir, 'sw.js');
  let sw;
  try {
    sw = await fsReadFile(swPath, 'utf8');
  } catch {
    return; // no sw.js, skip silently
  }
  const id = buildId();
  const files = await listDist(outDir);
  const precache = files
    .filter((f) => f !== 'sw.js' && !f.endsWith('.map') && f !== '.nojekyll')
    .map((f) => `./${f}`);
  // Ensure the HTML root is reachable both as "./" and as "./index.html".
  if (precache.includes('./index.html') && !precache.includes('./')) precache.push('./');
  const stamped = sw
    .replaceAll('__BUILD_ID__', id)
    .replaceAll('__PRECACHE__', JSON.stringify(precache));
  await writeFile(swPath, stamped);
}

async function listDist(dir, prefix = '') {
  const out = [];
  for (const ent of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, ent.name);
    const rel = relative(outDir, full);
    if (ent.isDirectory()) {
      out.push(...(await listDist(full, rel + '/')));
    } else {
      out.push(rel);
    }
  }
  return out;
}

if (serve) {
  const port = 5173;
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
  createServer(async (req, res) => {
    try {
      let url = (req.url ?? '/').split('?')[0] ?? '/';
      if (url.endsWith('/')) url += 'index.html';
      const file = join(outDir, url);
      const data = await readFile(file);
      res.writeHead(200, {
        'Content-Type': mime[extname(file)] ?? 'application/octet-stream',
        // Don't cache during dev so reloads always reflect the latest build.
        'Cache-Control': 'no-store',
      });
      res.end(data);
    } catch {
      res.writeHead(404).end('not found');
    }
  }).listen(port, () => {
    // eslint-disable-next-line no-console
    console.log(`[serve] http://localhost:${port}/`);
  });
}

// Silence unused-import warning for stat in some environments.
void stat;
