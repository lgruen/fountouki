# Patterns — implementation-ready port spec

Source of truth: `src/games/patterns/{game,patterns,themes,render,settings-section}.ts`,
`tools/{unit-mode-test,playtest}.mjs`, `src/shared/{storage,confetti}.ts`,
`public/style.css`. This document is exhaustive enough to reimplement the game
in Rust with byte-identical logic. Where the original relies on `Math.random`,
the Rust port must take an injectable RNG so the deterministic seed (see
§13) reproduces.

---

## 1. Overview

Two game modes share one core round generator:

- **`next`** ("What comes next?") — a repeating pattern is shown with a `?`
  slot at the end; the player taps a multiple-choice button for the missing
  item.
- **`unit`** ("Find the repeating piece") — the same sequence is shown with no
  `?` slot; the player selects a *contiguous range* of cells equal in length to
  the pattern period, then taps a submit button.

A round is built from a **template** (placeholder string like `"AAB"`) whose
distinct letters are mapped to distinct **Items** drawn from the active
**theme**. The unit is repeated to form the visible sequence.

Progression: a correct answer awards 1 star and +1 streak; 4 in a row levels
up (max level 6). Wrong answers reset the streak to 0 but never decrement stars
or level (monotonic progress).

---

## 2. Core data types

### Item (`themes.ts`)

```
ItemKind = Glyph | Shape

Item {
  id: String,          // stable id; equality + answer key are by id
  kind: ItemKind,
  glyph: Option<String>,   // for Glyph: the char(s) to render
  shape: Option<Shape>,    // for Shape: color + optional radius/clip
  label: String,           // accessible name / aria-label / debug
}

Shape {
  color: String,           // CSS color, e.g. "#ef476f"
  radius: Option<String>,  // CSS border-radius, e.g. "50%" or "6px"
  clip: Option<String>,    // CSS clip-path polygon, e.g. "polygon(50% 0, 100% 100%, 0 100%)"
}
```

Helper constructors in the TS:
- `glyph(id, char, label)` → `{id, kind:Glyph, glyph:Some(char), label}`.
- `shape(id, color, label, {radius?, clip?})` → `{id, kind:Shape, shape:Some{color,radius,clip}, label}`.

### Theme

```
Theme { id: ThemeId, label: String, items: Vec<Item> }
```

`ALL_THEME_IDS` = the `THEMES` map keys **in insertion order** (this order is
load-bearing — the "mix" theme picker indexes into it; see §6). Order:

```
[ emoji-animals, emoji-fruit, emoji-vehicles, emoji-construction,
  emoji-dinosaurs, shapes, letters-upper, letters-lower, numbers ]
```

### PatternRound (`patterns.ts`)

```
PatternRound {
  template:   String,     // e.g. "AAB"
  unit_items: Vec<Item>,  // index 0 == placeholder 'A', etc.
  visible:    Vec<Item>,  // full visible sequence, no '?' slot
  answer:     Item,       // the correct next item
  full_reps:  usize,      // # full repetitions visible at the start
  partial_len:usize,      // tail length beyond last full rep (0..period-1)
}
```

### Settings / enums (`settings-section.ts`)

```
ThemeChoice = mix | emoji-animals | emoji-fruit | emoji-vehicles
            | emoji-construction | emoji-dinosaurs | shapes
            | letters-upper | letters-lower | numbers
Difficulty  = easy | hard | auto
GameMode    = next | unit
```

### Runtime state (`game.ts`)

```
state {
  level: u32        = 1,        // 1..=6
  stars: u32        = 0,
  streak: u32       = 0,
  theme_choice: ThemeChoice = mix,
  difficulty: Difficulty    = auto,
  mode: GameMode            = next,
  show_hint: bool           = false,
  round: Option<PatternRound> = None,
  active_theme: Option<Theme> = None,
  locked: bool      = false,    // input lock during the post-answer pause
}
const MAX_LEVEL = 6;
```

