# fountouki 🌰

Tiny static web app with kid-sized games. TypeScript → esbuild → `dist/`, no
runtime deps. Deploys to GitHub Pages from `main`.

```
npm install
npm run dev       # build + serve on http://localhost:5173
```

Other scripts: `build`, `typecheck`, `check` (typecheck + build),
`screenshots`, `icons`, `test`.

## Games

- **Patterns** — "what comes next?" sequence completion + find-the-
  repeating-piece. Themes (animals / fruit / shapes / letters / …),
  auto-scaling difficulty.
- **Phonics** *(in progress)* — parent-graded lowercase-letter → sound
  flashcards with a Leitner SRS, rainbow accumulator, and cross-device
  sync. Worker source + deploy in `server/`.

Future game wishlist in `docs/IDEAS.md`.

## Parent menu

Long-press the in-game ← (~500ms) to open the parent settings panel:
sync token + endpoint always, plus any per-game knobs (theme, mode,
mastery view, …) the current game contributes. Kids tap ← briefly →
home; parents hold ← → settings. No visible gear in the topbar.

## Audience

Built for two specific preschoolers on their devices — public repo,
not a general-purpose product. Working agreements: see `CLAUDE.md`.
