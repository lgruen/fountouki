# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "pypdf==6.4.0",
#   "pymupdf==1.27.2",
#   "fonttools==4.63.0",
#   "freetype-py==2.5.1",
#   "numpy==2.3.5",
#   "scipy==1.16.3",
#   "scikit-image==0.25.2",
#   "shapely==2.1.2",
#   "pillow==11.3.0",
# ]
# ///
"""Author `app/assets/fonts/handwriting.ttf` in the Tasmanian Basic Handwriting
Style, centerline-first.

    uv run tools/handwriting_font/build.py

Nothing from the source PDFs is redistributed: the vendor font programs are
read locally (see README.md), reduced to *centerlines* — geometry we then
re-expand with our own pen and re-fit with our own curves — and only that
output is committed. Extracted vendor bytes live in `reference/`, which is
gitignored.

Pipeline per glyph:
  reference outline -> raster -> skeletonize -> branch-walk polylines ->
  smooth/resample -> normalize to upem 1000 -> round-pen stroke expansion ->
  cubic Bezier fit -> cu2qu -> glyf.

`--traces` re-runs the tracing-game emitter (hand-authored pen routes in
routes.py) against the *built* font and writes core/src/tracing_data.rs;
`--traces-dry-run PATH` does the same to a scratch path.
"""

from __future__ import annotations

import argparse
import math
import os
import sys
import time

import numpy as np
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.misc.timeTools import timestampSinceEpoch
from fontTools.ttLib.tables._g_l_y_f import GlyphCoordinates
from PIL import Image, ImageDraw, ImageFont
from shapely import affinity
from shapely.geometry import LineString, Point, Polygon
from shapely.geometry.polygon import orient
from shapely.ops import unary_union

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import routes  # noqa: E402
import trace  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
REF_DIR = os.path.join(HERE, "reference")
OUT_TTF = os.path.join(REPO, "app/assets/fonts/handwriting.ttf")
SHEET_PNG = os.path.join(HERE, "sheet.png")
TRACES_RS = os.path.join(REPO, "core/src/tracing_data.rs")
TRACE_DEBUG_PNG = os.path.join(HERE, "debug-sheet-lowercase.png")
DEFAULT_PDF_DIR = os.path.expanduser("~/Downloads/tas-handwriting")

PDFS = {
    "guidelines": "Handwriting.pdf",
    "beginners_chart": "Handwriting-Beginners-Alphabet-chart.pdf",
    "capitals_chart": "Handwriting-Capital-Alphabet-chart.pdf",
}
CHART_PNGS = {
    "beginners_chart": os.path.join(HERE, "tas-beginners-chart.png"),
    "capitals_chart": os.path.join(HERE, "tas-capitals-chart.png"),
}
CHART_DPI = 200

# Which embedded subset supplies what.
REF_FILES = {
    "regular": ("TasBegRegular", "guidelines", "tas-beg-regular.ttf"),
    "regnum": ("TasBegRegNum", "guidelines", "tas-beg-regnum.ttf"),
    "bold": ("TasBegBold", "guidelines", "tas-beg-bold.ttf"),
}

# ---------------------------------------------------------------------------
# The one place the design space is pinned. Everything downstream derives.
# ---------------------------------------------------------------------------
UPEM = 1000
X_HEIGHT = 400.0          # target x-height ink top; sets the master scale k
DIGIT_SIDE_BEARING = 90.0  # digits/!/? are set on a tight, uniform sidebearing


def digit_bearing(ch: str) -> float:
    """Per-glyph digit sidebearing. The official '1' is a bare stem (no flag,
    no base), so at the uniform bearing a pair like "11" reads as two tally
    marks; halving its bearing pulls 10/11/12/21 into one visual number
    without touching the letterform itself."""
    return 45.0 if ch == "1" else DIGIT_SIDE_BEARING
SPACE_ADVANCE = 300
FAMILY = "Fountouki Handwriting"
SUBFAMILY = "Regular"
VERSION = "1.000"
ATTRIBUTION = (
    "Letterforms follow the Tasmanian 'Basic Handwriting Style', Tasmanian "
    "Handwriting Guidelines (2023), © State of Tasmania (Department for "
    "Education, Children and Young People), CC BY 4.0. Font authored for the "
    "fountouki project."
)
CC_BY_URL = "https://creativecommons.org/licenses/by/4.0/"
# Deterministic head.created/modified (the font fingerprint is a staleness
# guard for the baked tracing data, so the build has to be reproducible).
FIXED_TIMESTAMP = timestampSinceEpoch(1750000000)

LOWER = "abcdefghijklmnopqrstuvwxyz"
UPPER = LOWER.upper()
DIGITS = "0123456789"
PUNCT = ".,"
AUTHORED = "!?"
COVERAGE = LOWER + UPPER + DIGITS + PUNCT + AUTHORED

# Raster / centerline tuning.
PPEM = 1024               # render size for the font build
TRACE_PPEM = 512          # render size for the tracing emitter (as before)
CORNER_LOOKAHEAD = 8      # px, centerline V-turn detection at PPEM 1024
TRIM_NARROW = 0.80        # trim free centerline ends out of sub-pen-wide ink
CORNER_COS = math.cos(math.radians(50.0))
BUF_RES = 12              # shapely buffer segments per quarter circle
MITRE_LIMIT = 8.0         # cap the spike a very sharp centerline corner makes
FIT_TOL = 1.5             # cubic fit tolerance, font units
CU2QU_TOL = 0.5           # cubic -> quadratic tolerance, font units
RING_CORNER_COS = math.cos(math.radians(50.0))
# Overlay stripping. Measured on the TasBegRegNum subset (upem 3000): the
# smallest *real* contour is 'e's counter at 97424 units^2 (0.0108 em^2) and
# the largest baked-in arrow is 23379 (0.0026 em^2) — a 4x band. The i/j dots
# fall below both (~8000) but are near-circular, which nothing in the arrow
# overlay is (arrows/labels/rings top out at compactness 0.38).
OVERLAY_AREA_EM2 = 0.0055  # keep contours at least this big (fraction of em^2)
OVERLAY_COMPACT = 0.80     # ... or this round (4*pi*A/P^2), which saves i/j dots


def log(msg):
    print(msg, flush=True)


# ---------------------------------------------------------------------------
# 0. Source material: extract the embedded subsets, render the charts.
# ---------------------------------------------------------------------------

def extract_reference(pdf_dir):
    """Pull the three TasBeg* TrueType subsets out of the local PDFs into
    reference/ (gitignored). Cached: re-run is a no-op."""
    os.makedirs(REF_DIR, exist_ok=True)
    out = {}
    missing = [key for key, (_, _, fn) in REF_FILES.items()
               if not os.path.exists(os.path.join(REF_DIR, fn))]
    if missing:
        from pypdf import PdfReader
        readers = {}
        for key, (base, pdf_key, fn) in REF_FILES.items():
            if key not in missing:
                continue
            path = os.path.join(pdf_dir, PDFS[pdf_key])
            if not os.path.exists(path):
                raise SystemExit(
                    f"missing source PDF {path}\n"
                    "Download the three Tasmanian handwriting PDFs (see README.md) "
                    "and pass --pdf-dir.")
            if pdf_key not in readers:
                readers[pdf_key] = PdfReader(path)
            blob = _find_fontfile(readers[pdf_key], base)
            if blob is None:
                raise SystemExit(f"no /FontFile2 for {base} in {path}")
            with open(os.path.join(REF_DIR, fn), "wb") as f:
                f.write(blob)
            log(f"  extracted {base} -> reference/{fn} ({len(blob)} bytes)")
    for key, (_, _, fn) in REF_FILES.items():
        with open(os.path.join(REF_DIR, fn), "rb") as f:
            out[key] = f.read()
    return out