---

## 3. Templates and difficulty (`patterns.ts`)

`TEMPLATES_BY_LEVEL` — an array indexed by `level-1`. A template's **period**
is its string length; its **distinct count** is the number of unique letters.

| Level (1-based) | Templates | Periods present | Distinct-item counts |
|---|---|---|---|
| 1 | `AB`, `AAB`, `ABB` | 2, 3 | 2 |
| 2 | `AB`, `AAB`, `ABB`, `ABC` | 2, 3 | 2, 3 |
| 3 | `AAB`, `ABB`, `ABC`, `AABB` | 3, 4 | 2, 3 |
| 4 | `ABC`, `AABB`, `AABC`, `ABBC`, `ABCB` | 3, 4 | 2, 3 |
| 5 | `AB`, `AAB`, `ABC`, `AABB`, `AABC`, `ABBC`, `ABCB`, `ABCD` | 2, 3, 4 | 2, 3, 4 |
| 6 | `ABCD`, `AABCD`, `ABCBD`, `ABCDE` | 4, 5 | 4, 5 |

- Pattern **period** spans 2..=5 across the game: period 2 (levels 1,2,5),
  period 3 (levels 1–5), period 4 (levels 3–6), period 5 (level 6 via `AABCD`,
  `ABCBD`, `ABCDE`).
- `distinctCount(template)` = number of unique chars. Needed pool size.

`chooseTemplate(level, rng)`:
```
idx  = min(level - 1, TEMPLATES_BY_LEVEL.len() - 1)   // clamp to last tier
tier = TEMPLATES_BY_LEVEL[max(0, idx)]   // fallback ["AB"] if somehow empty
return pick_rng(tier, rng)
```
So level > 6 reuses the level-6 tier (game caps level at 6 anyway).

`letterIndex(ch)` = `ch as u32 - 'A' as u32` (0-based: A→0, B→1, …, E→4).

---

## 4. RNG primitives (`patterns.ts`) — replicate exactly

These define the deterministic sequence. The Rust RNG must be a
`FnMut() -> f64` returning `[0,1)`; all consumers below pull from the *same*
stream in the *same order*.

`pickRng(arr, rng)`:
```
if arr.is_empty() { panic "pickRng: empty array" }
i   = floor(rng() * arr.len())
idx = min(arr.len()-1, max(0, i))   // clamp for rng()==1.0
return arr[idx]
```

`shuffle(arr, rng)` — Fisher–Yates, **high→low**, operating on a copy:
```
out = arr.clone()
for i in (1..out.len()).rev() {        // i = len-1 down to 1
  j = floor(rng() * (i + 1))
  swap(out[i], out[j])
}
return out
```
RNG-consumption order matters: `shuffle` of an N-element slice consumes exactly
`N-1` `rng()` calls (one per `i` from `len-1` down to `1`).

---

## 5. Round generation (`generateRound`) — exact algorithm

Input: `{ pool: Vec<Item>, level, rng = Math.random }`. The RNG calls happen in
this precise order:

1. `template = chooseTemplate(level, rng)` → **1 rng call** (`pickRng`).
2. `needed = distinctCount(template)`.
3. If `pool.len() < needed`, panic:
   `"theme pool has {n} items but template '{t}' needs {needed}"`. (All themes
   have ≥6 items except `numbers`=9; max `needed` is 5 → always satisfied. But
   keep the check.)
4. `unitItems = shuffle(pool.clone(), rng)[0..needed]` → **pool.len()-1 rng
   calls** (full shuffle of the whole pool, then take first `needed`). Note: the
   entire pool is shuffled even though only `needed` items are kept.
