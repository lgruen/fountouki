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
- Rainbow wrapper: stripe / star per correct, no decoration on the
  stimulus itself.
- Single canonical emoji per letter early; variety set unlocks at
  box ≥ 3 (desirable difficulty after the basic mapping is solid).
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

## Grammar (future)
- Pronouns (he / she / I / you), SVO vs VSO word order for statements
  vs questions. Picture prompts + tap-the-right-sentence or
  drag-to-build.

## Mechanics worth reusing
- Confetti burst on correct (already in `src/confetti.ts`).
- Web Audio chimes (already in `src/sounds.ts`).
- Monotonic star counter (already in Patterns).
- Cross-device sync via family-token + CF Worker (see commit 3 + `server/`).
