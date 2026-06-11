# trace_extract — stroke-order data for the tracing game

Bakes pen-stroke centerlines for VicModernCursive glyphs into
`core/src/tracing_data.rs`. macroquad only rasterizes fonts (no outline
access at runtime), so this offline step is where stroke geometry comes from.

## Run

```sh
pip install freetype-py numpy scikit-image scipy pillow
python3 tools/trace_extract/extract.py
```

Outputs:
- `core/src/tracing_data.rs` — the baked data (commit it).
- `/tmp/trace_debug.png` — a contact sheet of every routed glyph. **Eyeball
  this after any change** (see "Verifying" below). A snapshot of the current
  extraction is committed as `debug-sheet-lowercase.png`; refresh it when the
  routes change.
- Per-glyph `cover=` numbers on stdout (fraction of skeleton pixels the route
  passes through; < ~0.95 usually means part of a letter was skipped).

## How it works

Per glyph: FreeType render (no hinting) → threshold → skeletonize →
prune short spur branches (corner artifacts) → route the pen as
waypoint-guided Dijkstra over the skeleton pixels → extend the tips to the
visual stroke ends (a skeleton stops one stroke-radius short) → smooth +
resample → convert to font units (y up, origin at the pen position on the
baseline — the same frame `draw_text_ex(glyph, pen_x, baseline_y, ..)` uses,
so the app overlays the paths on the rendered glyph exactly).

Dijkstra over *pixels* (not a topology graph) is the trick that keeps this
simple: retraced segments (the a/d/m stems, where the pen passes the same ink
twice) are just the same pixels appearing in two shortest-path segments, and
loops ('o') are forced around by intermediate waypoints.

## Reading the stroke-order chart

`vmc-stroke-order-chart.png` (committed here) is the Victorian Modern Cursive
handwriting chart: lowercase, digits, and capitals. Conventions:
- **green dot** = stroke start, **red dot** = stroke end;
- **arrows** show direction along the path;
- **numbers** label strokes on multi-stroke glyphs (lowercase: f, t, x, plus
  the i/j dots; most digits/capitals have 2–4 strokes).

The markers are tiny at full-page zoom. To read them reliably (this is how
the lowercase routes were authored): crop a region of 2–4 glyphs and upscale
4–6× before looking, e.g.

```python
from PIL import Image
im = Image.open('tools/trace_extract/vmc-stroke-order-chart.png')
c = im.crop((x0, y0, x1, y1))            # a few letters at a time
c.resize((c.width * 5, c.height * 5), Image.LANCZOS).save('/tmp/zoom.png')
```

Row layout of the chart (y in px, full height ~955): lowercase a–m ≈ 0–170,
n–z ≈ 150–330, digits 0–9 ≈ 320–480, capitals A–I ≈ 470–640, J–R ≈ 630–800,
S–Z ≈ 790–955. When a red/green dot is ambiguous at 4×, crop that single
glyph at 6×.

## Adding glyphs (digits, capitals)

1. Read the glyph's strokes off the chart (above).
2. Add a `ROUTES` entry in `extract.py`: one list of `(nx, ny)` waypoints per
   stroke, normalized to the glyph's **ink bbox** (x 0→1 left→right, y 0→1
   bottom→top, descenders included; the i/j-style detached dot is excluded
   from the bbox and written as the string `"dot"`). First waypoint = start,
   last = end; add intermediate waypoints wherever the shortest path could
   shortcut (loop direction, retraces, which side of a junction).
3. Extend `LETTERS` / the glyph list, run the script, check `cover=`, and
   eyeball the debug sheet.
4. Capitals land in new `GLYPHS` entries automatically; the runtime needs a
   letter-set/`ORDER` decision before they're playable (see docs/IDEAS.md).

Topology gotchas the lowercase pass hit (the debug sheet makes them obvious):
- **Skeleton spurs** at sharp turns / tapered terminals are pruned
  (`SPUR_PX`); real flick tips are longer and survive. If a needed tip
  vanishes, it was pruned — lengthen the threshold check or route to it.
- **Crossings collapse**: a shallow ✕ (the 'x') becomes two junctions joined
  by a short shared bridge; both strokes legitimately reuse the bridge.
- **Waypoints snap to the nearest skeleton pixel**, so a sloppy coordinate
  can land on the wrong branch (the original 'a' skipped its bowl because
  the "bottom" waypoint snapped to the stem). Nudge coordinates, don't add
  precision.

## Verifying

On the debug sheet each stroke is drawn dark→light along its direction, with
a green circle at the start and a red dot at the end, over the glyph fill.
Check against the chart: start position, direction, end position, stroke
count/order — and that no ink is left uncovered (light blue with no line
through it). Then run the full app gates (`cargo test --workspace`,
`--playtest`, `tools/goldens.sh`): core tests assert every baked stroke is
traceable to completion, and the goldens show the data overlaid on the real
rendered font.
