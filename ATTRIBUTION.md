# Attribution

Third-party material bundled in this repo, and what it requires.

## Handwriting letterforms — Tasmanian Basic Handwriting Style

- The app's learn-to-write letterforms follow the **Basic Handwriting Style**
  set out in the **Tasmanian Handwriting Guidelines (2023)**, © State of
  Tasmania (Department for Education, Children and Young People) —
  licensed **CC BY 4.0** (https://creativecommons.org/licenses/by/4.0/).
  - Guidelines + alphabet charts:
    https://publicdocumentcentre.education.tas.gov.au/library/Shared%20Documents/Handwriting.pdf
- **The font file is authored by this project**: `app/assets/fonts/handwriting.ttf`,
  family **"Fountouki Handwriting"**, built by `tools/handwriting_font/build.py`.
  - Provenance: glyph geometry is **re-derived from the letterforms as published
    in the official CC BY documents** — the build rasterizes them (from the
    documents' own typesetting, locally) and re-authors the geometry
    (skeletonize → refit centerlines → stroke-expand). The tracing stroke data
    (`core/src/tracing_data.rs`) uses hand-authored stroke-order routes read off
    the official charts.
  - **No third-party font software is redistributed** — no vendor font-program
    bytes are copied into the repo or any shipped artifact; the commercial
    Tasmanian-style fonts (schoolfonts.com.au) are neither bundled nor required
    at runtime.
  - Changes from the source charts (weight, metrics, spacing, outline fitting)
    are ours; the style attribution above covers the letterform design.

## UI font — Varela Round

- `app/assets/fonts/ui.ttf` — **Varela Round** by Joe Prince (later Hebrew work
  by Avraham Cornfeld), used unmodified.
- License: **SIL Open Font License 1.1**
  (https://openfontlicense.org/) — https://fonts.google.com/specimen/Varela+Round

## Emoji art — Twemoji

- The sprites in `app/assets/emoji/` are **Twemoji** (CC-BY 4.0). Full notice:
  [`app/assets/emoji/ATTRIBUTION.md`](app/assets/emoji/ATTRIBUTION.md).
