# handwriting_font — we author our own Tasmanian-style handwriting font

Builds `app/assets/fonts/handwriting.ttf` ("Fountouki Handwriting"), the
learn-to-write typeface the games render, and hosts the skeleton/centerline
machinery the tracing game's stroke data is baked with.

```sh
uv run tools/handwriting_font/build.py          # font + proof sheet + charts
uv run tools/handwriting_font/build.py --traces # ... and core/src/tracing_data.rs
```

~8 s end to end. Never `pip install` — the scripts carry PEP-723 inline
metadata and `uv` resolves them.

## Sources

Three official PDFs, downloaded **once, by hand**, into `~/Downloads/tas-handwriting`
(override with `--pdf-dir`):

| file | from |
| --- | --- |
| `Handwriting.pdf` | <https://publicdocumentcentre.education.tas.gov.au/library/Shared%20Documents/Handwriting.pdf> |
| `Handwriting-Beginners-Alphabet-chart.pdf` | <https://publicdocumentcentre.education.tas.gov.au/library/Shared%20Documents/Handwriting-Beginners-Alphabet-chart.pdf> |
| `Handwriting-Capital-Alphabet-chart.pdf` | <https://publicdocumentcentre.education.tas.gov.au/library/Shared%20Documents/Handwriting-Capital-Alphabet-chart.pdf> |

Tasmanian Handwriting Guidelines (2023), © State of Tasmania (Department for
Education, Children and Young People), **CC BY 4.0**.

## Licensing stance

- The **guidelines document and its charts** are CC BY 4.0. Rendered chart pages
  are committed here as `tas-beginners-chart.png` / `tas-capitals-chart.png` —
  attribution line in `ATTRIBUTION.md` at the repo root.
- The **handwriting fonts embedded inside those PDFs** (`TasBegRegular`,
  `TasBegRegNum`, `TasBegBold`, from schoolfonts.com.au) are third-party
  commercial font *software*. They are **not** covered by the CC BY licence and
  are **never committed**: `build.py` extracts the embedded subsets into
  `reference/`, which is gitignored, and reads them locally.
- A **typeface design** (the shapes of letters) is not itself protected in the
  jurisdictions that matter here; the *font program* is. So the pipeline is
  deliberately centerline-first: it measures the reference, throws the outlines
  away, and re-authors every curve from our own pen and our own Bézier fit. No
  vendor bytes, no vendor outlines, no derived outline data ship — only geometry
  this tool generates.
- Sharing the built font outside this repo would want a fresh look at that
  reasoning. Inside the repo it is our own work, in the Tasmanian *style*.

## Committed vs local-only

Committed:
- `build.py`, `trace.py`, `routes.py`, this README
- `sheet.png` — proof sheet, rendered **from the built TTF**
- `debug-sheet-lowercase.png` — the routed tracing strokes (see `--traces`)
- `tas-beginners-chart.png`, `tas-capitals-chart.png` — page 1 of each chart at
  200 dpi; the human reference for stroke order
- the output, `app/assets/fonts/handwriting.ttf`

Local-only (gitignored):
- `reference/` — the three extracted vendor subsets and any intermediate rasters
- `~/Downloads/tas-handwriting/*.pdf` — never copy these into the repo

## Pipeline

Per glyph, `build.py`:

1. **Reference selection.** Letters `a–z A–Z` and `. ,` come from
   `TasBegRegular` (clean outlines). Digits come from `TasBegRegNum`, whose
   glyphs have the numbered stroke-order arrows **baked into the outline** as
   extra contours; those are stripped (a contour is an overlay if it is below
   `OVERLAY_AREA_EM2` of an em² *and* is not near-circular — the roundness
   escape is what saves the i/j dots, which are as small as an arrowhead).
   Two validations gate it: the same rule on `a–z A–Z` must reproduce
   `TasBegRegular`'s contour counts and silhouettes, and the per-digit kept
   count must match the clean `TasBegBold` digits.
2. **Render** at 1024 ppem through FreeType, no hinting, threshold > 127.
3. **Skeletonize**, prune spurs, then decompose into polylines by walking every
   skeleton edge once (`trace.skeleton_polylines`). Branches that merely
   continue through a junction are stitched back together, so a V/W/A vertex
   stays one stroke and the pen mitres a point through it instead of capping
   both arms round and blunt. Small round components (i/j dots, `.`) become
   dots, sized from the reference.
4. **Terminals**: trim a free end back out of ink narrower than that stroke's
   own width (a skeleton keeps going into a taper, and a round pen laid there
   paints a stub), otherwise push it out so the round cap lands on the reference
   ink boundary — a skeleton stops about one pen radius short of a stroke end.
   A trimmed end is never re-extended (that guard keeps the A/N/M apexes
   blunt instead of stubby), so a long gradual exit taper is swallowed whole;
   the known casualty — the `2` base bar — is re-extended explicitly via
   `TERMINAL_EXTEND`, in final font units along the end tangent.