5. `period = template.len()`.
6. `fullReps = if period == 2 { 3 } else { 2 }`.
7. `tailMax = period - 1`. Then:
   ```
   r = rng()                              // 1 rng call (always consumed for the < 0.2 test)
   if tailMax <= 0 || r < 0.2 {
     partialLen = 0                       // NOTE: when tailMax<=0 the `< 0.2`
                                          // test still consumed an rng() via
                                          // short-circuit? -> NO. JS `||`
                                          // short-circuits: if tailMax<=0,
                                          // rng() is NOT called. See below.
   } else {
     partialLen = 1 + floor(rng() * tailMax)   // 1 more rng call
   }
   ```
   **Short-circuit detail (critical for seed reproduction):** the JS condition
   is `if (tailMax <= 0 || rng() < 0.2)`. Because `period >= 2` always (smallest
   template `AB`), `tailMax = period-1 >= 1 > 0`, so the left side is always
   false and `rng()` is *always* evaluated here. So in practice exactly 1 rng
   call for the `< 0.2` test, and a 2nd rng call only when that test fails
   (partial branch). Replicate: always call `rng()` once for the threshold; if
   `>= 0.2`, call `rng()` again for `1 + floor(rng()*tailMax)`.
8. Build `visible`:
   ```
   visible = []
   for r in 0..fullReps {
     for ch in template.chars() {
       visible.push(unitItems[letterIndex(ch)].clone())
     }
   }
   for i in 0..partialLen {
     ch = template[i]
     visible.push(unitItems[letterIndex(ch)].clone())
   }
   ```
9. Answer = item at the next position in the infinite repetition:
   ```
   nextCh = template[ visible.len() % template.len() ]
   answer = unitItems[letterIndex(nextCh)].clone()
   ```
10. Return `{template, unitItems, visible, answer, fullReps, partialLen}`.

**Visible-length formula:** `visible.len() = fullReps*period + partialLen`.
- period 2: `3*2 + partialLen` (partialLen ∈ {0,1}) → 6 or 7.
- period 3: `2*3 + partialLen` (∈ {0,1,2}) → 6,7,8.
- period 4: `2*4 + partialLen` (∈ {0..3}) → 8,9,10,11.
- period 5: `2*5 + partialLen` (∈ {0..4}) → 10,11,12,13,14.

Partial bias: `partialLen == 0` only when `rng() < 0.2` (≈1 in 5 rounds show a
clean cycle break; the rest end mid-cycle).

---

## 6. Choice building (`buildChoices`) — `next` mode only

`buildChoices(round, mode: easy|hard, pool, rng = Math.random) -> Vec<Item>`:

```
correct  = round.answer
fromUnit = round.unitItems.filter(|it| it.id != correct.id)   // unit items minus the answer

if mode == easy {
  return shuffle([correct, ...fromUnit], rng)   // exactly the distinct items in the unit
}

// hard:
targetCount = max(4, round.unitItems.len())
needed      = targetCount - 1 - fromUnit.len()
unitIds     = set(round.unitItems.map(id))
extras      = shuffle( pool.filter(|it| !unitIds.contains(it.id)), rng )[0 .. max(0, needed)]
return shuffle([correct, ...fromUnit, ...extras], rng)
```

Notes / consequences:
- **Easy** choice count = `unitItems.len()` = `distinctCount(template)` (2, 3,
  4, or 5). Every choice is a building block already visible in the row.
- **Hard** choice count = `max(4, distinctCount)`:
  - distinct 2 → 4 choices (correct + 1 unit-mate + 2 extras)
  - distinct 3 → 4 choices (correct + 2 unit-mates + 1 extra)
  - distinct 4 → 4 choices (correct + 3 unit-mates + 0 extras)
  - distinct 5 → 5 choices (correct + 4 unit-mates + 0 extras)
  - `needed` can be 0 or negative; `max(0, needed)` clamps; `extras` may be
    empty. If the pool can't supply enough distinct extras (small themes), the
    count falls short of 4 — accepted as-is.
- Distractors are *theme-pool items not in the unit*, chosen by shuffling the
  filtered pool and taking the first `needed`.
