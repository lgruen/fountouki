# Port spec: app shell, navigation, settings

Source of truth for the Rust reimplementation of fountouki's shell.
Behaviour transcribed from the TS app (`src/main.ts`, `src/router.ts`,
`src/picker.ts`, `src/shared/*`, `public/manifest.webmanifest`,
`public/index.html`, the two per-game settings sections). All numeric
constants and string literals below are load-bearing — copy them, don't
re-derive.

Audience: future Claude porting to Rust. Platform that matters is iPad
iOS Safari PWA; the same shell ships native via iOS/Android wrappers, so
keep platform glue (orientation lock, standalone detection, SW/update
check) behind a thin trait the host implements.

---

## 1. Navigation / routing

### Route model
Two routes only:
- `Picker` — the home grid.
- `Game { id: String }` — a single mounted game.

### Hash grammar (web parity)
- `parse_hash(hash)`: regex `^#/([a-z0-9-]+)` (case-insensitive). On
  match → `Game { id: m[1].to_lowercase() }`. Anything else (including
  bare `#/`, empty, `#`, garbage) → `Picker`. **Unknown-route fallback
  is "go to picker", silently.**
- `hash_for(route)`: `Picker → "#/"`, `Game{id} → "#/{id}"`.
- `navigate(route)`: if `location.hash` already equals the target,
  re-dispatch a synthetic `hashchange` (forces a re-render of the same
  route, e.g. tapping the same card / "start over"); else assign the
  hash and let the native hashchange fire.

In native ports there is no URL bar; model the route as in-memory state
+ a back stack of depth 1 (picker is the root, a game is the one child).
Keep `parse_hash`/`hash_for` as pure functions even off-web — golden
tests assert them and deep-link/state-restore can reuse them.

### Mount / unmount lifecycle
Single live "screen" at a time, owned by the router:
```
render():
  if current unmount fn exists: call it; clear it
  route = parse_hash(location.hash)
  if Picker:
    unmount = mount_picker(app, GAMES, on_pick = |id| navigate(Game{id}))
    return
  game = GAMES.find(id == route.id)
  if not found: navigate(Picker); return           # unknown-route fallback
  unmount = game.mount(app, MountOpts{ on_home: || navigate(Picker) })
```
- Every mount returns an `unmount` closure (`-> ()`). The picker's
  unmount is `container.innerHTML = ''`; games tear down their own
  listeners/timers and clear the container.
- **What is torn down on navigation:** the entire DOM subtree of `#app`,
  plus every event listener / timer / animation the screen registered.
  Games hold no module-level singletons that survive unmount except
  persisted storage and the sync singleton (below). Score/streak/level
  live only in the mounted game's closure state → discarded on unmount
  (scores are session-only by policy; never persisted).
- The router itself registers exactly one persistent listener:
  `hashchange → render`. In Rust, the engine owns an analogous
  `Screen` trait with `mount(&mut Ctx) -> Box<dyn Mounted>` and
  `Mounted::unmount(self)`; the router swaps the boxed screen and the
  old one's `Drop` runs teardown.

### Boot sequence (`main.ts` order — preserve it)
1. `migrate_legacy()` — one-time localStorage key moves (see §6).
2. `apply_on_boot()` — read shared settings, push `muted` into the audio
   layer.
3. register `hashchange → render`.
4. `render()` once (initial route from the launch hash).
5. `try_lock_landscape()` (see §7).
6. `register_service_worker()` (web only; native host no-ops).
7. register `visibilitychange(hidden) → sync.flush()` and
   `pagehide → sync.flush()` (flush pending sync writes when the kid
   backgrounds the app).
8. stash `build_id()` somewhere debuggable.

App container is `#app` (the `<main>` in `index.html`). A sibling
`#confetti` canvas and `#rotate-hint` overlay live outside `#app` and
are never torn down by routing.

---

## 2. Shared chrome: home (back), long-press, mute