5. **Smooth** (boxcar) and resample at a fixed arc-length step.
6. **Normalize** to upem 1000 with a single scale `k`, pinned so the *built*
   `x` has an ink top of exactly 400. (Stroke expansion is a Minkowski sum, so
   it commutes with uniform scaling — one measurement fixes `k` exactly.)
7. **Pen width** = median of 2 × distance transform sampled along every letter
   centerline; dots keep their measured radius.
8. **Stroke-expand**: shapely `buffer` per centerline — round caps, mitred
   joins — unioned per glyph.
   For `a`–`z` this expansion happens **twice**: the extraction centerlines
   carry medial-axis junction artifacts, so their buffer union grows small
   blobs at every join. That stage-1 font is only the routing scaffold — the
   tracing emitter (below) traces it, and stage 2 rebuilds each traced
   letter's outline as the pen extrusion of its *traced strokes*, clipped to
   the stage-1 silhouette (keeps the calibrated caps/terminals, and nothing
   can poke past the reference-gated ink). The shipped glyph ink and the
   tracing-game template are therefore the same drawing.
9. **Outline fit**: split each ring at its corners (tangents measured over an
   arc-length window well under the pen radius, or the round caps read as
   corners), least-squares cubic Béziers (Schneider), then `cu2qu` → `glyf`.
10. **`!` and `?`** have no clean reference, so they are hand-authored
    centerlines in `authored_glyphs()` — same pen, same measured slope, cap
    height — and run through the same expand → fit path.
11. **Metrics**: letters and `. ,` keep the reference advance × `k` (numbers are
    facts); digits and `! ?` are set on a uniform 90-unit sidebearing so
    `10 11 12 20` packs as numerals rather than as spaced-out digits.
12. **Proof sheet** rendered back out of the built TTF.

Every stage asserts: coverage, contour counts, x-height 400 ± 3, ascender /
cap 770–800, descender −380…−410, pen 28–45, all advances > 0, TTF round-trips
through fontTools and renders through FreeType. The build prints an FNV-1a-64
fingerprint of the final TTF bytes (`fnv1a64()`); `--traces` bakes it into
`tracing_data.rs` as `SOURCE_FONT_FNV1A64`, a staleness guard. The build is
deterministic (fixed `head` timestamps), so the same sources give the same
fingerprint.

### Known limitation

A round pen cannot reproduce a *very* sharp apex: at `A`/`N`/`M` the medial axis
runs into ink narrower than the pen, so the point comes out slightly blunt with
a small rounded cap. Everything else matches the reference silhouette to within
a pen radius (median tolerant coverage 1.00, worst 0.95).

## Tracing data (`--traces`)

`core/src/tracing_data.rs` holds pen-stroke centerlines for the tracing game —
macroquad only rasterizes fonts, so stroke geometry has to be precomputed.
`--traces` re-runs that emitter **against the built font** using the
hand-authored pen routes in `routes.py`, and writes:

- `core/src/tracing_data.rs` — the baked data (commit it). Besides the glyph
  table it carries `UPEM`, `X_HEIGHT`, `ASCENT`, `DESCENT`, `PEN_WIDTH` and
  `SOURCE_FONT_FNV1A64` (the fingerprint of the TTF it was baked from — a
  staleness guard; re-run `--traces` whenever the font changes);
- `debug-sheet-lowercase.png` — the contact sheet to eyeball (each stroke
  dark→light along its direction, green ring at the start, red dot at the end,
  over the glyph fill);
- per-glyph `cover=` on stdout: the fraction of the glyph's skeleton the pen
  passes over. Below ~0.95 means part of the letter was skipped;
- per-glyph `!!` warnings when a glyph's mid-line reversal count differs from
  `EXPECTED_REVERSALS`: a ~180° turn on a degree-2 skeleton pixel with no wedge
  branch beyond it is usually a waypoint that snapped past a junction, sending
  the pen down the wrong branch and back (a visible jerk in the demo).

Warnings, the coverage floor, and "every baked point is inside the glyph's
ink" are **asserts** — the run fails rather than quietly baking a wrong letter.

The routes are authored for the Tasmanian Beginner's Alphabet: one continuous
stroke per lowercase letter, except `f`/`t`/`x` (two) and `i`/`j` (stroke +
dot). Digits and capitals are not routed yet.

### Authoring routes

Dijkstra over *pixels* (not a topology graph) is the trick that keeps this
simple: retraced segments (the a/d/m stems, where the pen passes the same ink
twice) are just the same pixels appearing in two shortest-path segments, and
loops (`o`) are forced around by intermediate waypoints.

