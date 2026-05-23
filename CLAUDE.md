# Working agreements

## Workflow
- Open a PR against `main` at the end of any code-changing task. Concise
  title + short summary. No test plan needed.
- Develop on the branch the session was started on; never push to `main`.
- Before pushing: `npm run check` (typecheck + build), `npm test`
  (Playwright). Visuals: also `npm run screenshots` and eyeball the
  output. Red tests block deploy — fix them even if unrelated.

## Working style

### Self-verify in a real browser before claiming done
- For new UI / gameplay: add a Playwright spec under `tools/` covering
  golden path + at least one edge case (wrong answer, reload mid-flow,
  empty state).
- Visual changes: run `npm run screenshots` and eyeball at phone
  landscape / tablet portrait / tablet landscape; consider iPhone Pro
  Max landscape (932×430) for safe-area edge cases.
- Tokens are cheap; bug reports are not.

### Delegate noisy work to subagents
- Use the Agent tool (Explore / general-purpose) for heavy Playwright
  runs, screenshot review, log scraping. Tell them what to report and
  how short (concise pass/fail + paths, not verbose output).
- Main thread stays for code, decisions, commits.

### Independent review for non-trivial work
- After implementing a meaningful slice (a new game, a major refactor,
  any non-trivial feature), launch an Agent (general-purpose) to review
  the result. Brief it lightly — file paths, scope, audience — but
  don't justify your choices or you'll bias the review.
- The review covers code quality + architecture + game design /
  pedagogy + visual layout. Final call is still yours unless the agent
  flags a question only the user can answer — escalate those.

### Tight docs
- Audience for every doc here (CLAUDE.md / README / TODO / IDEAS /
  code comments) is primarily a future Claude. Bullets over prose,
  sentences over paragraphs. Drop motivation unless the *why* is
  unrecoverable.

## No personal details in commits
- Repo is public; the audience is the maintainer's kids only.
- Keep kid names, ages, current-mastery state, or
  "for the user's son" framing out of every committed file. Generic
  preferences (themes, mechanics) are fine. Personal context lives in
  Claude's local memory, not the repo.

## Project shape
- TypeScript → esbuild → `dist/`. No runtime deps. Strict tsconfig.
- `src/main.ts` is the bundle entry. Hash routing: `#/` → picker,
  `#/<game-id>` → that game.
- `src/shared/` — `sounds`, `confetti`, `storage` (namespaced
  localStorage), `settings` (shared mute + sync token), `chrome` (home
  / mute buttons), `pwa` (SW + orientation lock), `sync` (cross-device
  client talking to the CF Worker in `server/`).
- `src/games/<id>/` — per-game code. Each game exports
  `mount(container, opts) -> unmount`.
- `src/games/registry.ts` — list of games. Add a game = import + entry.
- `server/` — CF Worker for sync. See `server/README.md` for live URL.
- Static: `public/index.html` (just the shell), `public/style.css`,
  `public/sw.js`.

## Settings model
- Mute is shared across all games (one toggle).
- Everything else is per-game under `fountouki.<game>.<key>.v1`.
- Sync state is per-game under one family-level token.
- Scores are session-only — never persist them.

## Audience & pedagogy baseline
- Preschoolers; big tap targets, minimal text, visual-first navigation.
- Word labels are optional reading practice, never required.
- Every game: errorless learning (never sit in "I don't know"),
  monotonic progress (stars / bars never decrement), no time pressure,
  theme as wrapper around the stimulus not embedded clutter, ~5-minute
  soft sessions.
- **Design for language delays and memory challenges.** Assume the
  player has a smaller working-memory budget than a typical preschooler
  and slower receptive-language processing:
  - One stimulus on screen at a time. No competing visual elements
    near the target.
  - Generous repetition + spaced practice (SRS, in-session re-presents).
  - Pictures *with* words for any concept-naming UI; never picture-only,
    never text-only.
  - Predictable layout across sessions; avoid surprise mechanics or
    randomized button placement.
  - Short, direct prompts. No idioms, wordplay, or sarcasm.
  - When grading is parent-mediated, lean on the parent for nuance
    (pacing, hints, model-and-repeat); the app's job is the structured
    excuse, not the assessment.

## PR descriptions
- For visual / UI work, attach a couple of representative screenshots
  to the PR body (under `## Screenshots`). Use `gh pr create` with
  `--body` referencing files from `screenshots/`, or upload via the GH
  web UI after the PR is open.
