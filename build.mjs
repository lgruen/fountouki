// Tiny build script: bundles src/game.ts into dist/, copies static assets.
// Usage: node build.mjs          -> one-shot build
//        node build.mjs --watch  -> rebuild on change
//        node build.mjs --serve  -> also run a local dev server on :5173
import { build, context } from 'esbuild';
import { cp, mkdir, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { extname, join } from 'node:path';

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
  entryPoints: [join(srcDir, 'game.ts')],
  bundle: true,
  format: 'esm',
  target: ['es2022'],
  sourcemap: true,
  minify: !watch,
  outfile: join(outDir, 'game.js'),
  logLevel: 'info',
};

if (watch) {
  const ctx = await context(buildOptions);
  await ctx.watch();
  // Re-copy static assets when html/css change.
  const { watch: fsWatch } = await import('node:fs');
  fsWatch(publicDir, { recursive: true }, async (_evt, file) => {
    if (!file) return;
    try {
      await cp(join(publicDir, file), join(outDir, file));
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

if (serve) {
  const port = 5173;
  const mime = {
    '.html': 'text/html; charset=utf-8',
    '.js': 'application/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
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
