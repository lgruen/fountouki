# fountouki

Tiny learning games for preschoolers (ages ~4). Rewritten from a DOM/CSS PWA to
**Rust + macroquad**: the app **draws every pixel itself** onto one GPU surface,
so the UI is **identical on iOS, Android, desktop and web** — nothing is
delegated to a browser's CSS engine. That cross-platform consistency is the
reason the rewrite exists. Audience for this doc: a future Claude working here.

## Ships as
- **WASM in a PWA** (the `web/` shell), deployed to GitHub Pages, installed to
  the home screen. This is the canonical, supported deploy — no app store, no
  signing, no Xcode. The same macroquad code also runs native; `ios/` + `android/`
  hold **optional, unmaintained** native scaffolds.

## Layout
```
core/      fountouki-core — PURE logic/data/protocol, no macroquad. 120 unit tests.
app/       the macroquad binary `fountouki` — rendering, scenes, input, audio.
  assets/  fonts (VicModernCursive stimuli, Varela Round UI) + 110 Twemoji PNGs.
web/       PWA shell: index.html + macroquad mq_js_bundle.js + sw.js + manifest/icons.
server/    Cloudflare Worker sync (UNCHANGED from the TS app; see server/README.md).
tools/     goldens.sh — the screenshot matrix.
docs/      port-spec/ — the spec the rewrite was ported from (source of truth).
ios/ android/  optional native build scaffolds + READMEs.
```

### `core/` modules (pure, testable)
- `srs` — phonics Leitner SRS: boxes 0–4, intervals, INTRO_ORDER frontier gate,
  `merge` (last-seen-wins), validate/ensure_letters, `build_queue`.
- `patterns` — round generation: levels, period scaling, choice rules, the
  `mulberry32` RNG (consumption order matters for reproducibility).
- `themes` — the 9 theme item pools (`Item::Glyph`/`Item::Shape`), `ThemeChoice`.
- `deck` — phonics letters, INTRO_ORDER, per-letter exemplar (emoji + word).
- `audio` — PCM synthesis (correct/incorrect/level_up/tap/frog) → `Vec<f32>`.
- `settings` — `SharedSettings` (mute + sync) + `PatternsSettings`; token gen.
- `sync` — CF Worker protocol: `sync_url`, `interpret_pull`, `Debouncer`,
  `SyncTransport` trait. (See "Known follow-ups": the network transport isn't
  wired into the app yet.)
- `storage` — `KeyValueStore` trait + `ns_key` (`fountouki.<area>.<name>.v1`) +
  legacy migration. `route` — `parse_hash`/`hash_for`. `rng` — `Mulberry32`.
- `tracing` — letter-tracing stroke data + progress logic: per-letter pen
  centerlines baked from VicModernCursive by `tools/trace_extract/extract.py`
  (chart-accurate stroke order; macroquad can't read glyph outlines at
  runtime), corridor-follow `advance_progress`, teaching `ORDER`, persisted
  next-letter state.

### `app/` modules (rendering)
- Engine: `palette` `text` (cursive + UI font) `draw` (vector primitives,
  rainbow-arc geometry, frog, star, confetti shapes) `anim` `input` (pointer +
  500ms long-press) `layout` (**Frame** computes every region from viewport +
  safe-area + form factor — the consistency cure) `scene` (`Scene` trait + `Ctx`
  + `Nav`) `sound` (synth→WAV→macroquad) `confetti` `store` (`Db`) `emoji`
  (thread-local Twemoji sprite set).
- `parent.rs` — the long-press parent settings overlay.
- `games/{picker,phonics,patterns,tracing}.rs` — the scenes.
- `main.rs` — window, the router/app loop, `build_game`, and the `--capture` /
  `--playtest` entry points.

## Dev commands
```sh
cargo run -p fountouki                         # desktop (interactive)
cargo test --workspace                         # core unit tests (120)
cargo run -p fountouki -- --playtest           # scripted gameplay assertions
bash tools/goldens.sh                          # screenshot matrix → screenshots/golden/
cargo build --release -p fountouki --target wasm32-unknown-unknown   # web build
```
- Fresh machine: install `rustup`, then `rustup target add wasm32-unknown-unknown`.
  `.cargo/config.toml` already adds the wasm linker flags macroquad needs
  (`--import-undefined`/`--export-table`).
- `--capture <png> <scene> [w] [h]` renders a scene offscreen to a PNG. Scene
  ids: `picker phonics phonics-miss phonics-done patterns patterns-emoji
  patterns-unit tracing tracing-watch tracing-two-stroke tracing-done
  parent-patterns parent-phonics parent-tracing`.

## Testing & visual verification
- **Logic**: `cargo test --workspace` (core). **Gameplay**: `--playtest` drives
  the real scenes with synthetic taps + asserts (phonics 7-star completion,
  patterns correct-scores + errorless). **Visuals**: `tools/goldens.sh` renders
  every scene × {ipad-landscape, ipad-portrait, phone-landscape} to
  `screenshots/golden/` — the SAME renderer that ships produces these, so a
  golden reflects the device (modulo GPU AA). Eyeball iPad landscape first.
- CI (`.github/workflows/ci.yml`) runs all three on Linux under `xvfb` +
  software GL and uploads the goldens as an artifact. Determinism caveat: the
  software-GL path isn't byte-identical to native GL — goldens are for
  eyeballing regressions, not byte-exact cross-environment diffing.

## Deploy
- `.github/workflows/deploy.yml` (push to `main`): builds the wasm, assembles
  `web/` + `fountouki.wasm` into `dist/`, deploys to GitHub Pages.

## Extending
- **Add a game**: implement a `Scene` in `app/src/games/`, add an arm to
  `main::build_game` + an entry in `games::picker::GAMES` (id, label) + a
  `draw_icon` arm. Add a `--capture` scene id + a `--playtest` scenario.
- **Add an emoji**: drop its Twemoji PNG in `app/assets/emoji/` (lowercase hex
  codepoints, FE0F stripped) and add an `insert!` line in `app/src/emoji.rs`.
- Rendering rule: keep layout **ours** (compute from `Frame`); never reintroduce
  platform-delegated/CSS-style layout — that's the bug class the rewrite fixed.

## Cross-device sync
- Wired in `app/src/net.rs` (poll-based HTTP via `quad-net`): phonics **pulls +
  last-seen-wins merges on mount**, **debounce-pushes on each grade**, and
  **flushes on leaving**. Talks to the unchanged CF Worker at
  `<endpoint>/<token>/<game>` (one family token, set in the parent menu).
- The WASM build needs quad-net's JS plugins — `web/sapp_jsutils.js` +
  `web/quad-net.js`, loaded (in that order) before `load()` in `index.html`.
- **Caveat:** the in-browser HTTP round-trip can't be headless-tested here;
  verify on first deploy by setting the same token in two devices' parent menus.
  The protocol + merge are unit-tested in `core`; the app is offline-first and
  recovers from local storage regardless.

## Known follow-ups
- Native `ios/`/`android/` scaffolds are best-effort + unverified end-to-end
  (need Xcode + an Apple Developer account / Android NDK). WASM-PWA is the
  supported target.
- Visual polish backlog (from review): richer done-screen confetti, fuller
  picker frog, a friendlier patterns progress indicator.