Built by `shared/chrome.ts`; every game's topbar reuses these. Topbar is
a `<header class="topbar">`; games append `[home, …hud…, spacer, mute]`.
Picker topbar is just `[flex-spacer, mute]` (no back button on home).

### Home / back button (`make_home_button`)
- `<button class="icon-btn home-btn" aria-label="Home">` with **no inner
  content** — the chevron is two CSS pseudo-element bars (`::before`
  rotated −45°, `::after` rotated +45°) meeting at a left tip. *Do not*
  use inline SVG or a single rotated-border box: both rendered wrong on
  the maintainer's iPad (empty button / single diagonal slash). In Rust
  draw the chevron as two rounded bars (12×3 px, 14×3 px ≥540px width)
  pivoting around their left-center, so the visual midpoint lands on the
  button center. This is a hard iOS-parity requirement.
- Tap → `on_home()` (router passes `navigate(Picker)`).
- **Long-press (500 ms) → parent settings.** Default handler is
  `open_parent_settings()`; a game may override via `on_long_press` to
  pass its own section (patterns/phonics do — §4).
- Long-press impl: on `pointerdown` start a 500 ms timer that sets
  `long_fired = true` and runs the long-press handler;
  `pointerup`/`pointercancel`/`pointerleave` clear the timer. On
  `click`, only fire `on_home()` if `!long_fired`. (So a completed
  long-press suppresses the tap.)
- `LONG_PRESS_MS = 500`. Same constant appears in the picker version
  long-press (§5) — share it.

### Mute button (`make_mute_button`)
- `<button class="icon-btn mute-btn" aria-label="Mute sounds"
  aria-pressed=...>` containing two spans: `🔊` (`.icon-sound`) and `🔇`
  (`.icon-muted`, initially hidden). Only one shows at a time.
- Paints from `load_shared().muted` on creation; click → `toggle_muted()`
  then repaint (swap glyph visibility + `aria-pressed`).
- **Mute is global/shared**, not per-game (one toggle for the whole app).
- Neutral glyphs only. 🌰 (acorn/hazelnut) is the PWA launcher icon
  **only** — never render it in-app. Back is the chevron, mute is the
  speaker. No topbar gear; settings live behind the back-button
  long-press.

In a native renderer the speaker glyphs are emoji rendered at
`clamp(32px,3.8vw,40px)` so they fill the button at iPad sizes — match
the visible-glyph size, not the layout box.

---

## 3. Shared settings model

`SharedSettings` (app-wide), stored as one JSON blob under
`fountouki.shared.settings.v1`:
```
struct SharedSettings {
  muted: bool,                 // default false
  sync_token: Option<String>,  // default None  — family namespace
  sync_endpoint: Option<String>, // default None — override of default URL
}
```
- `load_shared()` = DEFAULTS merged with stored (missing keys fall back).
- `save_shared(patch)` = merge patch over current, persist, return next.
- `apply_on_boot()` = read once, push `muted` to the audio layer.
- `toggle_muted()` = flip, persist, push to audio, return new value.

**Persistence policy (whole app):**
- Mute → shared (above).
- Everything else game-specific → `fountouki.<game>.<key>.v1`.
- Sync token/endpoint → shared, one family-level token spans all games.
- Scores/streak/level → **never persisted** (session-only).

---

## 4. Parent settings panel (`shared/parent-settings.ts`)

Opened by the back-button long-press. Parent-only; deliberately no
visible chrome on the gameplay screen.

### Structure
- Singleton: a module-level `open_panel` guard — if a panel is already
  open, `open_parent_settings()` is a no-op (no stacking).
- Full-screen modal: `<div class="parent-settings-panel">` (fixed,
  inset:0, dim backdrop `rgba(43,44,52,0.72)` + blur) containing a
  `<div class="parent-settings-card" role="dialog"
  aria-label="Parent settings">`.