- RNG order in hard mode: `shuffle(filteredPool)` first, then
  `shuffle(final list)`.

`effectiveAnswerMode()` (game.ts) maps difficulty→mode:
```
easy -> easy
hard -> hard
auto -> if level >= 4 { hard } else { easy }
```

---

## 7. Theme catalog (exact glyph/shape sets)

All glyph items: `{id, glyph (the literal char/emoji), label}`. Listed in pool
order (order matters only as the input to `shuffle`; the shuffle randomizes).

### emoji-animals (label "Animals") — 28 items
dog 🐶 · cat 🐱 · rabbit 🐰 · bear 🐻 · panda 🐼 · tiger 🐯 · frog 🐸 ·
monkey 🐵 · lion 🦁 · fox 🦊 · cow 🐮 · pig 🐷 · mouse 🐭 · hamster 🐹 ·
koala 🐨 · elephant 🐘 · giraffe 🦒 · zebra 🦓 · horse 🐴 · unicorn 🦄 ·
penguin 🐧 · chick 🐤 · owl 🦉 · whale 🐳 · octopus 🐙 · fish 🐠 · bee 🐝 ·
butterfly 🦋
(ids equal the names above; labels equal names.)

### emoji-fruit (label "Fruit") — 8 items
apple 🍎 · banana 🍌 · grapes 🍇 · strawberry 🍓 · orange 🍊 · kiwi 🥝 ·
pear 🍐 · watermelon 🍉

### emoji-vehicles (label "Vehicles") — 8 items
car 🚗 · bus 🚌 · train 🚂 · plane ✈️ · rocket 🚀 · bike 🚲 · boat ⛵ ·
tractor 🚜
(plane glyph is "✈️" — includes the U+FE0F variation selector. boat is "⛵".)

