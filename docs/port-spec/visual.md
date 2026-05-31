# Visual spec — fountouki (for canvas/GPU redraw)

Source of truth: `public/style.css` (1401 lines), `public/index.html`,
`src/games/phonics/game.ts` (arc geometry), iPad-landscape + portrait
reference screenshots under `screenshots/webkit-ios/`.

We have a **free hand to refresh visuals** — but the warm, playful,
preschool feel is load-bearing. This documents what the CSS app
currently renders so the port can match-or-improve deliberately, not by
accident. "Keep vs refresh" at the bottom.

---

## 1. Color palette (every notable value)

### Core tokens (`:root`)
| Token | Hex | Role |
|---|---|---|
| `--bg` | `#fef6e4` | App background — warm cream. Also PWA `theme_color` / `background_color` and `<meta theme-color>`. |
| `--card` | `#fffdf6` | Card / button / topbar-pill surface — off-white, very slightly warmer than pure white. |
| `--ink` | `#2b2c34` | Primary text + glyphs (near-black charcoal, slightly blue). |
| `--muted` | `#6f6e77` | Secondary text, labels, version stamp, miss-button glyph. |
| `--accent` | `#f582ae` | Brand pink. Slot/target highlight, primary buttons, "advance" action. |
| `--accent-soft` | `#ffd6e6` | Pale pink fill behind slot/`?` target. |
| `--ok` | `#8bd3a6` | Success green (button fill, correct choice). |
| `--ok-strong` | `#2b9d5f` | Strong green — check glyph on the green button, star-pop flash. |
| `--bad` | `#f6b3a2` | Wrong-answer salmon (never harsh red). |

Note: cell/choice **shape fills** use pure white `#fff`, not `--card`.

### Shadow / shape
- `--shadow`: `0 6px 16px rgba(43,44,52,0.10)` — the single soft drop shadow used on nearly every raised surface.
- `--radius`: `18px` (cards, buttons, sequence bar, picker tiles).
- `--tap`: `72px` min tap target. `--gap`: `10px`.

### Star / pip indicators
- Star glyph: `#f6b800` (warm gold). Star-count pop flashes to `--ok-strong` at mid-keyframe.
- Patterns level-pips (6, ROYGBIV-ish, empty = `rgba(0,0,0,0.10)`):
  1 red `#ef476f` · 2 orange `#ff8c42` · 3 yellow `#ffd166` · 4 green `#06d6a0` · 5 blue `#118ab2` · 6 purple `#9b5de5`.

### Phonics rainbow arcs (7, outer→inner = arc-0→arc-6, ROYGBIV)
| Arc | Hex | Color |
|---|---|---|
| arc-0 (outermost) | `#ff4d6d` | red |
| arc-1 | `#ff8c42` | orange |
| arc-2 | `#ffd166` | yellow |
| arc-3 | `#2bd5a0` | green |
| arc-4 | `#38b3e2` | blue |
| arc-5 | `#6e72e7` | indigo |
| arc-6 (innermost) | `#b364e5` | violet |

Unfilled arc = `transparent` (invisible until earned). This is **a
different, slightly brighter ramp than the patterns pips** — the
rainbow's red leans rose-red, the pips' red is a deeper crimson. The
port can unify these into one canonical 7-stop rainbow ramp.

### Patterns shape-theme colors (from screenshot)
- Orange cone/triangle ≈ `#f5953f`; green circle ≈ `#16c79a` (these are
  `--shape-color` set per-item in JS; `.cell.shape::after` / `.choice.shape::after`
  draw the colored shape via `clip-path` + `border-radius`). Other themes
  exist (dogs/cats emoji theme shown on picker tile). Treat shape colors
  as a per-item data field, not fixed tokens.

### Cell group tints (patterns, when grouping pattern units)
- group-a: bg `#fff4e6`, inset border `#ffd9a8` (warm).
- group-b: bg `#e9f4ff`, inset border `#b4d6ff` (cool).

### Mastery dots (parent panel only)
box-0 `rgba(43,44,52,0.10)` · box-1 `#ffd6a8` · box-2 `#ffb56e` ·
box-3 `#4adf99` · box-4 gradient `135deg #ffd84f→#ff9a3c` + glow.