- Card layout, top→bottom:
  1. `<h2>Parent settings</h2>`
  2. **Per-game section slot** (`.parent-section-slot`). If the caller
     passed a `section`, its `element` is appended here; otherwise the
     slot node is removed entirely.
  3. **Universal Sync section** (`<h3>Sync</h3>`), always present:
     - Token text input (`#parent-token`, autocomplete/autocap/
       autocorrect/spellcheck all off). Hint: "Same token on every
       device. Empty = no sync." Two buttons: **Generate new** (fills a
       fresh 16-char token, focuses+selects) and **Clear** (empties the
       field — note: only clears the *field*, the save happens on Done).
     - Endpoint text input (`#parent-endpoint`, same input hygiene),
       `placeholder` = `DEFAULT_ENDPOINT`. Hint: "Override only if
       you've moved the worker. Empty = default."
  4. `<button class="primary parent-close">Done</button>`.

### Behaviour
- On open: prefill token/endpoint inputs from `load_shared()` (null →
  empty string).
- **Save happens on close** (`close()`), not live: read both inputs,
  trim; empty string → `None`. `save_shared({ sync_token, sync_endpoint })`,
  then remove the panel and clear the singleton guard.
- Close triggers: Done button, click on the backdrop (`e.target ===
  panel`, i.e. outside the card), or `Escape` key (listener
  self-removes).
- `Generate new` → `generate_token()` (§4 sync), fill+focus+select.
- `Clear` → empty the token field only (commit on Done).
- `section.on_mount({ close })` is called after mount so a section's
  buttons (e.g. "Start over") can dismiss the panel — see below.

### Per-game section plug-in contract
```
struct ParentSection {
  element: Node,                         // slotted above Sync
  on_mount: Option<fn(api: { close })>,  // wire buttons that should close
}
```
A game supplies its section through the back-button's `on_long_press`:
`open_parent_settings({ section: build_<game>_section(...) })`.

#### Patterns section (`build_patterns_settings_section`)
`<h3>Patterns</h3>` then four controls + reset. Hooks struct gives the
game live-update callbacks (each persists + re-renders the current
round). Controls:
- **Pictures** `<select #ptn-theme>` → `ThemeChoice`, one of:
  `mix` ("Mix (auto)"), `emoji-animals` (Animals 🐶), `emoji-fruit`
  (Fruit 🍎), `emoji-vehicles` (Vehicles 🚗), `emoji-construction`
  (Construction 🏗️), `emoji-dinosaurs` (Dinosaurs 🦖), `shapes`
  (Shapes 🟥), `letters-upper` (Letters ABC), `letters-lower`
  (letters abc), `numbers` (Numbers 123). → `on_theme_change`,
  persist + `next_round()`.
- **Helpers** `<select #ptn-difficulty>` → `Difficulty`:
  `auto` ("Auto — gets harder"), `easy` ("Easy — pick from the row"),
  `hard` ("Hard — pick from all"). → `on_difficulty_change`,
  persist + `next_round()`.
- **Game** `<select #ptn-mode>` → `GameMode`: `next`
  ("What comes next?"), `unit` ("Find the repeating piece"). →
  `on_mode_change`, persist + `next_round()`.
- **Highlight the repeating piece** checkbox `#ptn-hint` → `bool`. →
  `on_hint_toggle`, persist + re-render current sequence.
- **Start over** button (`.ptn-reset`) — wired in `on_mount`: resets
  level=1, stars=0, streak=0, re-renders HUD, `next_round()`, then
  `close()`s the panel.
Selects/checkbox are initialised from `hooks.get_state()` on build.

#### Phonics mastery section (`build_phonics_mastery_section`)
`<h3>Phonics mastery</h3>` then a read-only report computed from
`fountouki.phonics.state.v1` (the SRS state; `validate(...) ?? empty`,
then `ensure_letters`). Read-only — no callbacks, no `on_mount`.
- If every letter `last_seen == 0`: just `<p class="hint">No phonics
  play yet.</p>`.
