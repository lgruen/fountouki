# Phonics — port spec (Rust reimplementation)

Parent-graded lowercase-letter → sound flashcards. Errorless, monotonic,
no time pressure. Single stimulus on screen. SRS-driven drip-in of new
letters. Per-session reward = building a rainbow, then a full-viewport
"rainbow garden" done scene with a tappable frog mascot.

This file is the build instruction. Every constant below is load-bearing;
copy values exactly. Source of truth was the TS implementation in
`src/games/phonics/{game,srs,deck,mastery-section}.ts`,
`src/shared/{storage,sounds,confetti}.ts`, and the phonics CSS block in
`public/style.css`.

---

## 0. Glossary / model summary

- **Letter**: one of the 26 lowercase ASCII letters `a`–`z`.
- **Box**: Leitner box `0..=4` (`MAX_BOX = 4`). 0 = new/just-missed,
  4 = mastered.
- **Star**: a per-session counter, 0..=`SESSION_GOAL`. Lights one rainbow
  arc each. **Session-only — never persisted.**
- **Queue**: an in-memory permutation of currently-active letters; the
  carousel the kid is shown.
- **Active set / frontier**: which letters are eligible to be queued right
  now (drip-in gate). See §3.
- **Persistent state** (`PhonicsState`): per-letter box/due/lastSeen, plus
  schema + version counter. The ONLY thing saved to disk / synced.

---

## 1. Persistent state model

```rust
struct LetterState {
    box_: u8,   // 0..=4
    due: i64,   // epoch ms; "ready" when due <= now
    last_seen: i64, // epoch ms of last grade; 0 = never graded
}

struct PhonicsState {
    schema_version: u32, // == SCHEMA_VERSION (1)
    letters: BTreeMap<char, LetterState>, // keyed by lowercase letter
    version: u64,        // bumped on every mutating grade
}
```

Constants:

- `SCHEMA_VERSION = 1`
- `MAX_BOX = 4`

### 1.1 empty / fresh state

```
emptyState() = { schema_version: 1, letters: {}, version: 0 }
```

### 1.2 ensureLetters(state, now)

For every letter in `LETTERS` (= deck order, §2): if not present in
`state.letters`, insert `{ box: 0, due: now, lastSeen: 0 }`. Mutates in
place. Called: at mount after load, after any remote merge.

Net effect: a freshly-loaded state always has all 26 letters with at least
a box-0 default.

### 1.3 validate(raw) -> Option<PhonicsState>

Reject (return `None`) if any of:
- raw is null / not an object,
- `schemaVersion != SCHEMA_VERSION`,
- `version` is not a number,
- `letters` is missing / not an object.

Otherwise return `{ schema_version: 1, letters: raw.letters, version: raw.version }`
(keep the loaded letters verbatim; do NOT coerce per-letter fields here).
On any validation failure the caller falls back to `emptyState()`.

### 1.4 Interval table (`intervalFor(box) -> ms`)

```
box 0 -> 0
box 1 -> 2 min        (2 * 60_000)
box 2 -> 15 min       (15 * 60_000)
box 3 -> 6 hours      (6 * 60 * 60_000)
box >=4 -> 24 hours   (24 * 60 * 60_000)
```

(`MIN = 60_000 ms`, `HOUR = 3_600_000 ms`.) Early ramp is short so a kid
sees the same card 2–3× in one ~5-min session; long park between sessions.

### 1.5 Grade transitions

**gotIt(state, letter, now):** if letter exists:
```
box = min(MAX_BOX, box + 1)
due = now + intervalFor(box)   // uses the NEW box
last_seen = now
state.version += 1
```

**missed(state, letter, now)** — *soft decay*: if letter exists:
```
box = max(0, box - 1)          // drop ONE box, not to 0
due = now + intervalFor(box)   // uses the NEW box
last_seen = now
state.version += 1
```

Rationale for soft decay: one wobble must not blow away days of spacing;
dropping 2 boxes flooded later sessions with the same letter. A box-1
letter that is missed returns to box 0 and re-counts as "unintroduced"
(see §3) — the kid gets breathing room again.

If the letter isn't in the map, both functions are no-ops (no version bump).

---

## 2. Deck (letter → exemplars + intro order)

Each letter has a **canonical** exemplar (always used for the miss-hint;
clean anchor) and an optional **variants** list (extra exemplars unlocked
at box >= 2 for generalization).

```rust
struct Exemplar { emoji: &str, word: &str }
struct LetterCard { letter: char, canonical: Exemplar, variants: Vec<Exemplar> }
```

### 2.1 DECK (order = deck order = `LETTERS`)

`LETTERS` is exactly the order below: alphabetical `a..z`. (`LETTERS` ≠
`INTRO_ORDER`.) Canonical first, then variants:

| letter | canonical | variants |
|---|---|---|
| a | 🐜 ant | 🍎 apple, 🐊 alligator |
| b | 🐻 bear | 🦋 butterfly, 🎈 balloon |
| c | 🐱 cat | 🥕 carrot, 🐄 cow |
| d | 🐕 dog | 🦆 duck, 🦖 dinosaur |
| e | 🐘 elephant | 🥚 egg |
| f | 🐟 fish | 🐸 frog, 🌸 flower |
| g | 🐐 goat | 🍇 grapes, 🎁 gift |
| h | 🐴 horse | 🏠 house, 🎩 hat |
| i | 🛖 igloo (drawn vector — no glyph exists) | 🪻 iris |
| j | 🪼 jellyfish | 🎷 jazz, 🃏 joker |
| k | 🦘 kangaroo | 🗝️ key, 🪁 kite |
| l | 🦁 lion | 🍋 lemon, 🐞 ladybug |
| m | 🐵 monkey | 🌙 moon, 🍄 mushroom |
| n | 🪺 nest | 👃 nose, 🥜 nut |
| o | 🐙 octopus | 🦉 owl, 🍊 orange |
| p | 🐼 panda | 🍍 pineapple, 🐧 penguin |
| q | 👸 queen | 🪶 quill, ❓ question |
| r | 🌈 rainbow | 🐰 rabbit, 🤖 robot |
| s | ☀️ sun | 🐍 snake, ⭐ star |
| t | 🐢 turtle | 🐅 tiger, 🌳 tree |
| u | ☂️ umbrella | 🆙 up |
| v | 🚐 van | 🎻 violin, 🌋 volcano |
| w | 🐳 whale | 🌊 wave, 🍉 watermelon |
| x | 🩻 x-ray | 📦 box, 6️⃣ six |
| y | 🪀 yo-yo | 🟡 yellow |
| z | 🦓 zebra | 0️⃣ zero, 💤 zzz |

Note these emoji include variation selectors / ZWJ sequences (☀️, ☂️,
🗝️, 6️⃣, 0️⃣). Store the exact UTF-8 byte sequences; do not normalize.

### 2.2 INTRO_ORDER (drip-in / introduction order)

Jolly-Phonics grouping. Distinct from deck/alphabetical order:

```
group 1: s a t i p n
group 2: c k e h r m d
group 3: g o u l f b
tail:    j z w v y x q
```

Flat list (26 entries, exact order):
`s, a, t, i, p, n, c, k, e, h, r, m, d, g, o, u, l, f, b, j, z, w, v, y, x, q`

Rationale: group 1 (s,a,t,i,p,n) already forms many CVC words (sat, pin,
nap…), so phonics "clicks" early. Used only by the active-set gate (§3).

### 2.3 pickExemplar(letter, box, rng) -> Exemplar

```
card = lookup(letter)  // panic/Err on unknown letter
if box < 2 OR card.variants is empty: return card.canonical
pool = [canonical] ++ variants
return pool[floor(rng() * pool.len())]   // uniform over pool; canonical included
```

`rng()` in `[0,1)`. Default `Math.random`. Used in normal card display
when a letter has graduated (box>=2) for variety. The **miss-hint always
uses box 0 → canonical** (see §5.3).

(In the TS port, the in-game card actually renders only the *letter glyph*,
not the exemplar — see §5.2. `pickExemplar` is invoked in two places: the
miss-hint with box 0, and conceptually for variety; the only caller in
`game.ts` is the miss-hint. Keep `pickExemplar` available for completeness
but the playing card shows no emoji.)

---

## 3. Active set (drip-in gate) + queue building

### 3.1 Constants

- `NEW_LETTER_BUFFER = 3` — max simultaneous "not-yet-settled" letters.
- `INTRODUCED_BOX_MIN = 1` — a letter is "introduced" once box >= 1
  (i.e. graded correct at least once). Relapse to box 0 re-counts it as
  unintroduced.

### 3.2 activeLetters(state) -> Vec<char>

Walk `INTRO_ORDER` from the start; gather letters into `active` until the
frontier is hit, then STOP. Algorithm exactly:

```
active = []
unsettled = 0
for letter in INTRO_ORDER:
    if unsettled >= NEW_LETTER_BUFFER: break   // frontier reached — stop
    box = state.letters[letter].box  (default 0 if missing)
    active.push(letter)
    if box < INTRODUCED_BOX_MIN: unsettled += 1
return active
```

Key behaviors (all tested):