### Rainbow-done scene
- Sky gradient (landscape): `to bottom, #cdefff 0% → #e6f6ff 55% → #fff7d6 100%`.
- Sun: radial `circle at 35% 35%, #fff3a8 0% → #ffd76b 55% → #ffb347 100%`, glow `0 0 28px rgba(255,196,73,0.55)`.
- Clouds: `rgba(255,255,255,0.94)`, soft drop shadow `0 4px 8px rgba(0,0,0,0.06)`.
- Ground: `to bottom, #6fcf6f 0% → #4caf4c 60% → #3a8c3a 100%`, top border `3px #2f7d2f`, inner highlight `inset 0 3px 0 rgba(255,255,255,0.18)`.
- Raindrop: `to bottom, transparent → #6cc6ff 60% → #3aa8ee 100%`.
- Done-scene corner buttons: `rgba(255,255,255,0.94)` fill, ink glyphs, shadow `0 4px 12px rgba(0,0,0,0.18)` + `inset 0 0 0 2px rgba(0,0,0,0.04)`.

### Border / form chrome (parent settings)
- Input/select border `#e6e1d3` (warm tan), white field bg `#fff`.
- Overlay scrim `rgba(43,44,52,0.72)` + `backdrop-filter: blur(2px)`.

---

## 2. Typography

- **Body font**: system stack `-apple-system, BlinkMacSystemFont, "Segoe UI", "Comic Sans MS", "Comic Sans", system-ui, sans-serif`. On iOS this resolves to San Francisco (the screenshots show SF, not Comic Sans). Used for labels, topbar, buttons, settings.
- **VicModernCursive** (`public/fonts/vicmodcursive/`, weights 400 + 700): the **canonical letterform** — single-story `a`, single-story `g` — matching how the child learns to handwrite. `font-display: swap` (system fallback while loading). Used **only where letters/numbers are the learning stimulus**:
  - `.phonics-letter` (the big card glyph).
  - `.cell` and `.choice` in patterns (so letter/number pattern themes teach the same shapes). Emoji & shapes fall through to the emoji font / `::after` drawing.
  - NOTE: the `s` in screenshot `20-phonics-card.png` renders **bold sans (SF), not the cursive** — webfont hadn't loaded in that capture. The port should render the actual VicModernCursive glyph. Bake the font into the GPU atlas.
- **Weights**: bold `700` for stimulus letters, topbar, choices, picker labels are `600`. Action glyphs use `900` (drawn as CSS shapes, see §3).
- `.phonics-letter`: `letter-spacing: -0.04em`, `line-height: 1`, color `--ink`.

### Approximate sizes (clamp ranges, `min, preferred, max`)
| Element | clamp |
|---|---|
| Phonics letter (base) | `clamp(96px, 20vw, 200px)` |
| …tablet portrait | `clamp(140px, 30vw, 280px)` |
| …phone landscape | `clamp(56px, 22vh, 110px)` |
| Topbar text | `clamp(18px, 2.2vw, 24px)` |
| Stars count | `clamp(20px, 2.5vw, 28px)`; star glyph `clamp(22px, 2.8vw, 30px)` |
| Sequence cell glyph | `clamp(20px, 6.5vw, 60px)`; slot `clamp(34px, 7vw, 68px)` |
| Choice glyph | `clamp(38px, 5.5vw, 72px)` |
| Picker tile icon (emoji) | `clamp(48px, 8vw, 84px)`; phonics rainbow `clamp(44px, 7.5vw, 78px)`, frog `clamp(28px, 5vw, 50px)` |
| Picker label | `clamp(14px, 1.6vw, 18px)` |
| Phonics hint emoji | `clamp(40px, 6.5vw, 76px)`; hint word `clamp(12px, 1.4vw, 15px)` muted |
| Phonics action glyph | `clamp(36px, 5.2vw, 60px)` |
| Version stamp | fixed `12px` tabular-nums, opacity 0.75 |

---

## 3. Component styling

### Phonics card (`.phonics-card`)
- Surface `--card`, radius `18px`, soft shadow. Padding `clamp(20px,4vw,40px) clamp(30px,8vw,84px)` (wider horizontal than vertical). `min-width: clamp(180px,28vw,320px)`. Column flex, centered, gap 12px.
- Holds the big cursive letter (and, in miss-recovery, a hint emoji + small muted word below it). Miss state tints bg `#fff6ef` (faint warm), never red.
- Letter has a perpetual gentle idle sway (`letter-idle`, ±1.5° over 5s) and a springy hop on correct (`letter-hop`, translateY −22px scale 1.22; "hot"/streak variant adds rotation and bigger jump).