- Else: a summary row of counts — **mastered** (`box >= 4`), **strong**
  (`box >= 3`), **learning** (seen but `box < 3`), and **new**
  (`last_seen == 0`, only shown if >0). Boxes: `MAX_BOX=4`,
  `STRONG_MIN_BOX=3`, `MASTERED_BOX=4`, `INTRODUCED_BOX_MIN=1`.
- A `.mastery-grid` of one `.mastery-dot.box-N` per letter (a-z sorted),
  colored by Leitner box: `box-0` gray, `box-1` `#ffd6a8`, `box-2`
  `#ffb56e`, `box-3` `#4adf99`, `box-4` gold. `title`/`aria-label`
  = "{letter}: box {n}".
- A `nextLine` hint: letters currently in rotation but not yet settled
  (`box < INTRODUCED_BOX_MIN`) → "In rotation now: a · b · …. The next
  letter unlocks when one of these is graded correct."; if none unsettled
  → "All 26 letters in rotation."
- Trailing hint: "Each dot = one letter, colored by Leitner box (gray =
  new, gold = mastered)."

In the Rust port these sections are just "build a settings subtree +
optional close hook". Keep the universal Sync controls in the shell and
let each game contribute a `ParentSection`.

---

## 5. Sync client (universal token/endpoint)

`shared/sync.ts`. One opaque **family token** spans all games; request
path is `<endpoint>/<token>/<game>`.
- `DEFAULT_ENDPOINT = "https://fountouki-sync.fountouki.workers.dev"`.
- `DEBOUNCE_MS = 500`.
- Config is read fresh from `load_shared()` on every call → token/
  endpoint edits take effect mid-session. `read_cfg()` → `None` when no
  token; else `{ endpoint: sync_endpoint || DEFAULT_ENDPOINT, token }`.
- `pull<T>(game) -> Option<T>`: GET; non-OK / empty / `"{}"` / parse
  error → `None` (best-effort).
- `push<T>(game, blob)`: debounced PUT per game (coalesces, 500 ms).
- `flush()`: PUT all pending immediately. Called on visibilitychange
  (hidden) and pagehide so the last grades aren't dropped.
- `configured() -> bool`: token present.
- Singleton `sync` shared across the app (survives screen unmount).
- `generate_token()`: 16 chars from `[a-z0-9]`, `crypto`-random
  (~82 bits). Used by the Sync "Generate new" button.
- All network is best-effort: never crash gameplay if offline/down.

In Rust, model this as a host-provided `SyncTransport` trait (the CF
Worker is the web impl; native hosts can reuse the same HTTP path) plus
a debounce/coalesce layer in engine code.

---

## 6. Storage (namespaced)

`shared/storage.ts`. Keys: `fountouki.<area>.<name>.v1`.
- Areas: `shared` (app-wide), `<game-id>` (per-game).
- `load<T>` / `save<T>` / `remove` wrap `localStorage` with try/catch
  (storage may be blocked → silently no-op / `None`). Values are JSON.
- `migrate_legacy()`: one-time moves, run at boot **before**
  `apply_on_boot`. Currently one move:
  `patternplay.settings.v1 → fountouki.patterns.settings.v1` (skip if
  destination already exists; remove source after copy). Keep the move
  table so historical installs upgrade cleanly.

Native port: back this with a `KeyValueStore` trait
(localStorage on web, a plist/SharedPrefs/file-backed store native).
Preserve the exact key strings — installed devices have data under them
and sync round-trips through the same `<game>` names.

---

## 7. PWA: orientation, standalone detection, SW, update check

`shared/pwa.ts`.

### Build id
- `build_id()` returns the compile-time `__BUILD_ID__`, a compact UTC
  ISO `YYYYMMDDTHHMMSS` injected by the build. In Rust inject at build
  time (env/`build.rs`) and expose the same string.

### Orientation lock (`try_lock_landscape`)
- **Only when standalone.** Standalone detection:
  `matchMedia('(display-mode: standalone)').matches` OR the iOS-only
  `navigator.standalone` OR false.
