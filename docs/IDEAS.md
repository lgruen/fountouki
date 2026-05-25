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

## Math (future)
- Counting, subitizing, cardinality, one-to-one correspondence before
  any arithmetic. Pedagogy research needed before designing — early
  numeracy has a wide research base.

## Letter & number drawing (future)
- Touch / stylus tracing → free draw. Auto or parent grading.
- Useful complement to Phonics: recognition vs production.
- See "Construction writing" below for a concrete first cut.

## Construction writing (future)
- Touch / stylus tracing, lowercase letters + digits first. Construction
  wrapper: each correct letter unlocks a pre-made house part (window,
  door, roof tile, pipe) via a tradie-installs animation. Session arc
  = one finished house (~5 min, ~6–10 letters/digits). Optionally
  finished houses accrete into a street on the picker / progress
  screen across sessions.
- Curriculum: Handwriting Without Tears stroke conventions per
  letter (start dots, numbered strokes, direction arrows). Sequence
  departs from HWT's capitals-first ordering: lowercase + digits
  first to mirror Phonics (kid needs lowercase for reading day-to-day).
  Capitals as a later pass if useful.
- Grading = coaching, not pass/fail (errorless):
  - Animated ghost reference plays first (numbered start dots +
    direction arrows, HWT style).
  - Kid traces over a faded guide; guide fades across sessions.
  - Per-stroke check: start zone, end zone, dominant direction,
    distance from reference curve. Wrong start / direction → gentle
    redirect (highlight the dot, replay that stroke), never a "wrong"
    state.
  - Tolerances tuned permissive; the teaching is the reference + the
    in-line nudges, not the grade.
- Parent-grade fallback toggle in the parent menu for letters where
  auto is unhelpful. Mirrors Phonics parent-grade pattern.
- Implementation cost: auto-grade mechanics ~1–2 weeks; real cost is
  tuning tolerances against actual play (expect 2–3 iteration rounds).
  Parent-graded MVP would be ~3 days if the auto path stalls.
- Symbolic "drawing unlocks a pre-made part" chosen over literal
  "stroke morphs into plank" — easier to render consistently, keeps
  the visual on-brand.

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