### emoji-construction (label "Construction") — 8 items
crane 🏗️ (label "crane") · truck 🚛 (label "truck") · digger 🚜 (label
"digger") · cone 🚧 (label "traffic cone") · hammer 🔨 · wrench 🔧 · saw 🪚 ·
toolbox 🧰
(crane glyph "🏗️" includes U+FE0F. Note `digger` reuses 🚜, same emoji as
vehicles' tractor but a different id/label.)

### emoji-dinosaurs (label "Dinosaurs") — 8 items
trex 🦖 (label "T-rex") · sauropod 🦕 (label "long-neck dino") · croc 🐊
(label "crocodile") · turtle 🐢 · lizard 🦎 · dragon 🐉 · egg 🥚 · bone 🦴

### shapes (label "Shapes") — 6 items (Shape kind)
| id | color | radius | clip |
|---|---|---|---|
| red-circle | `#ef476f` | `50%` | — |
| blue-square | `#118ab2` | `6px` | — |
| yellow-triangle | `#ffd166` | — | `polygon(50% 0, 100% 100%, 0 100%)` |
| green-circle | `#06d6a0` | `50%` | — |
| purple-square | `#9b5de5` | `6px` | — |
| orange-triangle | `#ff8c42` | — | `polygon(50% 0, 100% 100%, 0 100%)` |

labels: "red circle", "blue square", "yellow triangle", "green circle",
"purple square", "orange triangle".

### letters-upper (label "Letters (ABC)") — 18 items (Glyph)
A B C D E F G H J K L M N P R S T Y
(id == glyph == label; **omits** I, O, Q, U, V, W, X, Z — note no I/O/Q etc.)

### letters-lower (label "letters (abc)") — 17 items (Glyph)
a b c d e f g h j k m n p r s t y
(omits i,l,o,q,u,v,w,x,z — and unlike upper, also omits `l`. So lower has 17,
upper has 18.)

### numbers (label "Numbers") — 9 items (Glyph)
1 2 3 4 5 6 7 8 9
(labels are spelled out: one, two, three, four, five, six, seven, eight, nine.)

**Pool-size vs template needs:** numbers=9, animals=28, others 6–8. Smallest
distinct-count requirement is 5 (level-6 `AABCD`/`ABCDE`). All pools ≥ 6, so
`generateRound` never panics. Hard-mode distractor padding may run short only
for the 6-item `shapes` theme at high distinct counts (acceptable).

---

## 8. `next` mode — interaction & scoring (`game.ts`)

### Render
- Sequence row: for each `visible[i]`, render a `.cell`. If
  `show_hint && mode==next`, add `group-a` (when `floor(i/period) % 2 == 0`)
  else `group-b` to color alternating unit groups. Append a trailing
  `.cell.slot` with text `?`, aria-label "missing item".
- Choices: `buildChoices(round, effectiveAnswerMode(), theme.items)`, one
  `.choice` button per item. `data-id` = item id, aria-label = item label.

### `onChoice(item)`
```
if locked || round is None { return }
playTap()
if item.id == round.answer.id {
  locked = true
  mark this button .correct
  stars  += 1
  streak += 1
  renderHud(justEarnedStar=true)   // star-count "bump" animation, 500ms
  playCorrect()
  burst(70)                        // confetti, 70 particles
  if streak >= 4 && level < MAX_LEVEL {
    level  += 1
    streak  = 0
    after 480ms: renderHud(_, justLeveledUp=true); playLevelUp(); burst(50)
  }
  disable all .choice buttons
  after 1100ms: nextRound()
} else {
  mark button .wrong
  playIncorrect()
  streak = 0                       // wrong resets streak; stars/level unchanged
  after 350ms: remove .wrong from button
  // (NOT locked — player may keep tapping; errorless: stays until correct)
}
```

Timing constants: correct→nextRound `1100ms`; level-up cascade `480ms`; wrong
flash clear `350ms`; star bump `500ms`.

---

## 9. `unit` mode — interaction & scoring (`game.ts renderUnitMode`)

### Render
- Sequence row: each `visible[i]` → `.cell.selectable`, role="button",
  tabIndex 0, click → `handleTap(i)`. **No `?` slot, no hint groups.**
- A single submit button `.unit-submit` with text `✓`, aria-label "Check my
  answer", appended to the choices area, **hidden** initially.

### Selection model
A half-open contiguous range `[start, end)` over cell indices. Initially
`start = end = 0` (empty). `len = end - start`.

`paint()`:
- For each cell i: toggle `.unit-pick` on iff `start <= i < end`.
- `submit.hidden = (end <= start)` (hidden when empty).

`handleTap(idx, cell)`:
```
if locked { return }
playTap()
if end <= start {            // nothing selected: start a 1-cell selection
  start = idx; end = idx + 1; paint(); return
}
if idx == start - 1 { start -= 1; paint(); return }   // extend left
if idx == end       { end   += 1; paint(); return }   // extend right
if idx == start     { start += 1; paint(); return }   // shrink from left edge
if idx == end - 1   { end   -= 1; paint(); return }   // shrink from right edge
bounceNo(cell)               // non-adjacent tap: ignore + bounce animation
```
- Order of checks matters when a 1-cell selection is tapped on itself:
  `idx==start-1`? no. `idx==end`? `end=start+1`, idx=start ≠ end. `idx==start`?
  yes → `start += 1` → empty selection (`start==end`), submit hides.
- `bounceNo`: add `.bounce-no` 280ms then remove (only an animation; no state
  change). Triggered on any tap that isn't an endpoint-adjacent extend/shrink.

`onSubmit()`:
```
if locked { return }
len = end - start
if len <= 0 { return }
if len == period {                       // CORRECT — any start offset is valid
  locked = true
  for k in start..end { cell[k]: remove .unit-pick, add .unit-correct }
  submit.hidden = true
  stars += 1; streak += 1
  renderHud(justEarnedStar=true)
  playCorrect(); burst(70)
  if streak >= 4 && level < MAX_LEVEL {
    level += 1; streak = 0
    after 480ms: renderHud(_, justLeveledUp=true); playLevelUp(); burst(50)
  }
  after 1300ms: nextRound()
} else {                                 // WRONG — length mismatch
  for k in start..end { cell[k]: add .unit-wrong }
  playIncorrect(); streak = 0
  after 600ms: { remove unit-wrong/unit-pick/unit-correct from ALL cells;
                 start = end = 0; paint() }   // red flash, then full reset
}
```
- **Correctness criterion is length-only**: any contiguous run of exactly
  `period` cells is correct regardless of phase/offset (the unit-mode test
  explicitly selects a non-zero offset and expects a star). This is because any
  `period`-length window of a periodic sequence is a valid rotation of the unit.
- Wrong-length submit: streak resets, no star, red flash 600ms then reset to
  empty. `unit` mode timing: correct→nextRound `1300ms` (vs 1100 in `next`);
  wrong flash `600ms`.

---

## 10. HUD: stars + level pips (`game.ts renderHud`, CSS)

- **Stars**: `.stars` pill = a gold `★` glyph (`#f6b800`) + numeric count.
  `starCountEl.textContent = stars`. On `justEarnedStar`, re-trigger the `bump`
  class (remove, force reflow, add) and clear after 500ms → CSS
  `star-count-pop` 480ms scale-up animation.
- **Level pips**: 6 dots (`MAX_LEVEL`). Pip `i` (1-based) gets `.filled` iff
  `i <= level`. On `justLeveledUp`, pip `i == level` gets `.just-filled`
  (`pip-pop` 700ms). Pip fill colors by index: 1=`#ef476f` red, 2=`#ff8c42`
  orange, 3=`#ffd166` yellow, 4=`#06d6a0` green, 5=`#118ab2` blue, 6=`#9b5de5`
  purple. Unfilled = `rgba(0,0,0,0.10)`.
- Topbar order: home button, stars pill, level-pips, mute button. CSS pushes
  `level-pips` with `margin-right:auto` so mute sits at the right edge.

Progression rule (both modes): `stars` += 1 per correct, never resets. `streak`
+= 1 per correct, resets to 0 on any wrong, and resets to 0 at the moment of a
level-up. Level-up fires when `streak >= 4 && level < 6` → 4 consecutive
corrects per level. Level never decrements. Reset (parent menu "Start over"):
`level=1, stars=0, streak=0`, re-render HUD, `nextRound()`.

---

## 11. Confetti (`shared/confetti.ts`) — `burst(count)`

Called `burst(70)` on a correct answer; `burst(50)` on level-up.

- 1 canvas `#confetti` (fixed, full-viewport, `z-index:5`), DPR-scaled.
- **Anchor**: the TS looks up `document.getElementById('sequence')`.
  **DISCREPANCY TO PRESERVE-OR-FIX:** the game assigns the sequence element
  `class="sequence"` (no `id`), so `getElementById('sequence')` returns null and
  the burst falls back to the default anchor. Effective behavior today:
  `cx = innerWidth/2`, `emitY = min(innerHeight*0.55, 380)`, `spreadX = 60`.
  (If an `id="sequence"` were present, it would anchor to the card's bottom edge
  with `spreadX = min(width/3, 140)`.) For the Rust port, replicate the *actual*
  default-anchor behavior unless you also add the id.
- Per particle (count of them): position `x = cx + rand(-spreadX, spreadX)`,
  `y = emitY + rand(-10,10)`; velocity `vx = rand(-220,220)`,
  `vy = rand(-360,-180)`; `size = rand(6,10)`; `color` = random of
  `[#f582ae,#ffd166,#06d6a0,#118ab2,#9b5de5,#ef476f]`; `rot = rand(0,2π)`,
  `vr = rand(-6,6)`; `life = rand(1.0,1.6)` seconds.
- Physics per tick: `dt = min(0.05, elapsed)`, gravity `g = 600 px/s²`;
  `vy += g*dt`; integrate position; `rot += vr*dt`; alpha = clamp(life,0,1);
  draw rotated rect `size × size*0.6`. Particle dies at `life <= 0`.
- `rand(min,max) = min + Math.random()*(max-min)` — uses the global RNG too, so
  confetti consumes from the same stream. (In the deterministic playtest this
  perturbs subsequent rounds; the Rust port's golden tests should either stub
  confetti out of the RNG path or include it identically.)

---

## 12. Persistence (`shared/storage.ts`)

- Namespaced key format: `fountouki.<area>.<name>.<version>`, version `v1`.
- Patterns settings key: **`fountouki.patterns.settings.v1`**.
- Payload (JSON):
  ```
  { themeChoice: ThemeChoice, difficulty: Difficulty,
    mode: GameMode, showHint: bool }
  ```
- `loadPersisted()` on mount: load that key; apply each field only if present
  (`themeChoice`/`difficulty`/`mode` truthy; `showHint` is a bool). Missing →
  keep defaults (mix / auto / next / false).
- `persist()` on any settings change (theme/difficulty/mode/hint) writes the
  full payload. Settings changes also call `nextRound()` (theme/difficulty/mode)
  or re-render the sequence (hint).
- **Scores are session-only**: `level`, `stars`, `streak` are NEVER persisted.
  Fresh mount always starts at level 1 / 0 stars / 0 streak.
- Legacy migration (`migrateLegacy`, app boot): one-time move of
  `patternplay.settings.v1` → `fountouki.patterns.settings.v1` if the new key is
  absent. Port should support reading the legacy key once if relevant.

---

## 13. Deterministic seed / playtest (`tools/playtest.mjs`, `unit-mode-test.mjs`)

Both tests override `Math.random` with a seeded PRNG via `addInitScript`. The
Rust port must offer an injectable RNG matching this exactly to reproduce golden
runs.

**PRNG (mulberry32-style), used by both tests** — `playtest` seed `0xC0FFEE`,
`unit-mode-test` seed `9001`:
```
seed: i32
random():
  seed = (seed + 0x6d2b79f5) as i32            // wrapping i32 add
  t = imul(seed ^ (seed >>> 15), 1 | seed)     // Math.imul = wrapping i32 mul
  t = (t + imul(t ^ (t >>> 7), 61 | t)) ^ t
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296 // u32 / 2^32 -> [0,1)
```
- `>>>` is logical (unsigned) right shift on the 32-bit value; `^`, `|` are i32
  bitwise; `imul` is 32-bit wrapping multiply. `(playtest)` re-masks `seed |= 0`
  at the top of each call (no-op given i32 storage). Implement with `i32`/`u32`
  wrapping ops and `i32::wrapping_mul` for `imul`.
- The state is process-global and shared by round generation, choice building,
  AND confetti `rand`. For byte-identical golden reproduction the consumption
  order must match: per round `next` mode = `generateRound` rng calls (§5) then
  `buildChoices` rng calls (§6) then `burst` rng calls on a correct answer.

**playtest.mjs behavior** (`node tools/playtest.mjs [rounds=40]`):
- Viewport 844×390 landscape (play orientation), DSR 2.
- Loads `#/patterns`, waits for `window.__patterns.answerId`.
- Each round: snapshot `window.__patterns`, count `.choice` buttons, then click
  `.choice[data-id="<answerId>"]` (always correct), wait for `answerId` to
  change (4s timeout; on timeout waits 800ms once for the level-up path).
- Records per round: level, stars, streak, template, themeId, answerId,
  visibleLen, choiceCount. Screenshots the first round at each new level into
  `screenshots/playtest/level-<L>-round-<r>.png`.
- Summarizes level transitions + per-level histograms of templates/themes/choice
  counts/visible lengths. Exits non-zero on any pageError or failure. It always
  picks the correct answer, so it levels up every 4 rounds: L1→L2 at round 5,
  L2→L3 at 9, …, reaching L6 at round 21 and staying there.

**unit-mode-test.mjs behavior** — drives `unit` mode through the parent menu and
asserts the §9 selection invariants:
- Opens parent settings by clicking `.home-btn` with a 700ms delay (long-press),
  selects `#ptn-mode = unit`, closes.
- Asserts: submit hidden until first tap; tapping cells 0,1 → 2 selected;
  tapping a non-adjacent cell (idx 4) leaves selection unchanged; tapping cell 1
  again shrinks to 1; submitting a length-1 selection awards a star **iff
  period==1** (never, since min period is 2) else no star + selection resets to
  0 picked; building a `period`-length selection starting at offset 1 and
  submitting awards a star and advances the round.

### Debug surface (`window.__patterns`) — port should expose equivalent
`exposeDebug()` after each `nextRound()`:
```
{ level, stars, streak, mode,
  themeId:   active_theme.id  | null,
  answerId:  round.answer.id  | null,
  template:  round.template   | null,
  visibleIds: round.visible.map(id) }
```

---

## 14. Round lifecycle (`nextRound`) and theme pick

`nextRound()`:
```
theme = pickTheme()
active_theme = theme
locked = false
round = generateRound({ pool: theme.items, level })
if mode == next  { renderSequence(round); renderChoices(round, theme.items) }
else             { renderUnitMode(round) }
exposeDebug()
```

`pickTheme()`:
```
if theme_choice == mix {
  idx = floor(rng() * ALL_THEME_IDS.len())
  return getTheme(ALL_THEME_IDS[idx] ?? emoji-animals)   // 1 rng call BEFORE generateRound
}
return getTheme(theme_choice as ThemeId)
```
**RNG order note:** in `mix` mode, `pickTheme` consumes **1 rng call** before
`generateRound`'s calls. In a fixed theme, zero. This must be reproduced for the
deterministic seed.

Boot sequence on mount: `loadPersisted(); renderHud(); nextRound();`.
Unmount: clear all pending timers, delete debug global, clear container.

---

## 15. Layout / styling crib (for the Rust renderer)

- `.cell`: `flex:1 1 0`, `aspect-ratio 1/1`, max-width `clamp(56px,9.5vw,104px)`,
  rounded, white bg, glyph font-size `clamp(20px,6.5vw,60px)`. Font family
  `'VicModernCursive', …` (single-story a/g) so letters/numbers match phonics;
  emoji/shapes fall through.
- `.cell.shape::after`: a 65%×65% block colored `--shape-color`, rounded
  `--shape-radius` (default 50%), `clip-path: --shape-clip` (default none).
- `.cell.slot`: accent-soft bg, `?` glyph, pulsing scale 1↔1.06 (1.6s).
- Hint groups: `.group-a` warm (`#fff4e6` bg, `#ffd9a8` ring), `.group-b` cool
  (`#e9f4ff` bg, `#b4d6ff` ring).
- `.choice`: card button, grid `auto-fit minmax(clamp(110px,18vw,200px),1fr)`,
  glyph size `clamp(38px,5.5vw,72px)`. `.choice.shape::after` block
  `clamp(48px,6.5vw,84px)` square. `.correct` → green (`--ok`) + `pop` 360ms;
  `.wrong` → red (`--bad`) + `shake` 320ms; disabled → opacity .85.
- `.cell.unit-pick`: accent ring (inset 4px) + accent-soft bg. `.unit-correct`
  green + pop; `.unit-wrong` red + shake; `.bounce-no` → 280ms scale-down bounce.
- `.unit-submit`: centered green check button, `clamp(84px,11vw,130px)` wide.
- Topbar is absolutely positioned (so play-area centers in the full viewport).
  Portrait phone (`max-width:540px`) shows a rotate-to-landscape overlay and
  hides `#app`.