1. **Fresh learner** (all box 0): active set = first 3 INTRO_ORDER letters
   = `s, a, t`. (Each is box 0 → unsettled increments; after pushing the
   3rd the loop's next iteration hits `unsettled >= 3` and breaks.)
2. **Drip-in unlock**: a new letter becomes active only when an earlier
   unintroduced letter reaches box >= 1, freeing a slot. Unlock is
   observed on the *next queue rebuild*, not at the instant of grading.
3. **Parking out-of-order introduced letters**: the loop STOPS at the
   frontier, so introduced letters *beyond* the frontier are parked —
   their box is retained (never erased), they just aren't queued until the
   kid drips far enough down INTRO_ORDER to reach them. The `break` gates
   BOTH branches (introduced and new) — critical: a legacy state polluted
   to box>=1 across the whole tail has no box-0 letter left to stop on, so
   if the gate only counted box-0 letters it would leak the entire tail.
   Regression case (from the test): letters `i,h,m` at box 0, every other
   letter at box 2. Frontier = up to and including `m`
   (`INTRO_ORDER[0..=index_of('m')]` = `s,a,t,i,p,n,c,k,e,h,r,m`). `x`,`v`,
   `q` etc. must NEVER surface.

### 3.3 buildQueue(state, now, rng) -> Vec<char>

```
active = activeLetters(state).filter(|l| state.letters.contains(l))
due = active.filter(|l| state.letters[l].due <= now)
if !due.is_empty():
    return shuffle(due, rng)              // Fisher–Yates
else:
    shuffle(active, rng)                  // shuffle first…
    active.sort_by_key(|l| state.letters[l].box)  // …then STABLE sort by box asc
    return active
```

- **Due letters preferred**: genuine SRS spacing across a day. The queue is
  a *permutation* — each active letter appears once before any repeat
  (coverage + spacing for free).
- **Shuffled, not due-sorted**: consecutive sessions must not replay the
  same recency-ordered sequence (reads mechanical). Tested: 8 fresh mounts
  on the same seeded multi-due state must not all open on the same letter.
- **Fallback (nothing due)**: the impatient same-session case. Shuffle all
  active, then **stable** sort by box ascending → weaker (lower-box)
  letters first, within-box order stays shuffled. Use a **stable** sort
  (Rust's `sort_by_key` is stable — good).
- Avoiding the same letter twice in a row across rebuilds is the *caller's*
  job (§5.1), not buildQueue's.

### 3.4 shuffle (Fisher–Yates, in place)

```
for i in (1..arr.len()).rev():
    j = floor(rng() * (i + 1))
    arr.swap(i, j)
```

`rng()` injectable for deterministic tests; default uniform `[0,1)`.

---

## 4. Merge (cross-device sync)

`merge(local, remote) -> PhonicsState`:

- If `remote` is None → return local.
- Union of letter keys from both. For each key:
  - only in remote → take remote.
  - only in local → take local.
  - in both → take the one with **larger `last_seen`** (remote wins ties:
    `b.lastSeen > a.lastSeen ? b : a` — strict `>`, so equal lastSeen
    keeps `a` = local).
- `version = max(local.version, remote.version)`.
- `schema_version = SCHEMA_VERSION`.

Sync flow at mount (best-effort, non-blocking): pull remote → validate →
if valid, `state = merge(state, remote)`, `ensureLetters`, save, **rebuild
queue** (don't yank the kid mid-card; the current `currentLetter` is left
displayed, only the queue behind it changes). See §8 for client contract.

---

## 5. Card flow (the playing screen)

### 5.1 Session-runtime variables (NOT persisted)

```
stars: u32 = 0
streak: u32 = 0
queue: Vec<char> = buildQueue(state)   // at mount
current_letter: Option<char> = None
in_miss_reveal: bool = false
frog_taps: u32 = 0   // done-scene only
```

Constants:

- `SESSION_GOAL = 7` (full rainbow; must match the 7 arc colors §6).
- `REQUEUE_GAP = 4` (cards between a miss and that letter re-appearing).
- `ADVANCE_DELAY_MS = 700` (delay between "got it" and next card).
- `BURST_BASE = 22`, `BURST_STREAK_STEP = 8`, `BURST_AT_DONE = 140`.

### 5.2 showNextCard()

```
if stars >= SESSION_GOAL: showDone(); return
if queue.is_empty(): queue = buildQueue(state)   // active-set unlocks happen HERE
next = queue.pop_front()
// Never the same letter twice in a row when there's an alternative.
if next is Some(l) and l == current_letter and !queue.is_empty():
    alt = queue.pop_front()
    // reinsert the dup deeper: at index min(REQUEUE_GAP, queue.len())
    queue.insert(min(REQUEUE_GAP, queue.len()), l)
    next = Some(alt)
if next is None: return
current_letter = next
in_miss_reveal = false
// UI: show letter glyph, clear miss tint, hide hint,
//     show miss+got buttons, hide advance button.
```

The card shows **only the letter glyph** (Victorian Modern Cursive font),
no exemplar emoji. No exemplar is shown during a correct prompt — single
stimulus.

### 5.3 onMissed() — parent taps ✗

```
guard: current_letter is Some and !in_miss_reveal, else return
playTap()
missed(state, current_letter)   // soft decay, §1.5
persist()                        // save + sync.push
streak = 0
in_miss_reveal = true
ex = pickExemplar(current_letter, 0)   // box 0 => ALWAYS canonical
// UI: show hint emoji = ex.emoji, hint word = ex.word (small/muted),
//     add "miss" tint class to card (warm tint #fff6ef; letter stays
//     full strength — errorless), hide miss+got buttons, show advance.
queue.insert(REQUEUE_GAP, current_letter)  // re-queue at fixed gap 4
```

Note the re-queue index differs from §5.2: a plain `insert(REQUEUE_GAP, ..)`
at the literal index 4 (the TS `splice(REQUEUE_GAP, 0, x)` — if the queue is
shorter than 4, splice clamps to the end; replicate that: insert at
`min(REQUEUE_GAP, queue.len())`).

Miss does NOT add a star (monotonic). Letter goes to box 0 if it was box 1.

### 5.4 onAdvance() — parent taps → (only visible during miss-reveal)

```
playTap()
showNextCard()
```

Reveals the canonical exemplar (already shown by onMissed) then advances.
No star added.

### 5.5 onGotIt() — parent taps ✓

```
guard: current_letter is Some and !in_miss_reveal, else return
playTap()
gotIt(state, current_letter)    // promote, §1.5
persist()
stars += 1
streak += 1
newly_lit_arc_index = stars - 1     // outer→inner: stars=1 lights arc-0
paintRainbow(just_filled = newly_lit_arc_index)
hopLetter(hot = streak >= 3)        // hot streak: bigger/tilted hop
// rainbow pulse: add classes "pulsing" + "celebrating" to the arc SVG
//   remove "pulsing"     after 480 ms
//   remove "celebrating" after 650 ms
streak_boost = min(streak - 1, 5)   // 0..=5
playCorrect(streak_boost)           // pitch up per step
burst(BURST_BASE + streak_boost * BURST_STREAK_STEP)  // 22..=62 particles
after ADVANCE_DELAY_MS (700ms):
    clear "just-filled" from all arcs
    if stars >= SESSION_GOAL: showDone()
    else: showNextCard()
```

### 5.6 What resets vs persists

| Thing | On "got it"/"miss" | On reload / new mount | On "play again" |
|---|---|---|---|
| `box`/`due`/`lastSeen`/`version` | mutated + saved + synced | loaded from storage (+remote merge) | unchanged |
| `stars` | got:+1, miss:+0 | **reset to 0** (session-only) | **reset to 0** |
| `streak` | got:+1, miss:→0 | reset to 0 | (not explicitly; effectively 0) |
| `queue` | shift/splice | rebuilt | rebuilt |
| `frog_taps` | n/a | 0 | reset to 0 |
| rainbow fill | follows `stars` | empty (0) | repainted empty |

Scores are session-only — never persisted (project rule). The only thing
on disk is `PhonicsState`.

---

## 6. Rainbow progress (in-game arcs)

7 arcs (= `SESSION_GOAL`). SVG `viewBox = "0 0 240 80"`. Each arc is a
150°-chord circular arc sharing baseline `y = ARC_Y_HORIZON`, fanning out
like a real rainbow seen from the ground.

### 6.1 Geometry constants

```
ARC_CX = 120
ARC_Y_HORIZON = 70
ARC_HALF_ANGLE = 75° in radians = 75 * π / 180
ARC_SIN = sin(ARC_HALF_ANGLE) ≈ 0.96593
ARC_COS = cos(ARC_HALF_ANGLE) ≈ 0.25882
ARC_SAGITTA_OUTER = 65
ARC_SAGITTA_INNER = 25
```

### 6.2 buildArcPath(index, totalArcs) -> SVG path `d`

`index` 0 = outermost (red), `totalArcs-1` = innermost (violet).

```
t = if totalArcs == 1 { 0 } else { index / (totalArcs - 1) }   // 0..1 float
sagitta = ARC_SAGITTA_OUTER - t * (ARC_SAGITTA_OUTER - ARC_SAGITTA_INNER)
r = sagitta / (1 - ARC_COS)
w = r * ARC_SIN
// path string (coords formatted to 2 decimals):
"M {CX-w} {Y_HORIZON} A {r} {r} 0 0 1 {CX+w} {Y_HORIZON}"
```

i.e. move to left chord end, arc (radius r, large-arc-flag 0, sweep-flag 1)
to right chord end. Both endpoints sit on the horizon baseline. Format CX±w
and r to 2 decimal places (`.toFixed(2)`).

Each path: `stroke-width = 8`, `stroke-linecap = round`, `fill = none`.

### 6.3 Per-arc colors (ROYGBIV, outer→inner)

Applied only when the arc is "filled":

```
arc-0  #ff4d6d   (red)
arc-1  #ff8c42   (orange)
arc-2  #ffd166   (yellow)
arc-3  #2bd5a0   (green)
arc-4  #38b3e2   (blue)
arc-5  #6e72e7   (indigo)
arc-6  #b364e5   (violet)
```

Unfilled arc stroke = `transparent` (invisible — the letter is alone on
screen until the first correct). Transition: `stroke 380ms, stroke-width
320ms, opacity 220ms`.

### 6.4 paintRainbow(just_filled_index?)

```
for (i, arc) in arcs:
    arc.filled      = (i < stars)        // fill outer→inner; star 1 → arc-0
    arc.just_filled = (just_filled_index == Some(i))
arc_svg.visible = (stars != 0)           // hide the whole SVG when 0 stars
```

So star N lights arc-(N-1). The kid sees a genuine arc from the very first
correct (outermost stripe), not just a partial until star ~4.

### 6.5 Arc animations

- `.just-filled`: `arc-pop 650ms cubic-bezier(0.34,1.6,0.64,1)` — stroke-width
  8 → 18 (at 35%) → 8; plus drop-shadow `0 2px 6px rgba(0,0,0,.18)`.
- `.celebrating` (on the SVG, while a new arc pops): already-filled arcs
  that are NOT just-filled dim to `opacity: 0.35` so the new stripe is the
  hero.
- `.pulsing` (on the SVG): `transform: scale(1.07)`, transform-origin
  `50% 90%`, transition `380ms cubic-bezier(0.34,1.6,0.64,1)`.

To force a CSS animation restart when the same class is re-applied: remove
class, force layout flush (read offset/bounding rect), re-add class, then
remove after the duration via timer. In Rust/non-DOM, model as: trigger
animation state machine with an explicit restart.

### 6.6 Letter hop

`hopLetter(hot)`:
- always add class `hop`; if `hot`, also `hop-hot`. Remove both after 700ms.
- `.hop`: `letter-hop 650ms` — translateY 0 → -22px(28%) scale1.22 →
  +2px(55%) scale0.94 → 0.
- `.hop.hop-hot`: `letter-hop-hot 700ms` — bigger + tilted: -34px scale1.32
  rotate-8° (22%) → -12px scale1.16 rotate6° (45%) → +4px scale0.92
  rotate-2° (65%) → 0. transform-origin `50% 80%`.
- Idle (no hop): `letter-idle 5s ease-in-out infinite` — rotate 0 → -1.5°
  (35%) → 1.2° (70%) → 0.

Both hop keyframes use easing `cubic-bezier(0.34, 1.6, 0.64, 1)`.

---

## 7. Done scene (rainbow garden)

Full-viewport scene (NOT a modal card), shown when `stars >= SESSION_GOAL`.
Layout: soft sky gradient on top, green ground strip on bottom, big rainbow
across the middle that draws itself, rain falls, ONE hero plant sprouts, a
tappable frog mascot, maybe a critter, and two floating corner buttons.

### 7.1 showDone()

```
frog_taps = 0
clear all react-* classes from frog
paintGarden()        // pick + place the one hero plant
spawnRaindrops(plantEls)
maybeSpawnCritter()
done.visible = true
// stagger-draw the big rainbow arcs:
for (i, arc) in done_arcs:
    remove "just-drawing"
    after i * 110 ms:
        add "just-drawing"
        after 700 ms: remove "just-drawing"
playLevelUp()
burst(BURST_AT_DONE)           // 140
after 380 ms: burst(80)
after 900 ms: burst(60)
sync.flush()                   // push pending state immediately
```

The big done rainbow is built once with all 7 arcs already `.filled` (same
geometry as §6, but `stroke-width = 10`). Stagger delay = `index * 110 ms`.

### 7.2 Garden — ONE hero plant per session

`GARDEN_POOL` (11 emoji):
`🌻 🌷 🌹 🌼 🍄 🌵 🍓 🌽 🥕 🌺 🌸`

`pickGardenPlants()` returns a **singleton** array: one random pool entry
(`floor(rng()*len)`). The reward IS "what grew this time?" — variety lives
in *which* plant, not how many.

`paintGarden()`: clear garden, pick the plant, place a single
non-interactive element (`pointer-events: none`) at `left: 32%`, with
sprout delay `--sprout-delay = SPROUT_BASE_DELAY_MS = 600 ms`.

Plant CSS: `bottom: 14%` (landscape) / `bottom: 62%` (portrait), font-size
`clamp(56px,10vw,120px)`. Sprout animation `phonics-plant-sprout 600ms
cubic-bezier(0.34,1.6,0.64,1)` after the delay: from translateY 30px scale0
rotate-10° → -10px scale1.22 rotate6° (55%) → 0 scale1 rotate0°.

### 7.3 Rain

`spawnRaindrops(targets)`: clear rain layer; for each target, add a raindrop
positioned at the target's `left` (fallback `50%`), with `--drop-delay = 0ms`.
(So the single hero plant gets one drop at `left: 32%`, delay 0.)

Raindrop CSS: width `clamp(8px,1vw,12px)`, height `clamp(14px,1.8vw,20px)`,
blue gradient, teardrop border-radius. Animation `phonics-raindrop-fall
700ms cubic-bezier(0.4,0,0.6,1) forwards`, delay `--drop-delay` (default
600ms in CSS, overridden to 0ms here): opacity 0→1(25%)→1(85%)→0; transY
-120%→…→110%.

Choreography: drop fires immediately (delay 0); plant sprouts ~600ms later —
"rainbow → rain → growth", rain visibly lands as the plant scales in.

### 7.4 Critter (surprise, not entitlement)

`maybeSpawnCritter()`: clear critter layer; with probability **40%** spawn
one (i.e. `if rng() > 0.6 { return }` — return early 60% of the time, spawn
40%). Critter emoji uniformly from `['🦋', '🐝', '🐞']`
(`floor(rng()*3)`).

Critter CSS: starts `top: 30%, left: -10%`, font-size `clamp(28px,4vw,48px)`,
animation `phonics-critter-drift 7s ease-in-out 2.4s forwards` — drifts L→R
across the viewport (translateX 0→28vw→50vw→80vw→120vw with vertical
bobbing and rotation), fading in at 8% and out at 100%. The 2.4s delay lets
the garden settle first.

### 7.5 Frog mascot

Recurring central character. Button, tappable. Idle: `phonics-frog-idle
3.4s ease-in-out infinite` — gentle breathe (translateY 0→-2px, scale
(1,1)→(1.02,0.98)). transform-origin `50% 100%`. font-size
`clamp(58px,11vw,130px)` (landscape) / `clamp(72px,14vw,180px)` (portrait).
Glyph: 🐸. Positioned center-bottom (`left: 50%; bottom: 6%` landscape /
`56%` portrait, `translate(-50%,0)`).

`FROG_REACTIONS = [react-hop, react-twist, react-big, react-spin]` (4,
**cycles, does not escalate**).

`onFrogTap()`:
```
frog_taps += 1
reaction = FROG_REACTIONS[(frog_taps - 1) % 4]   // cycle
remove all 4 react-* classes
restartAnim(frog, reaction, clearMs = 700)        // add class, remove after 700ms
playFrog()
```
Every reaction is a *real jump* (frog leaves the ground); no in-place wobble
variants. Hop heights are em-scaled (track frog size across devices).

Reaction keyframes (all transform-origin `50% 100%`, easing
`cubic-bezier(0.34,1.6,0.64,1)`):

- `react-hop` → `phonics-frog-hop 650ms`: squash (scale 1.20,0.80 @15%) →
  rise -1em scale 0.92,1.12 @50% → land squash → settle.
- `react-twist` → `phonics-frog-twist 650ms`: squash → -1.1em rotate-18°
  @50% → overshoot rotate8° → settle.
- `react-big` → `phonics-frog-bighop 700ms`: deeper squash (1.28,0.74) →
  -1.8em rotate-10° @50% → settle.
- `react-spin` → `phonics-frog-spin 700ms`: squash → -1.5em rotate180° @50%
  → rotate340° @85% → rotate360°.

All durations sit under the 700ms class-removal timer so consecutive taps
replay cleanly.

### 7.6 Sky decor (ambient, non-interactive)

- Sun: top-left, radial-gradient yellow disc, `phonics-sun-pulse 4s
  ease-in-out infinite` (scale 1↔1.06 + glow).
- 5 clouds (`cloud-a..e`): white rounded rects with two `::before/::after`
  puffs, `phonics-cloud-drift Ns linear infinite` translateX 0→130vw.
  Per-cloud top/left/duration/delay/scale (already mid-drift at t=0):
  - a: top12% left40% dur46s delay-10s
  - b: top38% left18% dur62s delay-28s scale0.82
  - c: top4%  left72% dur78s delay-54s scale1.1
  - d: top52% left64% dur54s delay-38s scale0.72
  - e: top22% left-8% dur86s delay-6s  scale0.95
- Background gradient: `linear-gradient(to bottom, #cdefff 0%, #e6f6ff 55%,
  #fff7d6 100%)`. Ground: `linear-gradient(#6fcf6f, #4caf4c, #3a8c3a)`,
  top border `3px #2f7d2f`, height 28% (landscape) / 40% (portrait).

### 7.7 Done-scene buttons

Two floating round white buttons at bottom corners, visible from t=0 (kid
in a rush can leave anytime). NO mastery grid in the done scene (a 4yo with
language delays can't parse 26 colored dots; that data lives in parent
settings, §9).

- **Play again** (left, glyph `↻`, aria "Play again"):
  ```
  stars = 0
  frog_taps = 0
  paintRainbow()            // repaint empty
  queue = buildQueue(state)
  done.visible = false
  showNextCard()
  ```
- **Home** (right, glyph `⌂`, aria "Home"): call `onHome()`.

---

## 8. Persistence + sync contract

### 8.1 Storage key

Namespaced localStorage scheme: `fountouki.<area>.<name>.<version>`.
Phonics state key: **`fountouki.phonics.state.v1`**
(area=`phonics`, name=`state`, version=`v1`). Value = JSON of
`PhonicsState`. Save is best-effort (swallow storage errors).

### 8.2 persist()

```
save("phonics", "state", state)   // localStorage write
sync.push("phonics", state)       // debounced remote PUT
```
Called after every grade (gotIt / missed). `showDone()` additionally calls
`sync.flush()` to push immediately.

### 8.3 Sync client (for parity; can stub in a single-device Rust build)

- One opaque family **token** spans all games. Path = `/<token>/<game>`.
- Endpoint default `https://fountouki-sync.fountouki.workers.dev`,
  overridable. No token → pull returns None, push is a no-op.
- `push` is debounced (`DEBOUNCE_MS = 500`), coalescing pushes per game.
- `pull(game)` GETs the blob (None on no-token / non-OK / parse error).
- `flush()` sends pending pushes immediately.
- Mount flow: `validate(pull)` → if Some, `state = merge(state, remote)`,
  ensureLetters, save, rebuild queue. Don't interrupt the current card.

### 8.4 Mount/boot sequence (exact order)

```
state = validate(load("phonics","state")) ?? emptyState()
ensureLetters(state)
save("phonics","state", state)         // normalize on disk
stars=0; streak=0; queue=buildQueue(state); current_letter=None; in_miss_reveal=false
... build DOM ...
spawn async cloud pull (§8.3)
paintRainbow()       // empty (0 stars)
showNextCard()       // first card
```

Unmount: abort all timers/listeners, delete debug view, clear container.

---

## 9. Parent mastery section (settings panel)

Long-press the in-game back/home button (500ms) opens parent settings; the
phonics section reads (does not mutate) the saved state and renders a
summary + per-letter dot grid.

### 9.1 Buckets (thresholds)

```
MASTERED_BOX = 4   // box >= 4 → "mastered"
STRONG_MIN_BOX = 3 // box >= 3 (and < 4) → "strong"
                   // 1..=2 (seen) → "learning"
                   // lastSeen == 0 → "new" / "unseen"
```

For each letter (sorted alphabetically by `localeCompare`, i.e. a..z):
- `lastSeen == 0` → unseen, skip the box buckets.
- else: box>=4 mastered; box>=3 strong; else learning.

If ALL letters have `lastSeen == 0` → render "No phonics play yet." and
nothing else.

### 9.2 Output

- Summary line: `<n> mastered · <n> strong · <n> learning · [<n> new]`
  (the "new" span only if unseen > 0).
- A dot grid: one `mastery-dot box-<box>` per letter (a..z order), tooltip
  `<letter> · box <box>`. Dot colors:
  - box-0 `rgba(43,44,52,0.10)` (gray)
  - box-1 `#ffd6a8`
  - box-2 `#ffb56e`
  - box-3 `#4adf99`
  - box-4 `linear-gradient(135deg,#ffd84f,#ff9a3c)` + glow (gold)
- "Next up" line: `nextUp = activeLetters(state).filter(box < INTRODUCED_BOX_MIN)`.
  If non-empty: "In rotation now: **<joined ' · '>**. The next letter
  unlocks when one of these is graded correct." Else: "All 26 letters in
  rotation."

This is the only place the mastery data surfaces to a human — keep it out
of the kid-facing done scene.

---

## 10. Audio (synthesized, no asset files)

Single shared audio context, lazily created, resumed on user gesture
(suspended on iOS until then). Global `muted` flag (shared across games);
when muted, all sound calls are no-ops. Each "note" = an oscillator with a
gain envelope: ramp to peak in 10ms, exponential decay to 0.0001 by end;
osc stops 20ms after end. Default peak gain 0.18, default type `sine`.

- **playTap()** — on every grade button tap: one note 660Hz, 50ms,
  sine, gain 0.08.
- **playCorrect(streak)** — on "got it". Ascending triad C5/E5/G5
  (523.25, 659.25, 783.99 Hz) at starts 0.0/0.09/0.18s, durs
  0.18/0.18/0.28s. Pitch-shift all by `2^(min(streak,5)/12)` (semitones).
- **playLevelUp()** — on done scene. Fanfare 523.25@0.0,659.25@0.1,
  783.99@0.2,1046.5@0.32 (durs .14/.14/.14/.32).
- **playFrog()** — on each frog tap. Two "ri-bbit" syllables, triangle:
  220@0.0(.09,g.16), 300@0.05(.08,g.14), 200@0.18(.10,g.16),
  280@0.22(.09,g.14).
- `playIncorrect()` exists in the shared module but phonics does NOT call it
  (errorless — a miss plays `playTap`, not a sad sound).

---

## 11. Confetti (canvas burst)

`burst(count)` spawns `count` square particles from an anchor near the play
card. Physics: gravity `g = 600 px/s²`, `dt` capped at 0.05s. Particle:
- spawn x = anchor center ± `spreadX` (uniform), y = anchorBottom ± 10.
- vx uniform `[-220,220]`, vy uniform `[-360,-180]` (upward).
- size `[6,10]`, color from palette below, rot `[0,2π)`, vr `[-6,6]`,
  life `[1.0,1.6]s`.
- Drawn as a rotated filled rect `size × size*0.6`, alpha = clamp(life,0,1).
Palette: `#f582ae #ffd166 #06d6a0 #118ab2 #9b5de5 #ef476f`.

Anchor: the TS reads `#sequence` (the patterns game's card) if present;
phonics has no `#sequence`, so it falls to the default anchor: cx =
`innerWidth/2`, emitY = `min(innerHeight*0.55, 380)`, spreadX = 60. For the
Rust port, anchor the phonics burst to the rainbow/card region (center,
upper-mid). On viewport resize, drop in-flight particles.

Phonics burst counts: got-it = `22 + streak_boost*8` (22..=62); done scene
= 140, then 80 (@380ms), then 60 (@900ms).

---

## 12. Typography / fonts

- Letter glyph + parent-menu glyphs use **Victorian Modern Cursive**
  (`VicModernCursive`, Regular + Bold .ttf), single-story `a`/`g` — matches
  how the kid learns to write. Fallback stack: `-apple-system,
  BlinkMacSystemFont, "Segoe UI", "Comic Sans MS", "Comic Sans", system-ui,
  sans-serif`. Letter is `font-weight: 700`, `letter-spacing: -0.04em`,
  size `clamp(96px,20vw,200px)` base.

---

## 13. Action-row glyphs (drawn as shapes, NOT text)

iOS Safari renders unicode ✗/✓/→ with asymmetric bearings → visibly
off-center inside round buttons. So they are drawn as CSS pseudo-element
shapes (centered by construction). For the Rust renderer, draw the marks as
geometric shapes, centered on the button's geometric center:

- **Miss (✗)**: two crossing bars, each `0.55em × 0.14em`, centered, rotated
  +45° / -45°. Button is the SMALL neutral one (parent tap, not a reward):
  width/height `clamp(48px,6.5vw,70px)`, background `var(--card)`, color
  `var(--muted)`, inset ring `0 0 0 2px #e6e1d3`.
- **Got (✓)**: an L-shape (border-right + border-bottom of a `0.36em ×
  0.72em` box, radius on the corner), translated `(-65%,-65%)` then rotated
  45° so the visual center of mass lands at the button center. Button is the
  LARGE reward one: `clamp(72px,9vw,108px)`, background `var(--ok)`, color
  `var(--ok-strong)`.
- **Advance (→)**: a horizontal bar `0.55em × 0.12em` centered + a
  right-pointing triangle (`border-left 0.22em` solid currentColor, top/bottom
  `0.18em` transparent) translated to the bar's right end. Button background
  `var(--accent)`, color white, same large size.

Layout: a 2-column CSS grid, both columns sized to the LARGER (got) button
(`--phonics-slot`) so the two button CENTERS are symmetric about the row's
midline (a plain flex row reads as off-center because miss is smaller). The
advance button reuses the got slot (hidden until miss-reveal). Action row
must center under the card axis (this is an asserted iOS layout invariant).

---

## 14. Debug/observability hook (for tests)

The TS exposes `window.__phonics` for the Playwright suite:
```
{ letter, stars, in_miss_reveal, queue_length, state, frog_taps }
```
For the Rust port, expose an equivalent inspectable snapshot (e.g. a method
returning this struct) so the port's test suite can assert the same
invariants the JS test asserts (§15).

---

## 15. Behaviors the test suite pins (must hold in the port)

From `tools/phonics-test.mjs` — replicate these as Rust tests:

1. Fresh load: first card is a single `a–z` letter; `stars == 0`; all 26
   letters initialized in state.
2. Fresh learner rotation is limited to `{s,a,t}` (first 3 INTRO_ORDER) —
   walking 5 miss/advance cycles never surfaces a 4th letter, and all 3 do
   appear.
3. Legacy-polluted state (i,h,m box0; everything else box2): rotation stays
   within the Jolly frontier (up to & incl. `m`); `x`/`v`/`q` NEVER appear,
   even though they're "introduced" (box2). Also: never the same letter
   twice in a row (assert across 24 churned cards).
4. Rotation is shuffled: 8 fresh mounts of the same multi-due seeded state
   do NOT all open on the same letter (>=2 distinct first cards).
5. "Got it" → `stars += 1`, next card is a *different* letter, graded
   letter is now box 1.
6. "Missed" → hint shows (emoji non-empty), advance button replaces
   miss/got, `stars` unchanged (monotonic), graded letter reset to box 0
   (from box 1).
7. "Advance" → next card, no star added.
8. State persists across reload (localStorage blob identical); session
   `stars` reset to 0 on reload (session-only).
9. Drive to 7 "got it" → done scene appears, `stars == 7`.
10. Done scene: frog present & tappable; exactly 1 hero plant; 0 mastery
    dots inside the done scene; done rainbow has exactly 7 arcs.
11. Tap frog → a `react-*` reaction applied, `frog_taps` increments by 1.
12. "Play again" → `stars` reset to 0, `frog_taps` reset to 0, card visible
    again (single a–z letter).

---

## 16. Timing constants (one-stop reference)

| name | value | where |
|---|---|---|
| SESSION_GOAL | 7 | stars to finish; arc count |
| REQUEUE_GAP | 4 | re-queue offset after miss / dup-swap |
| ADVANCE_DELAY_MS | 700 | got-it → next card |
| BURST_BASE | 22 | got-it confetti base |
| BURST_STREAK_STEP | 8 | +confetti per streak step |
| BURST_AT_DONE | 140 | done-scene confetti |
| done burst followups | 80 @380ms, 60 @900ms | |
| SPROUT_BASE_DELAY_MS | 600 | plant sprout delay |
| done arc stagger | index * 110 ms | rainbow draw-in |
| pulsing class clear | 480 ms | rainbow pulse |
| celebrating class clear | 650 ms | rainbow dim-others |
| hop class clear | 700 ms | letter hop |
| frog react class clear | 700 ms | frog reaction |
| streak hop "hot" threshold | streak >= 3 | bigger/tilted hop |
| streak boost cap | min(streak-1, 5) | confetti + pitch |
| NEW_LETTER_BUFFER | 3 | max unsettled letters active |
| INTRODUCED_BOX_MIN | 1 | "introduced" threshold |
| MAX_BOX | 4 | top Leitner box |
| critter spawn probability | 0.40 | rng() > 0.6 → skip |
| Leitner intervals | 0 / 2min / 15min / 6h / 24h | box 0..>=4 |