- If standalone and `screen.orientation.lock` exists → `lock('landscape')`,
  ignore rejection (iOS rejects this; fine). Native hosts lock landscape
  via their own manifest/Info.plist instead — keep this behind the host
  trait.
- A CSS-only **rotate hint** is the real iOS enforcement: `#rotate-hint`
  overlay (`index.html`) shown via media query
  `(orientation: portrait) and (max-width: 540px)` → "Please turn me
  sideways!" with a rotating-phone illustration. The app is landscape-
  first; portrait phones get the hint, the topbar uses
  `(orientation: landscape) and (max-height: 540px)` compact rules.
  Reproduce the rotate-gate in the Rust renderer.

### Service worker + auto/manual update check (web only)
- `register_service_worker()`:
  - `?nosw` → unregister all SWs, clear all caches, reload to clean path
    (escape hatch).
  - Skip on localhost (`localhost`/`127.0.0.1`/empty host) unless
    `?sw=force`.
  - On `load`: register `./sw.js`; if a controller already existed, a
    later `controllerchange` reloads the page once (new SW activated).
    Boot counts as a recent auto-check.
  - `wire_auto_update_checks()`: on `visibilitychange→visible` and
    `pageshow(persisted)` (iOS BFCache resume), trigger
    `check_for_update()`, throttled to once per
    `AUTO_CHECK_THROTTLE_MS = 30 min`. Every path swallows
    offline/errors — an update check must never surface to the UI.
- `check_for_update() -> UpdateCheck` ∈ `{ unsupported, no-registration,
  error, current, updating }`. Resets the throttle window; calls
  `reg.update()`; `installing|waiting` → `updating` (controllerchange
  will reload), else `current`.
- Native ports have no SW; the host implements update checks its own way
  (App Store / Play). Keep `UpdateCheck` as the shared result enum so
  the picker version-stamp UI (§8) is host-agnostic.

---

## 8. Picker (home) layout

`picker.ts`. `mount(container, GAMES, on_pick) -> unmount`.

Structure (`<div class="picker">`):
1. **Topbar** `<header class="topbar picker-topbar">` = `[flex spacer,
   mute button]`. No back button on home. No acorn.
2. **Card grid** `<div class="picker-grid">`, one `<button
   class="picker-card" data-game=<id> aria-label=<label>>` per game:
   - `.picker-icon` — `g.render_icon(node)` if provided, else
     `textContent = g.emoji`.
   - `.picker-label` — `g.label` (incidental reading; navigation must
     work without it, per pedagogy baseline).
   - click → `on_pick(g.id)` (router → `navigate(Game{id})`).
3. **Build stamp** `<button class="picker-version">` at the bottom:
   - text = `format_build_stamp(build_id())` — parse the UTC ISO id,
     render `YYYY-MM-DD HH:mm` in **device local time**; on any parse
     failure return the raw id (diagnostics must not crash home).
   - `aria-label` = "Build {stamp}. Long-press to check for updates."
   - **Long-press (500 ms) → `check_for_update()`** (bypasses the auto
     throttle — explicit parent ask). Visible feedback by swapping the
     label: "checking…" → then per result: `updating` → "updating…"
     (leave it; controllerchange reloads), `current` → "up to date"
     (restore after 1800 ms), `no-registration`/`unsupported` → "no
     service worker" (restore 1800 ms), `error`/default → "update check
     failed" (restore 1800 ms). `busy` guard prevents re-entrancy; the
     synthetic post-long-press `click` is swallowed so a kid can't
     trigger anything by tapping the stamp.
- unmount = `container.innerHTML = ''`.

### Game registry (`registry.ts`)
```
struct GameDef {
  id: String,                 // route id, kebab/lowercase
  label: String,              // single word
  emoji: String,              // aria/fallback glyph
  render_icon: Option<fn(node)>,
  mount: fn(node, MountOpts) -> unmount,
}
struct MountOpts { on_home: fn() }   // tap-home handler
```
Two games registered, in order:
- **patterns** — label "patterns", emoji `🐶🐱🐶?`, custom icon
  `render_patterns_icon`: a `.picker-icon-sequence` of four
  `.picker-icon-cell`s `🐶 🐱 🐶 ?` (last is `.picker-icon-slot`) — a
  literal "what comes next?" teaser so the icon teaches the mechanic
  without reading.
