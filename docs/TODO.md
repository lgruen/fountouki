# TODO / future-Claude notes

A working memory dump for a fresh session. The repo is a tiny static
web app — see `README.md` for the basics.

## Repo orientation (60-second read)

- **Stack**: TypeScript → esbuild → `dist/`. No framework, no runtime
  dependencies. Strict tsconfig, single-file ESM bundle.
- **Source layout**:
  - `src/game.ts`     — UI wiring, state machine, settings, debug hook
  - `src/patterns.ts` — pure pattern generator (`generateRound`,
    `buildChoices`); no DOM. The core pedagogy lives here.
  - `src/themes.ts`   — item palettes (emoji / shapes / letters / numbers)
  - `src/render.ts`   — DOM helpers for cells and choice buttons
  - `src/sounds.ts`   — Web Audio chimes (no asset files)
  - `src/confetti.ts` — tiny canvas particle system
- **Static**: `public/index.html`, `public/style.css` get copied to `dist/`
  verbatim. CSS is global (no CSS modules). Mobile-first; cells use
  `flex: 1 1 0; max-width: 56px; aspect-ratio: 1 / 1;` so a single row
  always fits.
- **Build/dev**: `npm run dev` runs esbuild watch + a local server at
  `http://localhost:5173`. `npm run check` does typecheck + build.
- **Visual feedback loop** (no playtester needed):
  - `npm run screenshots` renders `screenshots/*.png` at iPhone-14 size
  - `node tools/playtest.mjs [N]` autoplays N rounds with deterministic
    RNG, picking the correct answer each time, then prints a per-level
    summary of templates / themes / choice counts / visible lengths
- **Debug hook**: `window.__patternplay` exposes the current round
  (template, answer id, visible item ids, level, stars, streak, mode).
  Used by `tools/playtest.mjs`; kept in production because it's harmless
  and useful for adhoc tinkering.
- **Sandboxed-env quirk**: Playwright's own browser download is blocked
  here, so `tools/screenshots.mjs` and `tools/playtest.mjs` point at
  `/opt/pw-browsers/chromium-1194/chrome-linux/chrome`. Override with
  `CHROME_PATH=...` or remove the `executablePath` line in normal envs.
- **Deploy**: pushes to `main` go to GitHub Pages via
  `.github/workflows/deploy.yml`. CI on other branches runs typecheck +
  build (`.github/workflows/ci.yml`).
- **Settings persistence**: `localStorage` key `patternplay.settings.v1`
  stores theme / difficulty / mode / hint / mute. Scores are session-only
  by design — never persist them.

## Pedagogy quick recap

- Templates are placeholder strings: `AB`, `AAB`, `ABC`, `ABBC`, `ABCD`.
  They're filled with distinct items from the active theme.
- Visible sequence = `fullReps × period + partialLen` cells, capped so it
  fits one row. Period-2 patterns use 3 reps; period-3/4 use 2.
- "Easy" answer mode = choices drawn from the items in the sequence; the
  choice count grows naturally with the template's distinct-item count.
  "Hard" = 4 choices including distractors from the wider theme pool.
- Level progression: 4 consecutive correct = level up, capped at L6. In
  `auto` difficulty, easy mode runs through L4, hard from L5.
- Implicit unit highlighting: alternating warm/cool background per period
  so the eye groups cells into "the piece that repeats" without needing
  explicit instruction.

## Not yet implemented

### Content / pedagogy

- **Phonics-based sequences** (explicitly requested for "later"). Two
  shapes this could take:
  1. Spoken letter sounds via `SpeechSynthesisUtterance` — patterns made
     of phonemes (`/k/ /æ/ /t/`) where tapping a cell speaks it.
  2. CVC-word patterns (cat-bat-cat-bat) with optional speech.
  Skeleton: a new theme `phonics` plus a `playSound(item)` hook in
  `render.ts` that calls `speechSynthesis.speak()` on tap.
- **Voice / spoken feedback** generally (read the prompt, name the
  correct item on a correct answer). Helps pre-readers.
