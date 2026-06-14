# Working agreements

## What this is
Preschool learning games (ages ~4) for the maintainer's kids. Rewritten from
a DOM/CSS PWA to **Rust + macroquad** so the app **draws every pixel itself** —
the rendering is identical on iOS, Android, desktop and web, because nothing is
delegated to a browser's CSS engine. That cross-platform consistency is the
whole reason the rewrite exists; don't reintroduce platform-delegated layout.

## Primary platform: WASM + PWA on iPad/Android
- Ships as a **WASM build in a PWA shell** (`web/`), deployed to GitHub Pages,
  installed to the home screen. This delivers the consistency win without app
  stores / signing / Xcode. Native iOS/Android (`ios/`, `android/`) are
  **optional, unmaintained scaffolds** — not the supported target.
- The same macroquad code renders natively too; the WASM build is canonical.

## Project shape
- Cargo workspace. **`core/`** = pure logic + data + protocol, NO macroquad
  (`fountouki-core`): srs, patterns, themes, deck, audio synth, settings, sync,
  storage, route, rng, tracing (stroke data baked by
  `tools/trace_extract/extract.py`). Fast to compile, unit-tested
  (`cargo test -p fountouki-core`).
- **`app/`** = the macroquad binary `fountouki`: rendering, scenes, input,
  audio playback, the engine. Depends on `core`.
  - `palette` `text` `draw` `anim` `input` `layout` `scene` `sound` `confetti`
    `store` `parent` `emoji`; `games/{picker,phonics,patterns,tracing}.rs`.
  - `layout.rs` computes every region from viewport size + safe-area insets +
    form factor — this is the consistency cure; keep layout ours.
  - Fonts (VicModernCursive) + Twemoji emoji sprites are `include_bytes!`-baked.
- **`web/`** = PWA shell (index.html + macroquad `mq_js_bundle.js` + `sw.js` +
  manifest/icons); the built `fountouki.wasm` is dropped in by CI.
- **`server/`** = Cloudflare Worker sync (unchanged). `docs/port-spec/` = the
  spec the rewrite was ported from. `tools/goldens.sh` = screenshot matrix.

## Binary modes (the test + visual harness)
- `cargo run -p fountouki` — interactive desktop.
- `--capture <png> <scene> [w] [h]` — render a scene offscreen to a PNG.
  Scenes: `picker phonics phonics-miss phonics-miss-igloo phonics-done patterns
  patterns-emoji patterns-unit patterns-hard patterns-levelup patterns-done
  tracing tracing-watch tracing-two-stroke tracing-reward tracing-build
  tracing-grade tracing-done tracing-housewarming parent-patterns
  parent-phonics parent-tracing`.
- `--playtest` — scripted taps drive the real scenes + assert invariants; exits
  non-zero on failure.

## Workflow
- Develop on a branch off `main`; open a PR at the end of a code-changing task.
  Never push to `main`.
- Before pushing: `cargo clippy --workspace --all-targets -- -D warnings` (also
  enforced by a PreToolUse hook in `.claude/settings.json` on every `git
  commit`/`git push`), `cargo test --workspace` (core unit tests), `cargo run -p
  fountouki -- --playtest` (gameplay), `bash tools/goldens.sh` (visuals), and
  `cargo build --release -p fountouki --target wasm32-unknown-unknown` (web).
- **Eyeball `screenshots/golden/` — iPad landscape first**, then portrait, then
  phone. The same renderer ships, so a golden reflects the device (no emulator
  gap), modulo GPU AA. (CI renders goldens under software GL via xvfb.)
- CI also diffs the fresh goldens against the committed baselines in
  `tests/golden-baseline/` (`tools/golden-diff.sh`). An intentional visual
  change must refresh the baselines in the same PR; use CI-rendered pixels
  (the `screenshots` artifact) if local GL output differs.