def _find_fontfile(reader, base_substr):
    """Largest /FontFile2 whose /BaseFont contains `base_substr` (the biggest
    subset is the one with the most glyphs)."""
    best = None
    for page in reader.pages:
        res = page.get("/Resources")
        if res is None:
            continue
        fonts = res.get_object().get("/Font")
        if fonts is None:
            continue
        for _, fo in fonts.get_object().items():
            fo = fo.get_object()
            if base_substr not in str(fo.get("/BaseFont")):
                continue
            fd = fo.get("/FontDescriptor")
            if fd is None:
                continue
            ff = fd.get_object().get("/FontFile2")
            if ff is None:
                continue
            data = ff.get_object().get_data()
            if best is None or len(data) > len(best):
                best = data
    return best


def render_charts(pdf_dir):
    """Page 1 of each alphabet chart -> PNG (CC BY page content; committed as
    the human stroke-order reference)."""
    import pymupdf
    for key, png in CHART_PNGS.items():
        path = os.path.join(pdf_dir, PDFS[key])
        if not os.path.exists(path):
            log(f"  ! {PDFS[key]} not found — keeping existing {os.path.basename(png)}")
            continue
        doc = pymupdf.open(path)
        pix = doc[0].get_pixmap(dpi=CHART_DPI)
        pix.save(png)
        doc.close()
        log(f"  chart {os.path.basename(png)} {pix.width}x{pix.height}")


# ---------------------------------------------------------------------------
# 1. Reference selection: strip the baked-in stroke-order arrows from RegNum.
# ---------------------------------------------------------------------------

def _contour_stats(coords, end_pts):
    stats = []
    s = 0
    for e in end_pts:
        pts = np.asarray(coords[s:e + 1], dtype=float)
        s = e + 1
        n = len(pts)
        if n < 3:
            stats.append((0.0, 0.0))
            continue
        nxt = np.roll(pts, -1, axis=0)
        area = 0.5 * float(np.sum(pts[:, 0] * nxt[:, 1] - nxt[:, 0] * pts[:, 1]))
        per = float(np.sum(np.hypot(*(nxt - pts).T)))
        compact = 4 * math.pi * abs(area) / per ** 2 if per > 0 else 0.0
        stats.append((area, compact))
    return stats


def strip_overlays(font_bytes, chars):
    """Drop the stroke-order arrow contours baked into TasBegRegNum outlines.

    A contour goes if it is smaller than OVERLAY_AREA_EM2 *and* is not
    near-circular. The compactness escape is what saves the i/j dots, which are
    small enough in absolute area to look exactly like an arrowhead but are
    discs (compactness ~0.97 vs <=0.38 for anything in the overlay).

    hmtx has to follow: FreeType positions an unhinted glyph by translating the
    outline by (lsb - xMin), so leaving the arrow-inflated lsb behind would
    shift every stripped glyph sideways.
    """
    font = TTFont(_bytesio(font_bytes))
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    upem = font["head"].unitsPerEm
    area_min = OVERLAY_AREA_EM2 * upem * upem
    cmap = font.getBestCmap()
    kept = {}
    for ch in chars:
        gname = cmap.get(ord(ch))
        if gname is None:
            continue
        g = glyf[gname]
        if g.numberOfContours <= 0:
            continue
        stats = _contour_stats(g.coordinates, g.endPtsOfContours)
        keep = [i for i, (a, comp) in enumerate(stats)
                if abs(a) >= area_min or comp >= OVERLAY_COMPACT]
        kept[ch] = (len(keep), len(stats))
        if len(keep) == len(stats):
            continue
        coords, flags, ends = [], [], []
        s = 0
        for i, e in enumerate(g.endPtsOfContours):
            if i in keep:
                coords.extend(list(g.coordinates[s:e + 1]))
                flags.extend(list(g.flags[s:e + 1]))
                ends.append(len(coords) - 1)
            s = e + 1
        g.coordinates = GlyphCoordinates(coords)
        g.flags = bytearray(flags)
        g.endPtsOfContours = ends
        g.numberOfContours = len(ends)
        g.recalcBounds(glyf)
        hmtx[gname] = (hmtx[gname][0], g.xMin)
    buf = _bytesio(b"")
    font.save(buf)
    return buf.getvalue(), kept


def _bytesio(data):
    import io
    return io.BytesIO(data)


def contour_count(font_bytes, ch):
    font = TTFont(_bytesio(font_bytes))
    gname = font.getBestCmap().get(ord(ch))
    if gname is None:
        return 0
    return max(0, font["glyf"][gname].numberOfContours)


def ref_advances(font_bytes, chars):
    font = TTFont(_bytesio(font_bytes))
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    return {ch: hmtx[cmap[ord(ch)]][0] for ch in chars if ord(ch) in cmap}


def ref_ymax(font_bytes, ch):
    font = TTFont(_bytesio(font_bytes))
    g = font["glyf"][font.getBestCmap()[ord(ch)]]
    return g.yMax


def ref_upem(font_bytes):
    return TTFont(_bytesio(font_bytes))["head"].unitsPerEm