### Phonics action buttons (X / check) — `.phonics-actions`
- Two circular buttons in a 2-col grid sized to the **larger** button so the two centers sit symmetric under the card axis (a plain flex row read as off-center on iPad — a shipped bug). Slot var `--phonics-slot: clamp(72px,9vw,108px)`, gap `clamp(14px,2.5vw,30px)`.
- **Check / "got it"** (`.phonics-got`): the hero. `clamp(72px,9vw,108px)` circle, fill `--ok` green, glyph `--ok-strong`. The ✓ is drawn as a CSS bent-bar (border-right+border-bottom on a rotated box), NOT a font glyph — iOS mono ✓ has asymmetric bearings. Pop animation on press.
- **Miss / X** (`.phonics-miss`): deliberately smaller `clamp(48px,6.5vw,70px)`, neutral — `--card` fill, `--muted` glyph, inset `2px #e6e1d3` ring. It's a parent tap, must not compete with the green. ✗ = two crossing CSS bars centered on geometric center.
- **Advance / →** (`.phonics-advance`): `--accent` pink fill, white →, drawn as bar + CSS triangle.
- Port note: draw all three glyphs as vector strokes centered on the button's true center (the CSS goes to lengths to fix iOS glyph-bearing skew; a canvas port gets this for free if it strokes its own marks).

### Choice buttons (patterns) — `.choice`
- `--card` surface, radius 18px, shadow, min-height `clamp(72px,13vh,140px)`, big glyph/shape centered. Grid auto-fit `minmax(clamp(110px,18vw,200px),1fr)`, gap `clamp(10px,1.8vw,20px)` — usually 2 wide on the pattern screen.
- Correct → fill `--ok` + `pop` (scale 1.15 bounce). Wrong → fill `--bad` + `shake` (±6px horizontal). Press → scale 0.97. Disabled → opacity 0.85.
- Shape choices draw a `clamp(48px,6.5vw,84px)` colored shape via `::after` (clip-path / border-radius from `--shape-clip` / `--shape-radius`).

### Sequence cells (patterns) — `.sequence` / `.cell`
- Sequence bar: `--card` pill, radius 18px, shadow, `min-height clamp(80px,12vh,140px)`, nowrap row, centered, gap `clamp(6px,1vw,12px)`.
- Each `.cell`: flex `1 1 0` up to `max-width clamp(56px,9.5vw,104px)`, square (`aspect-ratio 1/1`), radius `clamp(12px,1.6vw,18px)`, **white** `#fff` fill, inset hairline `0 0 0 2px rgba(0,0,0,0.04)`.
- **Slot / target** (`.cell.slot`): the `?` to fill — `--accent-soft` fill, `--accent` text + inset `3px --accent` ring, perpetual `pulse` (scale 1.06, 1.6s). This pink pulsing `?` is the single "look here" affordance and recurs as the picker's patterns-tile last cell.
- Unit-pick mode adds tappable cells (inset 4px accent ring + soft-pink) and a big round green submit FAB (`.unit-submit`, `clamp(84px,11vw,130px)`, `--ok`, pulsing).

### Star / level-pip indicators (topbar)
- `.stars`: white pill, radius 999px, gold ★ + count. Count pops (scale 1.45, flashes `--ok-strong`) on increment. Patterns shows this ("★ 0" in screenshot).
- `.level-pips`: white pill holding 6 dots `clamp(12px,1.6vw,18px)`; empty grey, fill to the ROYGBIV pip ramp; `pip-pop` (scale 1.8 + ring) on fill.
- Phonics deliberately has **no star counter** — the growing rainbow IS the progress meter (avoids "quiz score" feel).

### Rainbow arcs (the phonics hero) — SVG `viewBox 0 0 240 80`
- 7 concentric semicircle arcs, `stroke-linecap: round`, `fill none`, `stroke-width 8` in-game (`10` in done scene, animating to `18`/`22` on the pop).
- **Exact geometry** (`game.ts`): center x `120`, baseline (horizon) y `70`, half-angle `75°` (`sin≈0.966`, `cos≈0.259`). Per arc index `i` of 7, `t = i/6`; sagitta interpolates `65` (outer) → `25` (inner); radius `r = sagitta/(1−cos)`; half-width `w = r·sin`. Path: `M (120−w) 70 A r r 0 0 1 (120+w) 70`. So arcs are wide flat-ish bows anchored on a horizon line, outermost = widest/reddest.
- In-card: SVG `clamp(220px,44vw,360px)` wide, aspect `240/80`, sits just above the card (negative bottom margin so they read as one unit). Arcs invisible until earned; new arc `arc-pop` (stroke-width 8→18) + drop-shadow; sibling filled arcs dim to opacity 0.35 during the celebration; whole rainbow `pulsing` scales 1.07.
- Done scene: same geometry blown up to `clamp(320px, max(80vw,60vh), 1100px)`, drawn arc-by-arc on entry (`done-arc-pop`, fades+swells each).

