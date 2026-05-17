# Working agreements for Claude in this repo

## Workflow

- At the end of every task that introduces or modifies code on a feature
  branch, create a pull request against `main` automatically — don't wait
  to be asked. Use a concise PR title, a short summary, and a test plan.
- Develop on the branch the session was started on; never push directly
  to `main`.
- Run `npm run check` (typecheck + build) and `npm test` (Playwright
  smoke tests) before pushing. Tests gate the deploy in CI, so a red
  test means a blocked release — fix it before pushing, even if the
  failure was preexisting and unrelated to your change. If the change
  touches layout or visuals, also run `npm run screenshots` and eyeball
  the result.

## Project quick facts

See `docs/TODO.md` for the full orientation. The shortest version:

- TypeScript → esbuild → `dist/`. No runtime deps. Strict tsconfig.
- Core files: `src/game.ts` (UI + state), `src/patterns.ts` (pure
  generator), `src/themes.ts`, `src/render.ts`, `src/sounds.ts`,
  `src/confetti.ts`. Static: `public/index.html`, `public/style.css`.
- Target audience: a 4-year-old. Bias toward big tap targets, clear
  visual grouping, and minimal text.
- Settings persist in `localStorage`; scores are session-only by design
  — never persist them.