- Fresh sandbox: install Rust (`rustup`), `rustup target add
  wasm32-unknown-unknown`. The `.cargo/config.toml` adds the wasm linker flags
  macroquad needs (`--import-undefined`).

## CI / deploy
- `.github/workflows/ci.yml` — on PRs: `cargo test`, `--playtest`, goldens
  artifact, wasm build check (Linux deps + xvfb + software GL).
- `.github/workflows/deploy.yml` — on `main`: build wasm, assemble `web/` +
  the wasm into `dist/`, deploy to GitHub Pages.

## Working style
- **Self-verify before claiming done**: run `--playtest` + regenerate goldens +
  eyeball them. Add a `--playtest` scenario for new gameplay; extend
  `tools/goldens.sh` for new scenes.
- Delegate noisy/parallel work (spec extraction, golden review, independent
  code review) to subagents/workflows. Main thread owns the integrated build —
  a single Cargo crate doesn't parallelize authorship well (shared `target/`).
- **Independent review for non-trivial work**: after a meaningful slice, have an
  agent review code + visuals + pedagogy; iterate on visuals to the "would I
  play this?" bar.
- Tight docs: audience is a future Claude. Bullets over prose.

## The "would I play this?" test
Before claiming a game is done: if you were a 4yo with a short attention span,
would you push to come back? "Tolerates" fails; "wants more" is the bar. Big
animated rewards, vibrant color, characters that respond — over restrained/
minimal. The constraints below remove *noise around the target*, not the joy.

## Audience & pedagogy baseline
- Preschoolers; big tap targets, minimal text, visual-first navigation.
- **Co-played with a grown-up by design**: the adult guides and grades (phonics
  ✓/✗ is parent-mediated), so spoken letter-sound audio is *not* a gap — the
  parent's voice is the audio channel. Don't re-flag it.
- In-play animation stays calm near the stimulus; **reward moments** (on a
  correct answer / star / level-up) are where the joy goes.
- Errorless (never sit in "I don't know"), monotonic progress (stars/rainbow
  never decrement), no time pressure, theme as wrapper not clutter, ~5-min
  sessions.
- **Design for language delay + small working memory**: one stimulus at a time,
  no competing elements near the target; generous repetition + SRS; pictures
  *with* words (never picture-only or text-only); predictable layout across
  sessions; short direct prompts (no idioms); grading is parent-mediated.

## Brand
- 🌰 is the PWA launcher icon **only** — never in-app. In-app glyphs are neutral
  vectors (← chevron, speaker). The **frog is a drawn vector character** (not an
  emoji); the rainbow is the phonics progress meter.

## Parent menu (long-press ←)
- Long-press the in-game ← (500 ms) opens the parent settings overlay
  (`app/src/parent.rs`): universal sync token/endpoint + a session-only sync
  pause (never persisted; for testing without retyping the token) + a per-game
  section
  (patterns theme/difficulty/mode/hint cyclers + start-over; phonics + tracing
  read-only mastery grids; tracing start-over). No visible chrome / no topbar
  gear.

## Settings + storage + sync
- Mute is shared (one toggle). Per-game settings under
  `fountouki.<area>.<name>.v1` (JSON, via `core::storage`/`core::settings`).
- Scores/streak/level are session-only — never persisted.
- Sync: one family token; `core::sync` defines the protocol + merge
  (last-seen-wins). Transport is `app/src/net.rs` (poll-based `quad-net` HTTP):
  phonics + tracing pull+merge on mount, debounce-push on grade, flush on leave. The
  WASM build loads `web/sapp_jsutils.js` + `web/quad-net.js` before `load()`.

## No personal details in commits
- Repo is public; audience is the maintainer's kids. Keep kid names, ages,
  mastery state, and "for the user's son" framing out of committed files.
  Personal context lives in Claude's local memory, not the repo.

## PR descriptions
- For visual/UI work, attach representative `screenshots/golden/` images under
  `## Screenshots`. CI also uploads the full golden set as an artifact.