1. **Read the glyph's strokes off the chart.** `tas-beginners-chart.png`
   (lowercase) and `tas-capitals-chart.png` (capitals) are the official
   stroke-order charts: numbered start points and arrows for direction. The
   markers are small at full-page zoom — crop 2–4 glyphs and upscale before
   looking:

   ```python
   from PIL import Image
   im = Image.open('tools/handwriting_font/tas-beginners-chart.png')
   c = im.crop((x0, y0, x1, y1))
   c.resize((c.width * 5, c.height * 5), Image.LANCZOS).save('/tmp/zoom.png')
   ```

2. **Read the glyph's skeleton**, so the waypoints are anchors rather than
   guesses. Endpoints and junctions in the *same* normalized frame as the
   route, plus every chain between them, is a dozen lines against `trace.py`
   (this is how the lowercase pass was authored — render at `TRACE_PPEM`,
   `components` → `skeletonize` → `prune_spurs(TRACE_SPUR_PX_AT_512)`, then
   print `skeleton_polylines(sk, merge=False)` ends and mid-points normalized
   to the ink bbox). A junction listed at (0.80, 0.39) *is* a waypoint.
3. **Add a `ROUTES` entry** in `routes.py`: one list of `(nx, ny)` waypoints per
   stroke, normalized to the glyph's **ink bbox** (x 0→1 left→right, y 0→1
   bottom→top, descenders included; a detached i/j-style dot is excluded from
   the bbox and written as the string `"dot"`). First waypoint = start, last =
   end; add intermediate waypoints wherever the shortest path could shortcut
   (loop direction, retraces, which side of a junction). A retrace is just the
   far end written twice: `… (0.79,0.38), (0.93,0.91), (0.74,0.05)` walks up
   the closing side of `a` and back down it.
4. **Run** `uv run tools/handwriting_font/build.py --traces-dry-run /tmp/t.rs
   --trace-debug /tmp/t.png` and read `cover=` + the warnings; only grow
   `EXPECTED_REVERSALS` for a genuine pen-touch reversal you have verified on
   the debug sheet — print the turn's position out of `process_stroke` if it is
   not obvious which turn is being counted.
5. **Then** run with `--traces` and the app gates (`cargo test --workspace`,
   `--playtest`, `tools/goldens.sh`): the core tests assert every baked stroke
   is traceable to completion, that the oval family starts at 2 o'clock, and
   that only f/t/x/i/j have two strokes; the goldens show the data overlaid on
   the real rendered font.

Topology gotchas the lowercase pass hit before (the debug sheet makes them
obvious):

- **Skeleton spurs** at sharp turns / tapered terminals are pruned. The router
  prunes at `TRACE_SPUR_PX_AT_512` (6 px), *not* the font build's 18: pruning a
  branch leaves the junction knot behind, so a pruned pen tip drags the start
  dot a spur-length inside the ink and the terminal ray-extension can't rescue
  it (it only fires on a degree-1 end). At 6 px the 2 o'clock tips of `a g q`
  and the corners of `z` survive; nothing else in a–z changes topology.
- **Crossings collapse**: a shallow ✕ (the `x`) becomes two junctions joined by
  a short shared bridge; both strokes legitimately reuse the bridge.
- **Mitred vertices are curves in the skeleton**: at the `v`/`w` points the
  medial axis rounds off inside the join and stops short of the point, and the
  boxcar smoothing rounds it further. `turns_and_vertices()` finds them with a
  wider baseline so they get pinned and ray-cast into the point — without that,
  `v` bottomed out 64 units (16% of x-height) above its own ink.
- **Waypoints snap to the nearest skeleton pixel**, so a sloppy coordinate can
  land on the wrong branch (the old `a` skipped its bowl because the "bottom"
  waypoint snapped to the stem). Nudge coordinates, don't add precision.
- **Snap overshoot past a junction**: a waypoint meant for "just after the bowl
  closes" that snaps a few pixels down the *next* branch makes the pen run out
  and double back. The mid-line-reversal check catches this; the debug sheet
  often hides it under other strokes.

## Files

| file | what |
| --- | --- |
| `build.py` | entry point: reference extraction, chart render, font build, proof sheet, `--traces` |
| `trace.py` | raster → skeleton → centerline machinery (ported from the retired `tools/trace_extract/`) |
| `routes.py` | `ROUTES` + `EXPECTED_REVERSALS` — stroke order/direction for the tracing game |
| `sheet.png` | proof sheet; **eyeball it after any change** |
| `debug-sheet-lowercase.png` | routed pen strokes per letter; **eyeball it after any route change** |
| `tas-*-chart.png` | official stroke-order charts (CC BY 4.0, © State of Tasmania DECYP) |
| `reference/` | gitignored: extracted vendor subsets, intermediate rasters |
