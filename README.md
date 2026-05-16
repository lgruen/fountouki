# Pattern Play

A tiny browser game for preschoolers to practice completing sequence patterns —
"AAB AAB AA?". No accounts, no analytics, no personal data. Works offline once
loaded.

## Quick start

```bash
npm install
npm run dev      # builds + serves on http://localhost:5173
```

Other scripts:

```bash
npm run build       # one-shot build into dist/
npm run typecheck   # tsc --noEmit (strict)
npm run check       # typecheck + build
npm run screenshots # render screenshots/*.png for visual review (uses Playwright)
npm run icons       # regenerate PWA icons from public/icon.svg
```

## Fullscreen on a phone

The site is a small installable web app (PWA): adding it to the home
screen launches it without browser chrome.

- **iPhone / iPad (Safari)**: open the site → tap the **Share** button →
  **Add to Home Screen** → tap the new "Pattern Play" icon to launch
  fullscreen.
- **Android (Chrome)**: open the site → menu (⋮) → **Add to Home
  screen** (or **Install app**) → launch from the new icon.

If you change `public/icon.svg`, regenerate the PNG variants with
`npm run icons`.

## How it works

- Patterns are generated from small templates (`AB`, `AAB`, `ABC`, …) and
  filled with items from a theme (emoji animals, shapes, letters, numbers).
- Difficulty rises with consecutive correct answers (level 1–6).
- Two modes:
  - **What comes next?** — tap the missing item.
  - **Find the repeating piece** — tap the first then last cell of the
    smallest repeating unit.
- Sounds are synthesized in the browser (Web Audio); confetti is a tiny
  canvas particle system.

## Deploying

Pushes to `main` are deployed to GitHub Pages via
`.github/workflows/deploy.yml`. Enable Pages → "GitHub Actions" in repo
settings and the URL will appear on the workflow run.

## License

MIT — see [`LICENSE`](LICENSE).