def silhouette_match(ma, mb, tol):
    """How well two rendered glyph masks agree, as (IoU, tolerant coverage).

    The masks are aligned on their ink centroids — the two subsets place and
    space their glyphs differently, so origins are not comparable — then the
    best of a +-4px shift search is taken. Plain IoU is a harsh metric for a
    hairline font (the strokes are ~8px wide at this ppem, so a 2px design
    drift halves the overlap), so the *assert* runs on coverage: the fraction
    of each mask's ink lying within `tol` px of the other's. A surviving arrow
    contour is ink far from the reference and tanks coverage; a slightly
    different exit flick does not."""
    def crop(m):
        rr, cc = np.where(m)
        return m[rr.min():rr.max() + 1, cc.min():cc.max() + 1]

    A, B = crop(ma), crop(mb)
    pad = 10
    h = max(A.shape[0], B.shape[0]) + 2 * pad
    w = max(A.shape[1], B.shape[1]) + 2 * pad
    P = np.zeros((h, w), bool)
    P[(h - A.shape[0]) // 2:(h - A.shape[0]) // 2 + A.shape[0],
      (w - A.shape[1]) // 2:(w - A.shape[1]) // 2 + A.shape[1]] = A
    oy, ox = (h - B.shape[0]) // 2, (w - B.shape[1]) // 2
    best, best_q = 0.0, None
    for dy in range(-5, 6):
        for dx in range(-5, 6):
            y, x = oy + dy, ox + dx
            if y < 0 or x < 0 or y + B.shape[0] > h or x + B.shape[1] > w:
                continue
            Q = np.zeros((h, w), bool)
            Q[y:y + B.shape[0], x:x + B.shape[1]] = B
            u = (P | Q).sum()
            iou = float((P & Q).sum() / u) if u else 0.0
            if iou > best:
                best, best_q = iou, Q
    if best_q is None:
        return 0.0, 0.0
    dp = trace.distance_transform_edt(~P)
    dq = trace.distance_transform_edt(~best_q)
    cov = min(float((dp[best_q] <= tol).mean()), float((dq[P] <= tol).mean()))
    return best, cov


# ---------------------------------------------------------------------------
# 2-4. Raster -> skeleton -> centerline polylines.
# ---------------------------------------------------------------------------

class Cfg:
    def __init__(self, upem_ref, ppem, k):
        self.ppem = ppem
        self.k = k
        self.px2u = upem_ref / ppem * k          # raster px -> target units
        self.spur_px = trace.SPUR_PX_AT_512 * ppem / 512.0
        self.smooth_win = int(round(trace.SMOOTH_WIN_AT_512 * ppem / 512.0)) | 1
        self.resample = trace.RESAMPLE_AT_3000 * UPEM / 3000.0


def analyze(face, ch, cfg):
    """Skeleton pieces (raster pixels) + dot components for one glyph."""
    mask, left, top, adv_px = trace.render(face, ch)
    # pad before the distance transform: the bitmap is the ink bbox, so ink
    # touches the array edge and an unpadded EDT reports a stroke tip that runs
    # off the top (the A/N apex) as if it were metres from any background.
    dt = trace.distance_transform_edt(np.pad(mask, 2))[2:-2, 2:-2].astype(np.float32)
    pieces, dots = [], []
    for _area, comp in trace.components(mask):
        sk = trace.skeletonize(comp)
        r_max = float(dt[comp].max())
        if int(sk.sum()) <= max(4.0, 2.5 * r_max):
            rr, cc = np.where(comp)
            dots.append(((float(rr.mean()), float(cc.mean())), r_max))
            continue
        skp = trace.prune_spurs(sk, cfg.spur_px)
        deg = trace.degree_map(skp)
        for path in trace.skeleton_polylines(skp):
            # Split at V-turns so smoothing (which pins piece ends) cannot
            # round the corner off; the pieces are stitched back together
            # after smoothing so the pen still mitres through the vertex.
            turns = trace.sharp_turns(path, CORNER_LOOKAHEAD, CORNER_COS,
                                      2 * CORNER_LOOKAHEAD)
            bounds = [0] + turns + [len(path) - 1]
            subs = [path[a:b + 1] for a, b in zip(bounds, bounds[1:]) if b > a]
            if subs:
                pieces.append((subs, deg))
    return dict(ch=ch, mask=mask, left=left, top=top, adv_px=adv_px, dt=dt,
                pieces=pieces, dots=dots)


def pen_samples(res):
    """2 x distance transform along the centerline interiors (raster px)."""
    out = []
    for subs, _deg in res["pieces"]:
        for path in subs:
            if len(path) < 6:
                continue
            for r, c in path[2:-2]:
                out.append(2.0 * float(res["dt"][r, c]))
    return out


def finish(res, cfg, pen_px):
    """Extend true tips to the ink boundary, smooth, resample, to font units."""
    left, top = res["left"], res["top"]
    scale = cfg.px2u

    def to_units(p):
        r, c = p
        return ((left + c + 0.5) * scale, (top - r - 0.5) * scale)

    strokes = []
    for subs, deg in res["pieces"]:
        subs = [list(s) for s in subs]
        # this stroke's own half-width, not the font-wide pen: a t/f crossbar
        # is drawn thinner than a stem and must not read as "narrow ink"
        flat = [p for s in subs for p in s]
        core = flat[len(flat) // 6:-len(flat) // 6 or None] or flat
        r_ref = float(np.median([res["dt"][p] for p in core]))
        subs[0] = _terminal(subs[0], deg, res, pen_px, r_ref, at_start=True)
        subs[-1] = _terminal(subs[-1], deg, res, pen_px, r_ref, at_start=False)
        subs = [s for s in subs if len(s) >= 2]
        if not subs:
            continue
        out = []
        for s in subs:
            pts = trace.smooth_resample(s, to_units, cfg.resample, cfg.smooth_win)
            out.extend(pts[1:] if out else pts)
        strokes.append(out)
    dots = [(to_units(rc), r * scale) for rc, r in res["dots"]]
    return strokes, dots, res["adv_px"] * scale


def _terminal(pts, deg, res, pen_px, r_ref, at_start):
    """Fix up a free end of a centerline: trim, then extend.

    *Trim* back out of ink narrower than this stroke's own width (`r_ref`). The
    medial axis of a sharp wedge (the A/N/M apex, a tapered terminal) keeps
    going after the ink has become thinner than the stroke, and a round pen
    laid on that part paints a fat stub sticking out of the letter.

    *Extend* what is left along the ink — but only when nothing was trimmed: a
    skeleton stops about one pen radius short of a stroke end, so push the tip
    out until the round cap lands on the reference ink boundary. A trimmed end
    is deliberately never re-extended (see the comment below), which keeps the
    A/N/M apexes from growing stubs but also swallows a long gradual exit
    taper whole — a casualty gets an explicit TERMINAL_EXTEND repair instead."""
    seq = list(pts) if at_start else list(pts)[::-1]
    p = seq[0]
    if not (isinstance(p[0], (int, np.integer)) and deg[p] == 1):
        return list(pts)
    dt, mask = res["dt"], res["mask"]
    thr = TRIM_NARROW * r_ref
    cut, lim = 0, int(3 * pen_px)
    while cut < len(seq) - 2 and cut < lim and dt[seq[cut]] < thr:
        cut += 1
    seq = seq[cut:]
    p = seq[0]
    back = seq[min(8, len(seq) - 1)]
    d = trace.unit_vec((p[0] - back[0], p[1] - back[1]))
    # Extending is only right where the skeleton stopped short of a full-width
    # stroke end. If we just trimmed, the ink ahead is the sub-pen-wide part we
    # deliberately backed out of, and pushing forward would put it straight
    # back — which is what made an A apex grow a stub in the first place.
    if not cut and d != (0.0, 0.0):
        ext = trace.ink_run(p, d, mask, 3.0 * pen_px) - pen_px / 2.0
        if ext > 0.5:
            seq = [(p[0] + d[0] * ext, p[1] + d[1] * ext)] + seq
    return seq if at_start else seq[::-1]


# Explicit terminal repairs, in final font units (applied after the built-x
# rescale). `_terminal` trims a free end back out of sub-pen-width ink and
# never re-extends a trimmed end — right for the A/N/M apexes, but a long
# gradual exit taper is swallowed whole by that trim (the '2' base lost 109
# units of centerline). Each entry re-extends the glyph's *lowest* free
# stroke end along its own end tangent, calibrated against the reference:
# walk a corridor (half-width = the stroke's own half-width) along the end
# direction to the taper ink's farthest extent, land the cap edge there
# (extent - pen/2). On the '2' this matches the best-shift silhouette
# overlay's zero-mismatch point exactly. An audit of every free end found
# these five; the other ~80 trims are apexes / angled cuts within a few
# units (A/M/V apexes and the ',' tail must stay blunt — re-extending them
# regrows the stubs the trim exists to prevent). The coverage assert at the
# end of the build is the regression guard.
TERMINAL_EXTEND = {"2": 108.0, "d": 50.0, "e": 36.0, "l": 27.0, "q": 26.0,
                   "c": 7.0}


def repair_terminals(strokes, ext):
    """Extend the lowest stroke end (start or tip) of one glyph by `ext` units
    along the local end direction. `strokes` is mutated in place."""
    ends = [(s[i][1], si, i) for si, s in enumerate(strokes) if len(s) >= 2
            for i in (0, -1)]
    _y, si, i = min(ends)
    s = strokes[si]
    a = s[i]
    b = s[min(8, len(s) - 1)] if i == 0 else s[max(-9, -len(s))]
    d = trace.unit_vec((a[0] - b[0], a[1] - b[1]))
    p = (a[0] + d[0] * ext, a[1] + d[1] * ext)
    s.insert(0, p) if i == 0 else s.append(p)


# ---------------------------------------------------------------------------
# 7. Stroke expansion.
# ---------------------------------------------------------------------------

def expand(strokes, dots, pen):
    geoms = []
    for pts in strokes:
        pts = [p for i, p in enumerate(pts)
               if i == 0 or math.dist(p, pts[i - 1]) > 1e-9]
        if len(pts) < 2:
            if pts:
                geoms.append(Point(pts[0]).buffer(pen / 2, resolution=BUF_RES))
            continue
        # round caps at the terminals, mitred joins through the corners: a
        # round join would blunt every V/W/X/Z vertex by half a pen width.
        geoms.append(LineString(pts).buffer(pen / 2, cap_style=1, join_style=2,
                                            mitre_limit=MITRE_LIMIT,
                                            resolution=BUF_RES))
    for (cx, cy), r in dots:
        geoms.append(Point(cx, cy).buffer(max(r, pen * 0.4), resolution=BUF_RES))
    if not geoms:
        return None
    poly = unary_union(geoms)
    return poly


def polygons(poly, pen):
    polys = [poly] if isinstance(poly, Polygon) else list(poly.geoms)
    out = []
    hole_min = 1.5 * pen * pen  # smallest real counter (e) is ~4x this
    for p in polys:
        interiors = [r for r in p.interiors if Polygon(r).area >= hole_min]
        p = Polygon(p.exterior, interiors)
        out.append(orient(p, sign=-1.0))  # TrueType: outer clockwise
    out.sort(key=lambda p: -p.area)
    return out


# ---------------------------------------------------------------------------
# 8. Outline fit: corner split + least-squares cubics (Schneider) -> cu2qu.
# ---------------------------------------------------------------------------

def _dedupe(pts, eps=1e-6):
    out = []
    for p in pts:
        if not out or math.dist(p, out[-1]) > eps:
            out.append(p)
    return out


def corner_indices(P, win, cos_thr):
    """Corners of a closed ring, from tangents measured over an arc-length
    window. `win` must stay well under the pen radius or the round caps and
    joins (radius pen/2) register as corners."""
    n = len(P)
    seg = np.hypot(*(np.roll(P, -1, axis=0) - P).T)

    def walk(i, step):
        acc = 0.0
        j = i
        for _ in range(n // 2):
            if acc >= win:
                break
            acc += seg[j % n] if step > 0 else seg[(j - 1) % n]
            j += step
        return P[j % n]

    cos = np.ones(n)
    for i in range(n):
        v1 = P[i] - walk(i, -1)
        v2 = walk(i, 1) - P[i]
        n1, n2 = math.hypot(*v1), math.hypot(*v2)
        if n1 < 1e-9 or n2 < 1e-9:
            continue
        cos[i] = float(np.dot(v1, v2) / (n1 * n2))
    out = []
    for i in sorted((i for i in range(n) if cos[i] < cos_thr), key=lambda i: cos[i]):
        if all(min(abs(i - j), n - abs(i - j)) > 3 for j in out):
            out.append(i)
    return sorted(out)


def _bez(c, t):
    mt = 1.0 - t
    return (mt ** 3 * c[0] + 3 * mt * mt * t * c[1]
            + 3 * mt * t * t * c[2] + t ** 3 * c[3])


def _bez_d(c, t):
    mt = 1.0 - t
    return 3 * mt * mt * (c[1] - c[0]) + 6 * mt * t * (c[2] - c[1]) + 3 * t * t * (c[3] - c[2])


def _bez_dd(c, t):
    mt = 1.0 - t
    return 6 * mt * (c[2] - 2 * c[1] + c[0]) + 6 * t * (c[3] - 2 * c[2] + c[1])


def _chord_params(P):
    d = np.hypot(*np.diff(P, axis=0).T)
    u = np.concatenate([[0.0], np.cumsum(d)])
    return u / u[-1] if u[-1] > 0 else u


def _generate(P, u, t1, t2):
    """Least-squares cubic with fixed endpoints and tangent directions."""
    p0, p3 = P[0], P[-1]
    mt = 1.0 - u
    b0 = mt ** 3
    b1 = 3 * mt ** 2 * u
    b2 = 3 * mt * u ** 2
    b3 = u ** 3
    a1 = b1[:, None] * t1
    a2 = b2[:, None] * t2
    tmp = P - (b0 + b1)[:, None] * p0 - (b2 + b3)[:, None] * p3
    c00 = float(np.sum(a1 * a1))
    c01 = float(np.sum(a1 * a2))
    c11 = float(np.sum(a2 * a2))
    x0 = float(np.sum(a1 * tmp))
    x1 = float(np.sum(a2 * tmp))
    det = c00 * c11 - c01 * c01
    seglen = float(np.hypot(*(p3 - p0)))
    if abs(det) < 1e-12:
        alpha0 = alpha1 = seglen / 3.0
    else:
        alpha0 = (x0 * c11 - x1 * c01) / det
        alpha1 = (c00 * x1 - c01 * x0) / det
    cap = max(seglen * 2.0, 1e-6)
    if not (1e-6 < alpha0 < cap) or not (1e-6 < alpha1 < cap):
        alpha0 = alpha1 = seglen / 3.0
    return np.array([p0, p0 + alpha0 * t1, p3 + alpha1 * t2, p3])


def _max_err(P, u, c):
    B = np.array([_bez(c, t) for t in u])
    d = np.hypot(*(B - P).T)
    i = int(np.argmax(d))
    return float(d[i]), i


def _reparam(P, u, c):
    out = u.copy()
    for i, t in enumerate(u):
        d = _bez(c, t) - P[i]
        d1 = _bez_d(c, t)
        d2 = _bez_dd(c, t)
        num = float(np.dot(d, d1))
        den = float(np.dot(d1, d1) + np.dot(d, d2))
        if abs(den) > 1e-12:
            out[i] = t - num / den
    return np.clip(out, 0.0, 1.0)


def _tangent(P, i, j):
    v = P[j] - P[i]
    n = math.hypot(*v)
    return v / n if n > 1e-12 else np.array([1.0, 0.0])


def fit_cubics(P, tol, t1=None, t2=None, depth=0):
    """Schneider curve fit: chord parameterization, Newton reparameterization,
    recursive split at the worst point."""
    n = len(P)
    if n < 2:
        return []
    if t1 is None:
        t1 = _tangent(P, 0, min(3, n - 1))
    if t2 is None:
        t2 = _tangent(P, n - 1, max(0, n - 4))
    if n == 2 or depth > 18:
        d = P[-1] - P[0]
        return [np.array([P[0], P[0] + d / 3.0, P[0] + 2 * d / 3.0, P[-1]])]
    u = _chord_params(P)
    c = _generate(P, u, t1, t2)
    err, idx = _max_err(P, u, c)
    if err <= tol:
        return [c]
    if err <= tol * 16:
        for _ in range(6):
            u = _reparam(P, u, c)
            c = _generate(P, u, t1, t2)
            err, idx = _max_err(P, u, c)
            if err <= tol:
                return [c]
    idx = min(max(idx, 1), n - 2)
    center = _tangent(P, idx + 1, idx - 1)
    return (fit_cubics(P[:idx + 1], tol, t1, center, depth + 1)
            + fit_cubics(P[idx:], tol, -center, t2, depth + 1))


def fit_ring(ring, pen):
    pts = _dedupe(list(ring.coords))
    if len(pts) > 1 and math.dist(pts[0], pts[-1]) < 1e-6:
        pts = pts[:-1]
    if len(pts) < 4:
        return []
    P = np.asarray(pts, dtype=float)
    n = len(P)
    win = max(3.0, pen * 0.14)
    idx = corner_indices(P, win, RING_CORNER_COS)
    if len(idx) < 2:
        idx = sorted(set(idx) | {0, n // 2})
    beziers = []
    for a, b in zip(idx, idx[1:] + [idx[0] + n]):
        seg = np.array([P[i % n] for i in range(a, b + 1)])
        beziers.extend(fit_cubics(seg, FIT_TOL))
    return beziers


def glyph_from_polys(polys, pen):
    tt = TTGlyphPen(None)
    qpen = Cu2QuPen(tt, CU2QU_TOL)
    for p in polys:
        for ring in [p.exterior] + list(p.interiors):
            bez = fit_ring(ring, pen)
            if not bez:
                continue
            qpen.moveTo(tuple(bez[0][0]))
            for c in bez:
                qpen.curveTo(tuple(c[1]), tuple(c[2]), tuple(c[3]))
            qpen.closePath()
    return tt.glyph()


# ---------------------------------------------------------------------------
# 9. Authored glyphs (`!` and `?` have no clean reference).
# ---------------------------------------------------------------------------

def _arc(cx, cy, rx, ry, a0, a1, n=48):
    return [(cx + rx * math.cos(math.radians(a)), cy + ry * math.sin(math.radians(a)))
            for a in np.linspace(a0, a1, n)]


def _shear(pts, slope_deg):
    t = math.tan(math.radians(slope_deg))
    return [(x + t * y, y) for x, y in pts]


def authored_glyphs(cap, pen, slope_deg, dot_radius):
    """Monoline `!` and `?` in the same pen, sheared to the measured slope.
    Height class = cap height; the dot sits at the same height as `.`."""
    dot_y = dot_radius
    gap = max(pen * 1.6, 90.0)
    out = {}

    stem = [(0.0, cap), (0.0, dot_y + gap)]
    out["!"] = (_shear(stem, slope_deg), [_shear([(0.0, dot_y)], slope_deg)[0]])

    r = cap * 0.21
    cx, cy = r + pen * 0.5, cap - r * 1.05
    bowl = _arc(cx, cy, r, r * 1.10, 196.0, -28.0, 56)
    ex, ey = bowl[-1]
    tail_end = (cx, dot_y + gap)
    ctrl = (ex * 0.72 + cx * 0.28, (ey + tail_end[1]) * 0.5)
    tail = [((1 - t) ** 2 * ex + 2 * (1 - t) * t * ctrl[0] + t * t * tail_end[0],
             (1 - t) ** 2 * ey + 2 * (1 - t) * t * ctrl[1] + t * t * tail_end[1])
            for t in np.linspace(0.0, 1.0, 24)]
    out["?"] = (_shear(bowl + tail[1:], slope_deg), [_shear([(cx, dot_y)], slope_deg)[0]])
    return out


# ---------------------------------------------------------------------------
# 12. Proof sheet + fingerprint.
# ---------------------------------------------------------------------------

def fnv1a64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def render_text(face, text, ppem):
    """Rasterize a string with the font's own advances -> (image, baseline_y)."""
    face.set_pixel_sizes(0, ppem)
    import freetype
    cells, pen = [], 0.0
    for ch in text:
        face.load_char(ch, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_NO_HINTING)
        g = face.glyph
        bm = g.bitmap
        buf = np.array(bm.buffer, dtype=np.uint8).reshape(bm.rows, bm.width) \
            if bm.rows and bm.width else None
        cells.append((pen + g.bitmap_left, g.bitmap_top, bm.rows, bm.width, buf))
        pen += g.advance.x / 64.0
    up = max([c[1] for c in cells if c[4] is not None] + [1])
    down = max([c[2] - c[1] for c in cells if c[4] is not None] + [1])
    w = int(math.ceil(max(pen, max((c[0] + c[3]) for c in cells)))) + 4
    img = np.zeros((up + down + 4, w), np.uint8)
    base = up + 2
    for x, t, rows, wid, buf in cells:
        if buf is None:
            continue
        x0 = int(round(x)) + 2
        y0 = base - t
        img[y0:y0 + rows, x0:x0 + wid] = np.maximum(img[y0:y0 + rows, x0:x0 + wid], buf)
    return img, base


def write_sheet(path, ttf_path, metrics):
    face = trace.load_face(ttf_path, 96)
    rows = [
        ("a-z", LOWER, 110),
        ("A-Z", UPPER, 110),
        ("0-9", DIGITS, 110),
        ("punct", "?!.,", 110),
        ("word", "yay!", 130),
        ("numerals", "10 11 12 20", 130),
        ("sentence", "the quick brown fox jumps over the lazy dog", 76),
    ]
    imgs = []
    for label, text, ppem in rows:
        img, base = render_text(face, text, ppem)
        imgs.append((label, img, base, ppem))
    pad, lab_w = 26, 110
    width = max(i.shape[1] for _, i, _, _ in imgs) + 2 * pad + lab_w
    height = sum(i.shape[0] + pad for _, i, _, _ in imgs) + pad + 40
    sheet = Image.new("RGB", (width, height), (255, 255, 255))
    d = ImageDraw.Draw(sheet)
    try:
        fnt = ImageFont.load_default(size=18)
        small = ImageFont.load_default(size=14)
    except TypeError:  # very old Pillow
        fnt = small = ImageFont.load_default()
    d.text((pad, 10), f"{FAMILY} {VERSION} - upem {UPEM}, x-height "
                      f"{metrics['x_height']:.0f}, cap {metrics['cap']:.0f}, "
                      f"asc {metrics['ascender']:.0f}, desc {metrics['descender']:.0f}, "
                      f"pen {metrics['pen']:.1f}, slope {metrics['slope']:.1f}deg",
           fill=(20, 20, 20), font=small)
    y = 44
    for label, img, base, ppem in imgs:
        h, w = img.shape
        # guide lines from the font metrics, at this ppem
        s = ppem / UPEM
        by = y + base
        for val, col in ((0.0, (210, 210, 210)),
                         (metrics["x_height"], (225, 190, 190)),
                         (metrics["cap"], (190, 205, 230))):
            gy = int(round(by - val * s))
            if y <= gy < y + h:
                d.line([(pad + lab_w, gy), (pad + lab_w + w, gy)], fill=col, width=1)
        rgb = np.stack([255 - img] * 3, axis=-1)
        tile = Image.fromarray(rgb.astype(np.uint8))
        sheet.paste(Image.composite(tile, sheet.crop((pad + lab_w, y,
                                                      pad + lab_w + w, y + h)),
                                    Image.fromarray(img)),
                    (pad + lab_w, y))
        d.text((pad, y + h // 2 - 8), label, fill=(120, 120, 120), font=fnt)
        y += h + pad
    sheet.save(path)
    return sheet.size


# ---------------------------------------------------------------------------
# Tracing-data emitter: the hand-authored pen routes (routes.py) walked over
# the *built* font's skeleton -> core/src/tracing_data.rs + a debug sheet.
# ---------------------------------------------------------------------------

def emit_traces(out_path, ttf_path, fingerprint, pen_width):
    """Route + emit the pen strokes over `ttf_path`'s raster. Writes
    tracing_data to `out_path`, or only computes (returning the stroke
    geometry) when `out_path` is None — main() uses that compute-only pass
    to *build* the final outlines from the strokes (see stage 2 there)."""
    face = trace.load_face(ttf_path, TRACE_PPEM)
    upem = face.units_per_EM
    px2u = upem / TRACE_PPEM
    resample = trace.RESAMPLE_AT_3000 * upem / 3000.0
    smooth_win = trace.SMOOTH_WIN_AT_512
    spur = trace.TRACE_SPUR_PX_AT_512
    dot_area = 2500 * (TRACE_PPEM / 512.0) ** 2

    glyphs, cells, extremes, warnings, covers = [], [], {}, [], {}
    for ch in routes.LETTERS:
        mask, left, top, adv_px = trace.render(face, ch)
        radius = trace.distance_transform_edt(mask)

        def to_units(p, left=left, top=top):
            r, c = p
            return ((left + c + 0.5) * px2u, (top - r - 0.5) * px2u)

        comps = trace.components(mask)
        big = np.zeros_like(mask)
        dots = []
        for area, comp in comps:
            (big := big | comp) if area >= dot_area else dots.append(comp)
        sk = trace.prune_spurs(trace.skeletonize(big), spur)
        rows, cols = np.where(big)
        bbox = (rows.min(), cols.min(), rows.max(), cols.max())
        rows_a, _ = np.where(mask)
        extremes[ch] = (to_units((rows_a.max(), 0))[1], to_units((rows_a.min(), 0))[1])

        strokes, raw_paths, reversals = [], [], 0
        for spec in routes.ROUTES[ch]:
            if spec == "dot":
                if not dots:
                    warnings.append(f"{ch}: no dot component")
                    strokes.append([])
                    continue
                rr, cc = np.where(dots[0])
                strokes.append([to_units((rr.mean(), cc.mean()))])
                continue
            path, fail = trace.route_stroke(sk, bbox, spec)
            if path is None:
                warnings.append(f"{ch}: no path between {fail[0]} and {fail[1]}")
                strokes.append([])
                continue
            pts, rev = trace.process_stroke(path, big, radius, sk, to_units,
                                            smooth_win, resample)
            strokes.append(pts)
            raw_paths.append(path)
            reversals += rev
        if reversals != routes.EXPECTED_REVERSALS.get(ch, 0):
            warnings.append(f"{ch}: {reversals} mid-line reversals "
                            f"(expected {routes.EXPECTED_REVERSALS.get(ch, 0)})")
        covers[ch] = route_cover(sk, raw_paths, resample / px2u)
        outside = points_outside_ink(strokes, mask, left, top, px2u)
        if outside:
            warnings.append(f"{ch}: {outside} baked point(s) outside the ink")
        glyphs.append((ch, adv_px * px2u, strokes))
        cells.append((ch, mask, left, top, strokes))

    if out_path is not None:
        trace.write_rust(out_path, glyphs, upem, extremes["x"][1],
                         extremes["l"][1], extremes["g"][0], pen_width,
                         fingerprint, traces_header(fingerprint))
    return warnings, covers, cells, px2u, glyphs


def traces_header(fingerprint):
    return [
        "// @generated by tools/handwriting_font/build.py --traces — do not edit.",
        f"// Pen-stroke centerlines for {FAMILY}, in font units (y up, origin at",
        "// the pen position on the baseline). Stroke order and direction follow",
        "// the Tasmanian handwriting charts (tools/handwriting_font/*.png).",
        f"// source font fnv1a64 = 0x{fingerprint:016x}",
    ]


def points_outside_ink(strokes, mask, left, top, px2u):
    """How many baked points miss the glyph's ink. The tip extensions push the
    pen past where the skeleton stops (into a taper, a flick, a mitred v/w
    vertex) — they must stay *inside* the letter: a guide poking out of the ink
    teaches the wrong shape and looks broken under the rendered glyph."""
    n = 0
    for st in strokes:
        for ux, uy in st:
            c = int(round(ux / px2u - left - 0.5))
            r = int(round(top - uy / px2u - 0.5))
            if not (0 <= r < mask.shape[0] and 0 <= c < mask.shape[1] and mask[r, c]):
                n += 1
    return n


def route_cover(sk, raw_paths, resample_px):
    """Fraction of the glyph's skeleton pixels the *routed pixel path* passes
    over. Below ~0.95 means the route skipped part of the letter (a branch
    never visited, a loop cut short). Measured on the raw route, not the
    emitted geometry: the retrace-into-branch blend deliberately sweeps off
    the skeleton, but the route itself must still have visited everything."""
    route_px = [p for path in raw_paths for p in path]
    skp = np.argwhere(sk)
    if not route_px or not len(skp):
        return 0.0
    rp = np.array(route_px, dtype=float)
    d2 = ((skp[:, None, :] - rp[None, :, :]) ** 2).sum(-1).min(1)
    return float((d2 < (resample_px * 1.5) ** 2).mean())


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--pdf-dir", default=DEFAULT_PDF_DIR,
                    help="directory holding the three Tasmanian handwriting PDFs")
    ap.add_argument("--out", default=OUT_TTF)
    ap.add_argument("--sheet", default=SHEET_PNG)
    ap.add_argument("--skip-charts", action="store_true")
    ap.add_argument("--traces", action="store_true",
                    help="also regenerate core/src/tracing_data.rs")
    ap.add_argument("--traces-dry-run", metavar="PATH",
                    help="run the tracing emitter to PATH instead (smoke test)")
    ap.add_argument("--trace-debug", default=TRACE_DEBUG_PNG,
                    help="contact sheet of the routed strokes (eyeball it)")
    ap.add_argument("--digit-source", choices=["regnum", "bold"], default="regnum")
    args = ap.parse_args()
    t0 = time.time()

    log("== source material")
    ref = extract_reference(args.pdf_dir)
    if not args.skip_charts:
        render_charts(args.pdf_dir)

    upem_ref = ref_upem(ref["regular"])
    assert upem_ref == 3000, f"unexpected reference upem {upem_ref}"

    log("== reference selection")
    stripped, kept = strip_overlays(ref["regnum"], LOWER + UPPER + DIGITS)
    # (a) the strip rule must reproduce TasBegRegular's letter silhouettes
    val_ppem = 256
    f_reg = trace.load_face(ref["regular"], val_ppem)
    f_str = trace.load_face(stripped, val_ppem)
    tol = val_ppem * 0.02  # ~ the pen radius at this ppem
    ious, covs = {}, {}
    for ch in LOWER + UPPER:
        ious[ch], covs[ch] = silhouette_match(trace.render(f_reg, ch)[0],
                                              trace.render(f_str, ch)[0], tol)
    w_i = min(ious, key=ious.get)
    w_c = min(covs, key=covs.get)
    log(f"  strip vs TasBegRegular: coverage min {covs[w_c]:.4f} ({w_c!r}) "
        f"mean {np.mean(list(covs.values())):.4f}; IoU min {ious[w_i]:.3f} "
        f"({w_i!r}) mean {np.mean(list(ious.values())):.3f}")
    bad = [f"{ch}: kept {kept[ch][0]} vs regular {contour_count(ref['regular'], ch)}"
           for ch in LOWER + UPPER
           if kept[ch][0] != contour_count(ref["regular"], ch)]
    assert not bad, "letter strip mismatch — " + "; ".join(bad)
    assert covs[w_c] > 0.96, f"overlay strip left ink on {w_c!r} ({covs[w_c]:.3f})"
    # (b) per-digit kept-contour counts must match the clean bold digits
    bad = []
    for ch in DIGITS:
        n_bold = contour_count(ref["bold"], ch)
        if kept[ch][0] != n_bold:
            bad.append(f"{ch}: kept {kept[ch][0]}/{kept[ch][1]} vs bold {n_bold}")
    log("  digit contours kept: " + " ".join(f"{c}:{kept[c][0]}/{kept[c][1]}"
                                             for c in DIGITS))
    assert not bad, "digit strip mismatch — " + "; ".join(bad)

    # master scale
    x_top_raw = ref_ymax(ref["regular"], "x")
    k = X_HEIGHT / x_top_raw
    log(f"  raw 'x' ink top {x_top_raw} -> k = {k:.6f} (upem {upem_ref} -> {UPEM})")
    cfg = Cfg(upem_ref, PPEM, k)

    log("== centerlines")
    faces = {"regular": trace.load_face(ref["regular"], PPEM),
             "regnum": trace.load_face(stripped, PPEM),
             "bold": trace.load_face(ref["bold"], PPEM)}
    src = {ch: "regular" for ch in LOWER + UPPER + PUNCT}
    for ch in DIGITS:
        src[ch] = args.digit_source
    res = {ch: analyze(faces[src[ch]], ch, cfg) for ch in LOWER + UPPER + DIGITS + PUNCT}

    # 6. pen width: 2 x distance transform along the letter centerlines
    samples = []
    for ch in LOWER + UPPER:
        samples.extend(pen_samples(res[ch]))
    pen_px = float(np.median(samples))
    pen = pen_px * cfg.px2u
    lo, hi = np.percentile(samples, [10, 90])
    log(f"  pen width {pen:.2f} units (raw {pen_px * upem_ref / PPEM:.1f}), "
        f"p10-p90 {lo * cfg.px2u:.1f}-{hi * cfg.px2u:.1f}")
    assert 28.0 <= pen <= 45.0, f"pen width {pen:.1f} outside 28..45"

    strokes, dots, advances = {}, {}, {}
    for ch, r in res.items():
        strokes[ch], dots[ch], advances[ch] = finish(r, cfg, pen_px)
        r["dt"] = None

    # The reference 'x' ink top sets the ballpark, but a round pen laid on a
    # centerline reproduces a tapered terminal ~1% tall (the cap bulges past
    # the reference's angled cut). Stroke expansion is a Minkowski sum, so it
    # commutes with uniform scaling: measure the built 'x', then scale geometry
    # and pen together so the *shipped* x-height is exactly 400.
    c = X_HEIGHT / expand(strokes["x"], dots["x"], pen).bounds[3]
    k *= c
    pen *= c
    strokes = {ch: [[(x * c, y * c) for x, y in s] for s in v]
               for ch, v in strokes.items()}
    dots = {ch: [((p[0] * c, p[1] * c), r * c) for p, r in v]
            for ch, v in dots.items()}
    log(f"  built-x correction {c:.5f} -> k = {k:.6f}, pen {pen:.2f}")

    for ch, ext in TERMINAL_EXTEND.items():
        repair_terminals(strokes[ch], ext)
        log(f"  terminal repair {ch!r}: +{ext:.0f} units")

    # slope from the 'l' stem
    slope = measure_slope(strokes["l"])
    log(f"  stem slope {slope:.2f} deg (rightward)")

    # dot radius from the reference '.'
    dot_radius = dots["."][0][1] if dots["."] else pen / 2
    log(f"  dot radius {dot_radius:.1f} units")

    # digit cross-validation: regnum-stripped vs bold skeletons
    dev = digit_crosscheck(faces, cfg, pen_px)
    log(f"  digit centerline deviation regnum vs bold: worst-case {dev[0]:.1f} "
        f"units on {dev[1]!r} (terminal lengths differ between the weights); "
        f"bulk agreement p90 {dev[2]:.1f}")

    # authored ! and ?
    cap_raw = max(ref_ymax(ref["regular"], c) for c in "HITX")
    cap = cap_raw * k
    for ch, (pts, dt_pts) in authored_glyphs(cap, pen, slope, dot_radius).items():
        strokes[ch] = [pts]
        dots[ch] = [(p, dot_radius) for p in dt_pts]
        advances[ch] = 0.0

    # The font is built twice. Stage 1 expands the *extraction* centerlines
    # into outlines — those centerlines carry medial-axis junction artifacts,
    # so their buffer union grows small blobs at every join. That font is only
    # the routing scaffold: the pen-route emitter traces it, and stage 2
    # rebuilds every traced letter's outline as the pen extrusion of its own
    # traced strokes, clipped to the stage-1 silhouette (which keeps the
    # calibrated caps/terminals and guarantees nothing pokes past the
    # reference-gated ink). The shipped glyph ink and the tracing-game
    # template are then the same drawing.
    def assemble(trace_strokes=None):
        log("== outlines")
        polys, bounds = {}, {}
        for ch in COVERAGE:
            poly = expand(strokes[ch], dots[ch], pen)
            assert poly is not None and not poly.is_empty, f"{ch!r}: empty outline"
            if trace_strokes and ch in trace_strokes:
                # stage 2: the letter's ink is the pen sweep along its traced
                # strokes — no junction blobs — clipped to the stage-1
                # silhouette so caps and terminals stay where the calibrated
                # extraction put them
                ext = expand(trace_strokes[ch], dots[ch], pen)
                poly = ext.intersection(poly).buffer(0)
                assert not poly.is_empty, f"{ch!r}: empty trace-clipped outline"
            if ch in DIGITS + AUTHORED:
                poly = affinity.translate(poly, xoff=digit_bearing(ch) - poly.bounds[0])
            polys[ch] = polygons(poly, pen)
            bounds[ch] = poly.bounds

        glyphs = {".notdef": TTGlyphPen(None).glyph()}
        order = [".notdef", "space"]
        cmap = {0x20: "space"}
        metrics = {".notdef": (SPACE_ADVANCE, 0), "space": (SPACE_ADVANCE, 0)}
        glyphs["space"] = TTGlyphPen(None).glyph()
        ref_adv = ref_advances(ref["regular"], LOWER + UPPER + PUNCT)
        for ch in COVERAGE:
            name = glyph_name(ch)
            g = glyph_from_polys(polys[ch], pen)
            assert g.numberOfContours >= 1, f"{ch!r}: no contours"
            gx = [p[0] for p in g.coordinates]
            gy = [p[1] for p in g.coordinates]
            bx0, by0, bx1, by1 = bounds[ch]
            slack = 4 * pen
            assert (min(gx) > bx0 - slack and max(gx) < bx1 + slack
                    and min(gy) > by0 - slack and max(gy) < by1 + slack), (
                f"{ch!r}: fitted control points escape the outline "
                f"({min(gx)},{min(gy)})..({max(gx)},{max(gy)}) vs {bounds[ch]}")
            glyphs[name] = g
            order.append(name)
            cmap[ord(ch)] = name
            x0, _y0, x1, _y1 = bounds[ch]
            if ch in DIGITS + AUTHORED:
                adv = int(round(x1 - x0 + 2 * digit_bearing(ch)))
            else:
                adv = int(round(ref_adv[ch] * k))
            metrics[name] = (adv, int(round(x0)))
            assert adv > 0, f"{ch!r}: advance {adv}"

        # measured extremes of what we actually built
        x_height = bounds["x"][3]
        cap_built = max(bounds[c][3] for c in UPPER)
        ascender = max(bounds[c][3] for c in "bdfhklt")
        descender = min(bounds[c][1] for c in "gjpqy")
        y_max = max(b[3] for b in bounds.values())
        y_min = min(b[1] for b in bounds.values())
        log(f"  x-height {x_height:.1f} cap {cap_built:.1f} asc {ascender:.1f} "
            f"desc {descender:.1f}  (ink {y_min:.1f}..{y_max:.1f})")
        assert abs(x_height - X_HEIGHT) <= 3, f"x-height {x_height:.1f} != 400+-3"
        assert 770 <= ascender <= 800, f"ascender {ascender:.1f} outside 770..800"
        assert 770 <= cap_built <= 800, f"cap height {cap_built:.1f} outside 770..800"
        assert -410 <= descender <= -380, f"descender {descender:.1f} outside -380..-410"

        log("== font")
        asc_i, desc_i = int(math.ceil(y_max)), int(math.floor(y_min))
        fb = FontBuilder(UPEM, isTTF=True)
        fb.font.recalcTimestamp = False
        fb.setupGlyphOrder(order)
        fb.setupCharacterMap(cmap)
        fb.setupGlyf(glyphs)
        fb.setupHorizontalMetrics(metrics)
        fb.setupHorizontalHeader(ascent=asc_i, descent=desc_i, lineGap=0)
        fb.setupNameTable({
            "familyName": FAMILY,
            "styleName": SUBFAMILY,
            "uniqueFontIdentifier": f"{FAMILY} {VERSION}; fountouki",
            "fullName": f"{FAMILY} {SUBFAMILY}",
            "psName": "FountoukiHandwriting-Regular",
            "version": f"Version {VERSION}",
            "description": ATTRIBUTION,
            "licenseDescription": ATTRIBUTION,
            "licenseInfoURL": CC_BY_URL,
        })
        fb.setupOS2(version=4, sTypoAscender=asc_i, sTypoDescender=desc_i,
                    sTypoLineGap=0, usWinAscent=asc_i, usWinDescent=-desc_i,
                    sxHeight=int(round(x_height)), sCapHeight=int(round(cap_built)),
                    usWeightClass=400, usWidthClass=5, fsType=0, fsSelection=0x40,
                    achVendID="NONE", ulCodePageRange1=1)
        try:
            fb.font["OS/2"].recalcUnicodeRanges(fb.font)
        except Exception:  # pragma: no cover - cosmetic only
            pass
        fb.setupPost(keepGlyphNames=True)
        fb.updateHead(created=FIXED_TIMESTAMP, modified=FIXED_TIMESTAMP,
                      fontRevision=1.0, lowestRecPPEM=8)
        os.makedirs(os.path.dirname(args.out), exist_ok=True)
        fb.save(args.out)

        with open(args.out, "rb") as f:
            blob = f.read()
        fp = fnv1a64(blob)
        log(f"  wrote {args.out} ({len(blob)} bytes)  fnv1a64 = 0x{fp:016x}")

        # round-trip + render checks
        rt = TTFont(args.out)
        assert set(rt.getBestCmap()) >= {ord(c) for c in COVERAGE} | {0x20}, "cmap gap"
        rt.close()
        check = trace.load_face(args.out, 64)
        for ch in COVERAGE:
            m, _, _, adv = trace.render(check, ch)
            assert m.any(), f"{ch!r} renders blank"
            assert adv > 0, f"{ch!r} zero advance"
        log("  round-trip + freetype render OK")
        return dict(fp=fp, bounds=bounds, metrics=metrics, x_height=x_height,
                    cap_built=cap_built, ascender=ascender,
                    descender=descender)

    assemble()

    log("== pen traces")
    warns, covers, cells, px, tglyphs = emit_traces(None, args.out, 0, pen)
    for ch, cov in covers.items():
        log(f"    {ch}: strokes={len(routes.ROUTES[ch])} cover={cov:.3f}"
            + ("  <-- low" if cov < 0.95 else ""))
    for w in warns:
        log(f"    !! {w}")
    assert not warns, f"{len(warns)} tracing warning(s)"
    worst = min(covers.values())
    assert worst >= 0.95, f"route coverage {worst:.3f} too low"
    trace_strokes = {ch: [s for s in st if len(s) >= 2]
                     for ch, _m, _l, _t, st in cells}

    art = assemble(trace_strokes)
    fp = art["fp"]
    bounds, metrics = art["bounds"], art["metrics"]
    x_height, cap_built = art["x_height"], art["cap_built"]
    ascender, descender = art["ascender"], art["descender"]

    # Built-vs-reference silhouette gate: every glyph with a reference must
    # match its silhouette to within ~a pen radius. This is what catches a
    # swallowed terminal (the truncated '2' scored 0.959 here) or any future
    # trim/fit regression; `!?` are authored and have no reference.
    val_ppem = 256
    # the reference upem is 3000 and its letters sit at a different fraction
    # of the em than ours — render it at a k-scaled ppem so px/unit matches
    ref_ppem = int(round(val_ppem * upem_ref * k / UPEM))
    f_built = trace.load_face(args.out, val_ppem)
    f_ref = {"regular": trace.load_face(ref["regular"], ref_ppem),
             "regnum": trace.load_face(stripped, ref_ppem),
             "bold": trace.load_face(ref["bold"], ref_ppem)}
    tol = val_ppem * 0.02
    built_cov = {}
    for ch in LOWER + UPPER + DIGITS + PUNCT:
        _iou, built_cov[ch] = silhouette_match(
            trace.render(f_ref[src[ch]], ch)[0],
            trace.render(f_built, ch)[0], tol)
    worst = min(built_cov, key=built_cov.get)
    log(f"  built vs reference: coverage min {built_cov[worst]:.4f} "
        f"({worst!r}) mean {np.mean(list(built_cov.values())):.4f}")
    for ch, cov in built_cov.items():
        # ',' tapers to a point sharper than the pen (like the A apex, a
        # known limitation) and legitimately sits at ~0.95.
        floor = 0.94 if ch == "," else 0.97
        assert cov >= floor, (
            f"built {ch!r} drifted from the reference silhouette "
            f"({cov:.3f} < {floor}) — check its terminals/fit")

    m = dict(x_height=x_height, cap=cap_built, ascender=ascender,
             descender=descender, pen=pen, slope=slope)
    size = write_sheet(args.sheet, args.out, m)
    log(f"  wrote {args.sheet} {size[0]}x{size[1]}")

    log("== advances")
    for ch in "aomxil" + DIGITS + PUNCT + AUTHORED:
        n = glyph_name(ch)
        log(f"    {ch!r}: adv {metrics[n][0]:4d}  lsb {metrics[n][1]:4d}  "
            f"ink {bounds[ch][2] - bounds[ch][0]:6.1f}")

    dry = args.traces_dry_run
    if args.traces or dry:
        path = TRACES_RS if args.traces else dry
        log(f"== tracing data -> {path}")
        # the final font IS the extrusion of these strokes; bake them with
        # the final fingerprint, and re-render the final ink for the shipped
        # extremes + the debug sheet's grey underlay
        face_f = trace.load_face(args.out, TRACE_PPEM)
        p2u = UPEM / TRACE_PPEM
        cells_f, extr = [], {}
        for ch, _mask, _left, _top, st in cells:
            mask, left, top, _adv = trace.render(face_f, ch)
            rows_a, _ = np.where(mask)
            extr[ch] = ((top - rows_a.max() - 0.5) * p2u,
                        (top - rows_a.min() - 0.5) * p2u)
            cells_f.append((ch, mask, left, top, st))
        trace.write_rust(path, tglyphs, UPEM, extr["x"][1], extr["l"][1],
                         extr["g"][0], pen, fp, traces_header(fp))
        trace.write_debug(args.trace_debug, cells_f, px)
        log(f"    wrote {args.trace_debug} — eyeball it against "
            "tas-beginners-chart.png")

    log(f"done in {time.time() - t0:.1f}s")


def glyph_name(ch):
    special = {".": "period", ",": "comma", "!": "exclam", "?": "question"}
    if ch in special:
        return special[ch]
    if ch.isdigit():
        return ["zero", "one", "two", "three", "four", "five", "six", "seven",
                "eight", "nine"][int(ch)]
    return ch


def measure_slope(l_strokes):
    """Rightward lean of the 'l' stem, in degrees (x = tan(slope) * y)."""
    pts = np.array([p for s in l_strokes for p in s])
    top = pts[:, 1].max()
    sel = pts[(pts[:, 1] > 0.35 * top) & (pts[:, 1] < 0.95 * top)]
    if len(sel) < 4:
        return 0.0
    a, _b = np.polyfit(sel[:, 1], sel[:, 0], 1)
    return math.degrees(math.atan(a))


def digit_crosscheck(faces, cfg, pen_px):
    """Max centerline deviation between the two digit routes (stripped RegNum
    at regular weight vs the clean Bold digits, whose skeleton is
    weight-invariant). Reported in font units at upem 1000."""
    worst, worst_ch, per = 0.0, None, []
    for ch in DIGITS:
        pts = {}
        for key in ("regnum", "bold"):
            s, d, _ = finish(analyze(faces[key], ch, cfg), cfg, pen_px)
            arr = np.array([p for st in s for p in st] + [c for c, _r in d])
            pts[key] = arr
        a, b = pts["regnum"], pts["bold"]
        # align on ink bbox origin (the two weights differ in sidebearing)
        a = a - a.min(axis=0)
        b = b - b.min(axis=0)
        d1 = np.sqrt(((a[:, None, :] - b[None, :, :]) ** 2).sum(-1))
        near = np.concatenate([d1.min(axis=1), d1.min(axis=0)])
        h = float(near.max())
        per.append(float(np.percentile(near, 90)))
        if h > worst:
            worst, worst_ch = h, ch
    return worst, worst_ch, float(np.mean(per))


if __name__ == "__main__":
    main()
