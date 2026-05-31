# fountouki

- Preschool learning games as a PWA — a Rust + [macroquad](https://macroquad.rs)
  rewrite of the original TypeScript app.
- Renders **identically** on iOS / Android / desktop / web because it draws
  every pixel itself (one GL renderer, no DOM/CSS divergence). The old WebKit
  layout-quirk class of bug is gone by construction.

## Layout

- `core/` — `fountouki-core`: pure logic + unit tests. SRS, decks, themes,
  patterns generation, settings, storage/sync model, route ids, audio-fx
  params, rng. No rendering, no platform deps.
- `app/` — `fountouki` binary: macroquad rendering, scenes, input, layout,
  palette, fonts, sound, parent panel. Depends on `core`.
- `web/` — web shell: `index.html` + macroquad's `mq_js_bundle.js`. The built
  `fountouki.wasm` is copied in alongside for the Pages deploy.
- `server/` — Cloudflare Worker for cross-device sync. **Unchanged** from the
  TS app. See `server/README.md` for the live URL + deploy.
- `tools/goldens.sh` — golden-screenshot matrix (see below).
- `docs/port-spec/` — behavioral spec extracted from the old app (`shell`,
  `phonics`, `patterns`, `visual`, `audio-fx`, `storage-sync`). Source of
  truth for "what should this do" during the port.
- `ios/`, `android/` — native wrapper scaffolds + build docs (see their
  READMEs). On-device install is a manual step (TestFlight / sideloaded APK).

## Dev commands

```sh
cargo run -p fountouki                  # desktop interactive (native window)
cargo test --workspace                  # core logic tests + any app tests
cargo run -p fountouki -- --playtest    # scripted assertions; non-zero on fail
bash tools/goldens.sh                   # capture golden PNGs → screenshots/golden/

# WASM build (repo .cargo/config.toml supplies the wasm-ld flags):
cargo build --release -p fountouki --target wasm32-unknown-unknown
# then copy target/wasm32-unknown-unknown/release/fountouki.wasm → web/
```

- Native packaging / on-device builds: see `ios/README.md` and
  `android/README.md`.

## Binary modes

The `fountouki` binary has two non-interactive modes (otherwise it runs the
interactive app loop):

- `--capture <png> <scene> [w] [h]` — render one scene offscreen to a PNG.
  Same renderer that ships native, so the output reflects the device, not an
  emulator. `[w] [h]` default to a standard size if omitted.
- `--playtest` — drive the real scenes with synthetic taps and assert; exits
  non-zero on failure. Used in CI.

Scene ids (for `--capture` and `tools/goldens.sh`):
`picker`, `phonics`, `patterns`, `parent-patterns`, `parent-phonics`.

## Goldens

- `tools/goldens.sh` captures every scene at iPad landscape/portrait + phone
  landscape into `screenshots/golden/`. Eyeball them; CI uploads them as
  artifacts.
- Local (macOS): real GL. CI (Linux): runs under `xvfb-run` with software GL.
- **Determinism caveat:** the WASM/CI software-GL path is not pixel-identical
  to native GL (AA, gradients, text hinting differ). Goldens are for eyeballing
  regressions, not byte-exact diffing across the native↔CI boundary.

## Deploy

- Web → GitHub Pages (`.github/workflows/deploy.yml`): build wasm, assemble
  `web/`, publish.
- Sync server: `server/` (Cloudflare Worker) — deployed independently.