- **More pattern types**: growing patterns (1, 22, 333), AB-with-mirror
  (ABBA), symmetry tasks. Currently only periodic repetition.
- **Unit-mode UX needs work**: at L1 only 4 cells are visible (post-L1
  it's 6+), and "tap first → tap last" is conceptually heavy for a 4yo.
  Consider:
  - Unlock unit-mode only at L3+ where ≥6 cells are visible.
  - Switch interaction to "tap each item of the piece in order, then a
    'Done' button" — feels more like building a unit.
  - Visually divide cells into groups *after* a correct guess to confirm
    the insight.
- **L4 leans on 2-distinct templates (AAB, ABB)**, so the choice count
  doesn't actually grow vs L3 in many rounds. If we want a smoother bump
  before L5's full-palette switch, make the L4 tier `['ABC', 'ABBC']` or
  similar.

### Themes / accessibility

- **More themes**: clothes, weather, body parts. Add them in
  `src/themes.ts`; the picker is data-driven.
- **Phonics theme** (see above).
- **Color-blind safety for shapes**: currently relies on hue alone.
  Could differentiate by shape *and* color (already mostly true), but
  add patterns/icons inside shapes for true CVD-safe play.
- **Screen reader**: cells have `aria-label`, but the live region
  feedback could be richer ("Yes, the next picture is panda").
- **Keyboard play**: choice buttons are focusable, but there's no
  tab-through-cells flow in unit-mode.

### Features

- **Streak indicator** on the HUD (currently only the per-session star
  count is visible).
- **Parent-controlled starting level** (e.g. a `?level=3` URL param or a
  settings field) — useful when a child has progressed in past sessions.
- **Auto-advance speed slider** — currently 1100ms between rounds.
- **"Hide score" mode** for parents who want zero competitive pressure.
- **PWA — offline support**: manifest + iOS meta tags + icons are
  already wired (`public/manifest.webmanifest`, `public/icon.svg`,
  `tools/icons.mjs`); "Add to Home Screen" launches in standalone
  mode. Still missing: a service worker that caches `dist/` so it
  plays without network after first load.
- **Print mode**: render a PDF of patterns for paper practice
  (useful on trips without a phone).
- **Sharable seeds**: encode the round/level into the URL so a parent
  can show "this exact puzzle".

### Tech / infra

- **Unit tests for `patterns.ts`** with `node:test` — pure functions, no
  DOM needed. Cover: distinct-count, every template at every level,
  buildChoices always includes the answer, easy mode never draws outside
  the sequence.
- **End-to-end Playwright tests** beyond screenshots: assert level
  transitions, that wrong answers don't increment stars, etc. The
  `tools/playtest.mjs` script is the seed for this.
- **Drop the sandbox-specific `executablePath`** once we're outside this
  environment — let Playwright manage its own browser via `npx
  playwright install chromium`.
- **CSS organization**: single file is fine at this size; if it grows,
  consider co-locating styles with TS via a small `cssTemplate` helper
  rather than introducing a build-time CSS pipeline.
- **`tools/screenshots.mjs` has a stale "level-3 visual check" block**
  that no longer manipulates state directly (commented as a TODO in the
  file). Easier alternative: expose a `setLevel` debug hook.
- **README link to a live demo**: once GitHub Pages is enabled on the
  repo, paste the URL in the README header.
- **CodeQL / Dependabot**: tiny dep tree so probably not worth it, but
  free if added.

## Known minor visual things

- Tablet layout has the app capped at 560px so cells don't get tiny
  islands of whitespace — fine, but means tablets don't fully use the
  screen. Acceptable for a one-handed game.
- Confetti emits from ~55% screen height upward; on very short viewports
  it might fall mostly off-screen. Not a practical issue on phones.
- The speaker icon (🔊) renders inconsistently across platforms; some
  systems show a stylized loudspeaker that looks busy. A custom SVG
  would be cleaner but adds bytes.
