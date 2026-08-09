# Ideas

Working list of game / theme / mechanic ideas. Generic — no personal
details (see `CLAUDE.md`).

## Themes that landed
- Rainbows — high motivation hit for the audience.
- Construction, animals, Seuss-style silliness, dinosaurs — viable next.

## Pedagogy baseline (all games here)
- Errorless learning — never let the kid sit in "I don't know".
- Monotonic progress — stars / progress bars never decrement.
- No time pressure, no fail states.
- Big tap targets; navigation works without reading.
- Theme as wrapper around the stimulus, not embedded clutter over it.
- ~5-minute sessions; frequent over long.

## Phonics (next build)
- Lowercase letter → sound, parent-graded flashcards.
- Leitner SRS (5 boxes); per-letter `{ box, due, lastSeen }`.
- Drip-in introduction in Jolly-Phonics order (`s a t i p n …`): at
  most `NEW_LETTER_BUFFER` (=3) unsettled letters in rotation at once.
  A new letter unlocks when an existing one reaches box ≥ 1. Already-
  introduced letters stay active even if INTRO_ORDER changes later.
- Rainbow wrapper: stripe / star per correct, no decoration on the
  stimulus itself.
- Single canonical emoji per letter early; variety set unlocks at
  box ≥ 2 (desirable difficulty once the basic mapping is taking).
- Miss → reveal-then-repeat: show a hint cue (canonical-word emoji),
  parent voices the sound, kid repeats; card returns soon. Errorless.
- Session arc: open → ~5-minute soft cap → "session done" celebration;
  stars feed an external reward (parent-set, off-app).

## Math
- **Number Scales (compare) shipped as the `compare` game**: symbolic magnitude
  comparison ("which is more, 3 or 7?") on a balance scale, parent-graded like
  phonics, difficulty ladder (far→near→read→teens→fewer) in `core::compare`.
