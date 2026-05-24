# TODO / notes

For game wishlist see `IDEAS.md`. For working agreements see `../CLAUDE.md`.

## Gotchas

### PWA reinstall
Manifest changes (orientation, icons, name, colors) don't pick up in an
already-installed iOS PWA — remove & re-add to home screen. Android
Chrome usually picks up after a few launches. In-app code/asset changes
are fine — the SW handles those automatically.

### Layout
- CSS scales via `clamp(... vw|vh ...)`. `#app` caps at 960px wide.
- Topbar is `position: absolute` so the play-area can center in the full
  viewport (not just under the bar).
- Play area is centered via `margin-top: auto` / `margin-bottom: auto`
  on `.sequence` / `.choices`.
- Confetti anchors to the live `#sequence` rect; drops in-flight
  particles on resize so an orientation flip doesn't strand them.
- Rotate-to-landscape overlay shows in portrait on small viewports
  (< 540px wide).

## Debug hooks
- `window.__patterns` — current patterns round (template, answerId, …).
- `window.__fountouki_build` — build id stamp.
- `?nosw` — unregister SW + clear caches and reload.
- `?sw=force` — opt SW in on localhost (otherwise dev iteration skips it).

## Open work (patterns)
- Unit mode at L1 only renders 4 cells; "tap first / last" is heavy for
  pre-readers. Gating unit-mode to L3+ would help.
- L4 mixes 2-distinct templates so choice count doesn't grow vs L3 in
  some rounds. Cleaning that tier would smooth the curve.
- No unit tests for `patterns.ts` yet — pure functions, easy add.

## Open work (app)
- Per-game orientation override (currently landscape-only globally).