- **phonics** — label "phonics", emoji `🌈🐸` (aria/fallback only),
  custom icon `render_phonics_icon`: `.picker-icon-phonics` scene with
  `🌈` (`.picker-phonics-rainbow`) above and `🐸`
  (`.picker-phonics-frog`) below — previews the reward scene.

Adding a game = add an entry here + an import. In Rust this is a static
slice/`Vec<GameDef>`; the icon renderers are scene-draw closures, not
HTML.

---

## 9. Manifest / shell document

`public/manifest.webmanifest`:
- `name` / `short_name`: "fountouki"; `description`: "Tiny games for
  preschoolers."
- `start_url` "."; `scope` "."; `lang` "en";
  `categories` `["education","kids","games"]`.
- `display`: **standalone**.
- `orientation`: **landscape-primary**.
- `background_color` / `theme_color`: `#fef6e4`.
- `icons`: `icon.svg` (any), `icon-192.png`, `icon-512.png`,
  `icon-maskable-512.png` (purpose `maskable`). The 🌰 acorn art is the
  launcher icon **only** — never in-app.

`public/index.html` shell essentials to reproduce / map to native:
- viewport `width=device-width, initial-scale=1, viewport-fit=cover,
  user-scalable=no`; `theme-color #fef6e4`.
- iOS PWA meta: `apple-mobile-web-app-capable=yes`,
  `mobile-web-app-capable=yes`, status-bar `default`, app title
  "fountouki", `apple-touch-icon = icon-180.png`.
- Body: `#rotate-hint` overlay (portrait gate), `#app` (`<main>`, router
  target), `#confetti` canvas (shared FX, outside `#app`).

Native ports set these via the platform manifest/Info.plist (landscape
lock, standalone status bar, app name, icons). Keep `background_color`
`#fef6e4` as the launch/splash color.

---

## 10. Rust port checklist (shell)
- [ ] `Route` enum + pure `parse_hash`/`hash_for`/`navigate` (golden
      tests for the regex + unknown-route fallback to Picker).
- [ ] Router owns one live `Screen`; mount returns a teardown; nav drops
      the old screen (verify listeners/timers/round-state die, persisted
      + sync survive).
- [ ] `make_home_button` with the two-bar chevron (iOS parity),
      500 ms long-press → parent settings, tap → on_home; long-press
      suppresses the tap.
- [ ] `make_mute_button` reading/writing shared mute; speaker/mute glyph
      swap + aria-pressed. Mute is global.
- [ ] `SharedSettings` + storage namespacing (`fountouki.<area>.<name>.v1`),
      `migrate_legacy` table, JSON, fail-soft.
- [ ] Parent panel: singleton guard, section slot, save-on-close,
      backdrop/Esc/Done close, Generate/Clear token, endpoint override.
- [ ] `ParentSection` contract + patterns (4 selects/checkbox + reset)
      and phonics (read-only mastery report) sections.
- [ ] Sync transport trait: path `<endpoint>/<token>/<game>`, 500 ms
      debounced push, flush on background, fresh config per call,
      16-char token gen.
- [ ] PWA/host trait: standalone detect, landscape lock + rotate gate,
      build id, `UpdateCheck` enum (web SW impl; native no-ops/host
      update).
- [ ] Picker: topbar(mute only) + card grid (icon renderers, labels) +
      version stamp with 500 ms long-press update check (busy guard,
      swallow synthetic click, label feedback + 1800 ms restore).
- [ ] Manifest/shell parity: standalone, landscape-primary, `#fef6e4`,
      the icon set, viewport/iOS meta, rotate-hint, #app, #confetti.