- **Pond wrapper + Froggy shipped**: the scale lives on a lily-pad pond —
  **Froggy** (the series' teal hero, `RAINBOW[3]`) stands at the base and *holds
  the balance up from below* (calm while the child reads, a slight lean while the
  grown-up grades, a tongue-out hop when a correct answer tips the scale). Sky +
  calm water + cattails + a drifting dragonfly dress the play scene without
  crowding the number cards. New pond primitives live in `draw::scenery`
  (`lily_pad`, `cattail`, `dragonfly`, `pond`). The finale is a pond party:
  Froggy hoists a golden trophy on a big pad, friend frogs party on their own
  pads (party hats), numbered flag-bunting counts the compares won, balloons bob
  and blossoms bloom when poked — matching the tracing/clock finale bar.
- Still future: counting, subitizing, cardinality, one-to-one correspondence
  before any arithmetic. Early numeracy has a wide research base.

## Construction writing (tracing v1 shipped; wrapper + SRS future)
- **Shipped as the `tracing` game (lowercase a–z)**: per-letter pen strokes
  derived from the official Tasmanian handwriting charts
  (`tools/handwriting_font/build.py`, skeleton + chart-routed; baked into
  `core/src/tracing_data.rs`), taught in the official letter families
  (anticlockwise `c o a d g q e s f` → stick `l i t j` → wave `u y` →
  clockwise `r n m h p b k` → diagonal `v w x z`), animated
  stroke-order demo, green start / red end dots (numbered for f t x i j),
  corridor finger-tracing over the faded font glyph (errorless, monotonic),
  5-letter sessions, persisted next-letter progression, parent start-over.
- **Construction wrapper shipped**: build-a-house progress meter (one part per
  letter: walls → door → windows → roof → chimney, crane-cable install + hammer
  clonk), pencil-drawn demo, rising pentatonic trace ticks, and a house-warming
  finale (letter bunting, doorbell door, lightable windows, hard-hat frog).
- Still future from the original sketch below: capitals + digits as *traceable*
  glyphs (the `tools/handwriting_font/` pipeline supports them — add a
  hand-authored chart route per glyph), guide fading across boxes, finished
  houses accreting into a street on the picker.
- Touch / stylus tracing of letters + digits. Construction wrapper:
  each correct letter unlocks a pre-made house part (window, door,
  roof tile, pipe) via a tradie-installs animation. Session arc =
  one finished house (~5 min, ~6–10 letters/digits). Finished houses
  optionally accrete into a street on the picker across sessions.
- Letterforms: the **Tasmanian Basic Handwriting Style** (unjoined /
  print form) — the style actually taught in Tasmanian schools, per
  the *Tasmanian Handwriting Guidelines (2023)* (DECYP, CC BY 4.0).
  There is no official font file and commercial TAS fonts' licences
  exclude apps, so **we author our own font**
  (`tools/handwriting_font/build.py` → `app/assets/fonts/handwriting.ttf`,
  family "Fountouki Handwriting"; see `/ATTRIBUTION.md`). Wired into
  Phonics / Patterns / Compare / Clock too, so recognition and
  production share one canonical shape. Joined/semi-joined Tasmanian
  cursive is for a much later phase.
- Lowercase sequencing follows the **official Tasmanian letter
  families** (anticlockwise → stick → wave → clockwise → diagonal):
  movement-pattern groups, so each family reuses one motor plan.
- Handwriting Without Tears is still the reference for the *other*
  axes: capitals-then-lowercase-then-digits developmental ordering if
  capitals land, and per-letter presentation — numbered start dots,
  direction arrows, and the kid-friendly stroke vocabulary ("big line
  / little line / big curve / little curve / magic c"), plus a term
  for any exit flick the shipped letterform carries.
- Drip-in + Leitner, same shape as Phonics. Per-letter
  `{ box, due, lastSeen }`, 5-box Leitner. At most
  `NEW_LETTER_BUFFER` unsettled letters in rotation; a new letter
  unlocks when an existing one reaches box ≥ 1. Already-introduced
  letters stay active if HWT ordering is later refined.
- Grading: parent-graded MVP, mirror Phonics (✓ / ✗ after each
  attempt). Auto-grade deferred.
- Auto-grade (later, optional): per-stroke start zone, end zone,
  dominant direction, distance from reference curve. Design as
  coaching not pass/fail — wrong start or direction → gentle
  in-line redirect (highlight the dot, replay that stroke), never a
  "wrong" state. Permissive tolerances; teaching lives in the
  animated reference, not the grade. ~1–2 weeks for mechanics; real
  cost is tuning against real play (expect 2–3 rounds).
- Each card: animated HWT reference plays first (numbered start
  dots + direction arrows), then a faded guide for tracing. Guide
  fades across boxes.
- Finger vs stylus: finger covers the cognitive side (order,
  direction, shape). A cheap silicone-tip stylus closes most of the
  motor/grip gap; Apple Pencil is overkill. Don't gate the build on
  stylus. Pencil-on-paper practice belongs alongside the app
  regardless — HWT itself uses crayons as primary, app as
  supplement.

## Grammar (future)
- Pronouns (he / she / I / you), SVO vs VSO word order for statements
  vs questions. Picture prompts + tap-the-right-sentence or
  drag-to-build.

## Care & grow (future)
- Tend to a companion (pet or character); each completed care task =
  visible growth (egg → hatchling → adult across many sessions).
- Errorless drag-to-target interactions: food → mouth, soap → bath,
  blanket → bed. Sequence learning baked into routines
  (bath: water → soap → rinse → towel).
- Monotonic: the companion never regresses; growth is the reward signal
  (no stars needed).
- Labeled actions / objects for vocabulary scaffolding (pictures + words).
- Empathy / theory-of-mind angle: routines for another creature mirror
  real daily routines.

## Mechanics worth reusing
- Confetti burst on correct (already in `src/confetti.ts`).
- Web Audio chimes (already in `src/sounds.ts`).
- Monotonic star counter (already in Patterns).
- Cross-device sync via family-token + CF Worker (see commit 3 + `server/`).