### Frog + ambiance (rainbow-done scene)
- Full-viewport scene (not a modal). Layered: sky gradient → drifting clouds (5, `phonics-cloud-drift` 42–86s, staggered) → pulsing sun upper-left → big rainbow drawn across upper-middle → green ground strip (28% landscape / 40% portrait) → raindrops fall → one hero plant (random emoji from a garden pool) sprouts → **🐸 frog mascot** centered on the ground, the single tappable focal.
- Frog `clamp(58px,11vw,130px)` (bigger in portrait), gentle `frog-idle` breathing; tapping triggers one of four real jumps (`react-hop` / `react-twist` / `react-big` / `react-spin`, em-scaled, all <700ms so consecutive taps replay cleanly). Drop shadow `0 4px 6px rgba(0,0,0,0.18)`.
- Occasional drifting critter (🐛-type) crosses once after 2.4s.
- Two low-weight round white corner buttons (replay ↻ left, home/next ⌂ right), visible from t=0 so a kid can leave anytime. Rainbow + frog are the heroes; chrome stays quiet.

### Confetti
- `<canvas id="confetti">` fixed full-viewport, `pointer-events:none`, `z-index 5`. Driven by `src/shared/confetti`. Classic falling-particle burst on wins (not styled in CSS — it's a canvas routine; the port reimplements it natively). Keep the celebratory burst.

### Picker (home) — `.picker` / `.picker-card`
- Vertically + horizontally centered grid of large rounded `--card` tiles (radius 18, shadow), auto-fit `minmax(clamp(120px,22vw,220px),1fr)`, capped `clamp(560px,88vw,960px)`. Two tiles today: **patterns** (a mini-sequence: dog/cat/dog emoji + a pink pulsing `?` slot cell) and **phonics** (rainbow emoji-arc swaying above a bobbing frog). Muted label under each.
- Discreet build-stamp pinned bottom-center (`12px`, opacity 0.75, tabular-nums), long-press = SW update check.

### Misc chrome
- Icon buttons (home ←, mute 🔊): white circle `clamp(44px,5.2vw,56px)`, shadow, press scale 0.96. Back chevron drawn as two CSS bars (not a font glyph or SVG — both prior approaches shipped invisible on iPad). Mute uses the speaker emoji at explicit px sizing.
- Phone-portrait **rotate-to-landscape overlay**: cream full-screen card with an animated phone outline that rotates −90°, "Please turn me sideways!" — only on `portrait + max-width 540px`; hides `#app`.

---

## 4. Layout per form factor

App shell `#app`: `max-width min(960px,100%)`, centered, `height 100dvh`, flex column, `justify-content center`. Padding uses `max(clamp(...), env(safe-area-inset-*))` on top/bottom so content clears notch/home-indicator. The **topbar is `position:absolute`** (top/left/right with the same safe-area-aware insets) so the play-area can center against the FULL viewport height, not the leftover space below the bar (otherwise the centered content sits below the visual midline).

### iPad landscape (primary platform, ~2388×1668 / 1194×834 CSS)
- **Phonics** (ref `20-phonics-card.png`): big cursive letter on a white card dead-center; ← top-left, 🔊 top-right (no center topbar content); X + ✓ centered just below the card, ✓ larger & green, X smaller & neutral. Lots of warm-cream breathing room. The rainbow appears above the card once stripes are earned.
- **Patterns** (ref `01-patterns-initial.png`): topbar = ← + star pill + 6-pip pill (left cluster) and 🔊 (right). Sequence bar centered mid-screen (cone/circle alternating + pink `?` slot at the end), two big choice buttons in a row below it. Everything centered on one vertical axis.
- **Picker** (ref `00-picker.png`): two big tiles side by side, centered; 🔊 top-right; build stamp bottom-center.
- **Rainbow-done** (ref landscape `22-...png`): full-bleed scene, ground 28% tall, rainbow spanning the upper-middle, frog + one plant on the ground, ↻ / ⌂ in bottom corners.

### iPad portrait
- Same structure, but tuned bigger so cards fill the surface (media `orientation: portrait and min-width: 540px`): phonics letter up to `clamp(140px,30vw,280px)`, card padding grows, action buttons up to `clamp(96px,12vw,140px)`. Picker tiles get fatter padding + bigger icons. Done scene (ref portrait `22-...png`): ground 40% tall, rainbow lifted to the upper third, sun bigger upper-left, frog + plant pulled up to the horizon (`bottom 56–62%`) so they don't hug the bottom of a huge green strip.

### Phone landscape (`orientation: landscape and max-height: 540px`)
- Tight vertical budget. Reduced `#app` top/bottom padding (`6px` / `18px` floors), smaller gaps, sequence padding shrinks, phonics letter `clamp(56px,22vh,110px)`, actions `clamp(52px,13vh,82px)`, card padding collapses. Everything still fits above the iOS home indicator. Parent panel also compacts so its Done button stays on-screen.

### Phone portrait
- Gameplay is hidden behind the **rotate overlay** (`max-width 540px`). The app is landscape-locked (`manifest orientation: landscape-primary`), so portrait phone is a "turn sideways" wall, not a real layout.

### Safe area
- Every screen edge that can touch the notch / home indicator / Android gesture strip uses `env(safe-area-inset-*)` folded into a `max()` with a px floor. The done-scene corner buttons and picker version stamp explicitly clear the bottom inset. The port must honor device safe-area insets per orientation.

---

## 5. What to keep vs what we could refresh

### Keep (the identity — do not lose these)
- **Warm cream `#fef6e4` ground + off-white cards + soft single drop shadow.** This is the whole "calm, warm, not-a-quiz-app" feel. The restraint around the *target* (one stimulus, lots of breathing room) is pedagogy, not blandness — keep it.
- **The rainbow as the phonics progress meter** (grows inner→outer, ROYGBIV, no numeric score). Strong, legible, monotonic. The big draw-it-out done scene is the payoff.
- **The 🐸 frog mascot** — the recurring character, the single tappable joy at the reward, the only "pet" in the app. Highest-equity element after the rainbow. Keep the playful tap-reactions (real jumps).
- **Confetti + springy micro-animations** (letter hop, choice pop/shake, pip/arc pop, star-count pop). The springy cubic-bezier `(0.34,1.6,0.64,1)` overshoot is the app's motion signature.
- **Brand pink `--accent` `#f582ae`** for "tap here" affordances (pulsing `?` slot, primary buttons).
- **VicModernCursive** for all letter/number stimuli — pedagogically load-bearing (matches handwriting). Must ship in the GPU font atlas; don't fall back to sans like the screenshot capture did.
- **🌰 stays the launcher icon only** — never render it in-app.
- **Errorless / monotonic / one-stimulus / big-tap-target** layout discipline.

### Could refresh / fix (free hand here)
- **Unify the rainbow ramp.** There are *three* near-but-not-equal rainbow palettes: the phonics arcs (arc-0..6), the patterns level-pips (6 colors), and the picker emoji-rainbow. Pick one canonical 7-stop ramp and reuse it everywhere; let the pips be a 6/7 subset of the same ramp. Current arc-0 red `#ff4d6d` vs pip red `#ef476f` is a near-miss that reads as inconsistency.
- **Frog & garden are emoji.** The frog, garden plants, critter, and picker dog/cat are platform emoji — they'll look different per OS and can't be art-directed or animated beyond CSS transforms. A GPU port is the chance to make the frog a **first-class drawn/rigged character** (squash-stretch, blink, eye-follow) and draw the plants/sun/clouds as vector art. This is the single biggest visual upgrade available. Keep the *silhouette/charm*, raise the *fidelity*.
- **Rainbow rendering**: SVG round-cap strokes are fine but flat. A GPU port could add soft inner glow / subtle gradient per band / a gentle shimmer on the done scene without losing the clean look.
- **Confetti** is a generic falling-particle burst — could be richer (varied shapes, a frog-themed burst, depth) now that it's native.
- **Sun/cloud/ground in the done scene** are CSS gradients + pseudo-element puffs — serviceable but a bit flat/banded; redraw as layered vector with softer light.
- **Choice "wrong" salmon `--bad`** is fine but the win (`--ok` flat green) is modest — the "would I play this?" bar wants the *correct* moment to feel bigger (the letter-hop helps; consider a richer choice-correct burst).
- The numerous **iOS-glyph-bearing workarounds** (CSS-drawn ←, ✗, ✓, →) are pure platform debt — a canvas port draws its own centered vector marks and deletes all of it.

---

### One-paragraph north star
Warm cream stage, off-white rounded cards floating on a soft shadow, one
big stimulus at a time, brand-pink "tap here" cues, springy overshoot on
every win, and a growing ROYGBIV rainbow that pays off in a full-screen
sky-and-meadow scene starring a frog you can poke. The port should keep
that exactly — and spend its new GPU budget on making the frog, garden,
and rainbow *characters* rather than emoji + SVG strokes.
