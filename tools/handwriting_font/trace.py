# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "freetype-py==2.5.1",
#   "numpy==2.3.5",
#   "scipy==1.16.3",
#   "scikit-image==0.25.2",
#   "pillow==11.3.0",
# ]
# ///
"""Raster -> skeleton -> centerline machinery, shared by the font build and the
tracing-data emitter.

Ported (near-verbatim) from the retired `tools/trace_extract/extract.py`. Two
consumers, two ways of turning a skeleton into polylines:

* **Font build** (`skeleton_polylines`): stroke order/direction is irrelevant
  for an outline, so we just walk every edge of the skeleton graph once and
  cover all of it. No hand-authored routes needed.
* **Tracing data** (`route_stroke` + `process_stroke`): the pen must move in
  the taught order, so waypoint-guided Dijkstra follows the hand-authored
  routes in `routes.py`.

Everything works in *raster pixels* internally and converts to font units via
a `to_units` callback (y up, origin at the pen position on the baseline — the
frame `draw_text_ex(glyph, pen_x, baseline_y, ..)` uses).

Dijkstra over *pixels* (not a topology graph) is the trick that keeps the
routing simple: retraced segments (the a/d/m stems, where the pen passes the
same ink twice) are just the same pixels appearing in two shortest-path
segments, and loops ('o') are forced around by intermediate waypoints.
"""

import heapq
import io
import math
from collections import deque

import freetype
import numpy as np
from PIL import Image, ImageDraw
from scipy.ndimage import convolve, distance_transform_edt, label
from skimage.morphology import skeletonize

# Tunables, all expressed at a 512-pixel em; `Raster` scales them by ppem.
SPUR_PX_AT_512 = 18.0     # prune skeleton branches shorter than this
SMOOTH_WIN_AT_512 = 9     # boxcar window (px samples) to de-jag the skeleton
RESAMPLE_AT_3000 = 16.0   # output point spacing, font units at a 3000 upem

# The tracing router prunes *less* than the font build. A pruned branch leaves
# the junction knot behind, so a real pen tip that gets pruned is not merely
# missing — the route's start dot lands a spur-length inside the ink, and the
# terminal ray-extension can't rescue it (that only fires on a degree-1 end).
# Two places where that matters: the 2 o'clock tip of a/g/q (the whole point
# of the anticlockwise family is starting *at* the corner) and the sharp
# corners of z. At 6 px nothing else in a-z changes topology — verified by
# diffing the endpoint/junction listing at 18, 12, 9 and 6.
TRACE_SPUR_PX_AT_512 = 6.0

CONN8 = np.ones((3, 3), dtype=np.uint8)


def load_face(source, ppem):
    """FreeType face from a path or an in-memory font blob, sized to `ppem`."""
    if isinstance(source, (bytes, bytearray)):
        face = freetype.Face(io.BytesIO(bytes(source)))
    else:
        face = freetype.Face(str(source))
    face.set_pixel_sizes(0, ppem)
    return face


def render(face, ch):
    """Binary ink mask + bitmap origin + advance (pixels)."""
    face.load_char(ch, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_NO_HINTING)
    g = face.glyph
    bm = g.bitmap
    if bm.rows == 0 or bm.width == 0:
        return np.zeros((1, 1), bool), g.bitmap_left, g.bitmap_top, g.advance.x / 64.0
    img = np.array(bm.buffer, dtype=np.uint8).reshape(bm.rows, bm.width)
    return img > 127, g.bitmap_left, g.bitmap_top, g.advance.x / 64.0


def neighbors(r, c, sk):
    for dr in (-1, 0, 1):
        for dc in (-1, 0, 1):
            if dr == 0 and dc == 0:
                continue
            rr, cc = r + dr, c + dc
            if 0 <= rr < sk.shape[0] and 0 <= cc < sk.shape[1] and sk[rr, cc]:
                yield rr, cc


def degree_map(sk):
    return convolve(sk.astype(np.uint8), CONN8, mode="constant") - sk


def prune_spurs(sk, spur_px):
    """Remove short endpoint branches (skeleton artifacts at stroke corners)."""
    sk = sk.copy()
    changed = True
    while changed:
        changed = False
        deg = degree_map(sk)
        endpoints = list(zip(*np.where(sk & (deg == 1))))
        for ep in endpoints:
            # walk from the endpoint until a junction (deg>=3) or spur_px steps
            chain = [ep]
            prev = None
            cur = ep
            while len(chain) <= spur_px:
                nbs = [n for n in neighbors(*cur, sk) if n != prev]
                if len(nbs) != 1:
                    break
                nxt = nbs[0]
                if deg[nxt] >= 3:
                    # spur: erase the chain (junction stays)
                    for p in chain:
                        sk[p] = False
                    changed = True
                    break
                chain.append(nxt)
                prev, cur = cur, nxt
    return sk


def dijkstra(sk, src, dst):
    """Shortest path along skeleton pixels (8-connected)."""
    dist = {src: 0.0}
    prev = {}
    pq = [(0.0, src)]
    while pq:
        d, u = heapq.heappop(pq)
        if u == dst:
            break
        if d > dist.get(u, 1e18):
            continue
        for v in neighbors(*u, sk):
            w = math.hypot(v[0] - u[0], v[1] - u[1])
            nd = d + w
            if nd < dist.get(v, 1e18):
                dist[v] = nd
                prev[v] = u
                heapq.heappush(pq, (nd, v))
    if dst not in prev and dst != src:
        return None
    path = [dst]
    while path[-1] != src:
        path.append(prev[path[-1]])
    return path[::-1]


def snap(sk_pts, target):
    d = np.hypot(sk_pts[:, 0] - target[0], sk_pts[:, 1] - target[1])
    return tuple(sk_pts[np.argmin(d)])


def route_stroke(sk, bbox, vias):
    """Concatenate shortest paths through the waypoint list. bbox=(r0,c0,r1,c1)."""
    r0, c0, r1, c1 = bbox
    sk_pts = np.argwhere(sk)
    pix = []
    for nx, ny in vias:
        # normalized (x right, y up) -> pixel (row down, col right)
        pr = r1 - ny * (r1 - r0)
        pc = c0 + nx * (c1 - c0)
        pix.append(snap(sk_pts, (pr, pc)))
    path = [pix[0]]
    for a, b in zip(pix, pix[1:]):
        seg = dijkstra(sk, a, b)
        if seg is None:
            return None, (a, b)
        path.extend(seg[1:])
    return path, None


def unit_vec(v):
    n = math.hypot(*v)
    return (v[0] / n, v[1] / n) if n > 1e-9 else (0.0, 0.0)


def ink_run(p, d, mask, limit):
    """Pixels of ink from `p` along unit direction `d` before leaving the mask,
    capped at `limit`."""
    steps = 0
    while steps < limit:
        q = (int(round(p[0] + d[0] * (steps + 1))), int(round(p[1] + d[1] * (steps + 1))))
        if not (0 <= q[0] < mask.shape[0] and 0 <= q[1] < mask.shape[1]) or not mask[q]:
            break
        steps += 1
    return steps


def ray_extend(p, d, mask, radius, frac=0.85):
    """Walk from `p` along unit direction `d` to the edge of the ink (capped at
    a couple of stroke radii) — recovers the tapered tips and retrace wedges the
    skeleton stops short of. Returns the extension point, or None."""
    cap = max(3.0, radius[int(round(p[0])), int(round(p[1]))] * 2.4)
    steps = ink_run(p, d, mask, cap)
    if steps <= 1:
        return None
    return (p[0] + d[0] * steps * frac, p[1] + d[1] * steps * frac)


TURN_LOOKAHEAD = 6  # px each side when measuring the turn angle
TURN_COS = -0.17  # sharper than ~100° counts as a pen reversal/V-turn

# A *mitred vertex* (the v/w points, where two arms meet at ~35°) is a corner
# in the ink but only a bend in the skeleton: the medial axis rounds off inside
# the join and stops a pen radius short of the point, and the boxcar smoothing
# then rounds it further — v bottomed out 64 units above its own ink before
# this. Measured over a wider baseline the bend is still unmistakable: the four
# v/w vertices come out at cos <= +0.15 while the next-sharpest bend anywhere
# in a-z is +0.35 (the k pinch), so a second pass at that baseline finds them
# and nothing else. Splitting there pins the vertex through smoothing and lets
# the ray-cast push it out to the point.
VERTEX_LOOKAHEAD = 14
VERTEX_COS = 0.20
VERTEX_SEP = 16  # ... and ignore one this close to a turn we already have


def turns_and_vertices(path):
    """Sharp turns plus the mitred vertices the narrow pass reads as curves."""
    turns = sharp_turns(path)
    extra = [i for i in sharp_turns(path, VERTEX_LOOKAHEAD, VERTEX_COS, VERTEX_SEP)
             if all(abs(i - j) > VERTEX_SEP for j in turns)]
    return sorted(turns + extra)


def sharp_turns(path, lookahead=TURN_LOOKAHEAD, turn_cos=TURN_COS, sep=8):
    """Indices of sharp V-turns along a pixel path (retraces collapse to these
    after spur pruning: m/n stem bottoms, u/y peaks, the k pinch)."""
    k = lookahead
    cands = []
    for i in range(k, len(path) - k):
        a = unit_vec((path[i][0] - path[i - k][0], path[i][1] - path[i - k][1]))
        b = unit_vec((path[i + k][0] - path[i][0], path[i + k][1] - path[i][1]))
        c = a[0] * b[0] + a[1] * b[1]
        if c < turn_cos:
            cands.append((c, i))
    cands.sort()  # sharpest first; suppress neighbours
    chosen = []
    for _c, i in cands:
        if all(abs(i - j) > sep for j in chosen):
            chosen.append(i)
    return sorted(chosen)


def wedge_detour(t, d, route_set, sk):
    """At a V-turn, the retrace wedge often still has an unvisited skeleton
    branch hanging below the junction (m/n stem bottoms). Follow it: BFS from
    the turn pixel over skeleton pixels (escaping the routed pixels only near
    `t`), and return the excursion to the farthest pixel lying within a cone
    around the wedge direction `d` — or None when there is no such branch."""
    par = {t: None}
    q = deque([t])
    best = None
    while q:
        u = q.popleft()
        du = (u[0] - t[0], u[1] - t[1])
        dist = math.hypot(*du)
        if dist > 90:
            continue
        if u not in route_set and dist > 2:
            along = (du[0] * d[0] + du[1] * d[1]) / max(dist, 1e-9)
            if along > 0.35 and (best is None or dist > best[1]):
                best = (u, dist)
        for v in neighbors(*u, sk):
            if v in par:
                continue
            if v in route_set and math.hypot(v[0] - t[0], v[1] - t[1]) > 3:
                continue
            par[v] = u
            q.append(v)
    if best is None:
        return None
    seq = [best[0]]
    while seq[-1] != t:
        seq.append(par[seq[-1]])
    seq.reverse()
    return seq[1:]  # excursion beyond the turn pixel


def process_stroke(path, mask, radius, sk, to_units, smooth_win, resample):
    """Split the routed path at sharp turns, send each turn down its retrace
    wedge (the unvisited skeleton branch + a raycast to the ink boundary),
    extend the route ends to the ink boundary, and smooth each piece with its
    ends pinned — so reversals stay sharp and reach the bottom of their wedge
    instead of being rounded off early. Also returns the count of mid-line
    reversals (~180 deg turns on a degree-2 pixel with no wedge beyond), for
    the EXPECTED_REVERSALS overshoot check."""
    k = TURN_LOOKAHEAD
    deg = degree_map(sk)
    turns = turns_and_vertices(path)
    route_set = set(path)
    last = len(path) - 1

    def end_ext(idx, back_idx):
        if deg[path[idx]] != 1:
            return None
        d = unit_vec((path[idx][0] - path[back_idx][0], path[idx][1] - path[back_idx][1]))
        return ray_extend(path[idx], d, mask, radius)

    # Per-turn excursion: into-the-wedge points the pen visits and backtracks.
    excursion = {}
    midline_reversals = 0
    for i in turns:
        d_in = unit_vec((path[i][0] - path[i - k][0], path[i][1] - path[i - k][1]))
        d_out = unit_vec((path[i + k][0] - path[i][0], path[i + k][1] - path[i][1]))
        d = unit_vec((d_in[0] - d_out[0], d_in[1] - d_out[1]))
        ex = wedge_detour(path[i], d, route_set, sk)
        if (ex is None and deg[path[i]] == 2
                and d_in[0] * d_out[0] + d_in[1] * d_out[1] < -0.86):
            midline_reversals += 1
        if ex:
            tip = ex[-1]
            back = ex[max(0, len(ex) - 6)] if len(ex) > 1 else path[i]
            td = unit_vec((tip[0] - back[0], tip[1] - back[1]))
            ray = ray_extend(tip, td if td != (0.0, 0.0) else d, mask, radius)
            excursion[i] = ex + [ray] if ray else ex
        else:
            ray = ray_extend(path[i], d, mask, radius)
            if ray:
                excursion[i] = [ray]

    bounds = [0] + turns + [last]
    pieces = []
    for s, e in zip(bounds, bounds[1:]):
        seg = list(path[s:e + 1])
        if s == 0:
            r = end_ext(0, min(8, last))
            if r:
                seg = [r] + seg
        if e == last:
            r = end_ext(last, max(0, last - 8))
            if r:
                seg = seg + [r]
        pieces.append(seg)
        if e != last and e in excursion:
            ex = excursion[e]
            pieces.append([path[e]] + ex)
            pieces.append(list(reversed(ex)) + [path[e]])

    out = []
    for seg in pieces:
        pts = smooth_resample(seg, to_units, resample, smooth_win)
        out.extend(pts[1:] if out else pts)
    return out, midline_reversals


def smooth_resample(path, to_units, spacing, smooth_win):
    pts = np.array([to_units(p) for p in path], dtype=np.float64)
    if len(pts) > smooth_win:
        k = smooth_win
        pad = k // 2
        padded = np.vstack([np.repeat(pts[:1], pad, 0), pts, np.repeat(pts[-1:], pad, 0)])
        kernel = np.ones(k) / k
        sm = np.column_stack([
            np.convolve(padded[:, 0], kernel, mode="valid"),
            np.convolve(padded[:, 1], kernel, mode="valid"),
        ])
        sm[0], sm[-1] = pts[0], pts[-1]  # keep the (extended) tips exact
        pts = sm
    # uniform arc-length resample
    seg = np.hypot(*np.diff(pts, axis=0).T)
    arc = np.concatenate([[0.0], np.cumsum(seg)])
    total = arc[-1]
    if total < spacing:
        return [tuple(pts[0]), tuple(pts[-1])]
    n = max(2, int(round(total / spacing)) + 1)
    samples = np.linspace(0.0, total, n)
    out = np.column_stack([
        np.interp(samples, arc, pts[:, 0]),
        np.interp(samples, arc, pts[:, 1]),
    ])
    return [tuple(p) for p in out]


# --------------------------------------------------------------------------
# Automatic decomposition (font build): walk every skeleton edge exactly once.
# --------------------------------------------------------------------------

def _edge(a, b):
    return (a, b) if a < b else (b, a)


def skeleton_polylines(sk, merge=True):
    """Split a thinned skeleton into pixel polylines covering every pixel.

    Nodes are the endpoints (degree 1) and junctions (degree >= 3); every
    degree-2 chain between two nodes becomes one polyline, and a component
    with no nodes at all (a pure loop: o, 0, O) becomes one closed polyline.
    Order and direction are arbitrary — for an outline they do not matter.

    With `merge` (see `merge_continuations`) branches that simply continue
    through a junction are stitched back into one polyline.
    """
    deg = degree_map(sk)
    pix = set(map(tuple, np.argwhere(sk)))
    nodes = {p for p in pix if deg[p] != 2}
    used = set()
    paths = []
    for n in sorted(nodes):
        for v in sorted(neighbors(*n, sk)):
            if _edge(n, v) in used:
                continue
            used.add(_edge(n, v))
            path = [n, v]
            prev, cur = n, v
            while cur not in nodes:
                nxt = [q for q in neighbors(*cur, sk) if q != prev]
                if len(nxt) != 1 or _edge(cur, nxt[0]) in used:
                    break
                used.add(_edge(cur, nxt[0]))
                path.append(nxt[0])
                prev, cur = cur, nxt[0]
            paths.append(path)
    # pure loops have no nodes at all
    covered = {p for pa in paths for p in pa}
    remaining = pix - covered
    while remaining:
        start = min(remaining)
        path = [start]
        prev, cur = None, start
        while True:
            cand = [q for q in sorted(neighbors(*cur, sk))
                    if q != prev and _edge(cur, q) not in used]
            if not cand:
                break
            nxt = cand[0]
            used.add(_edge(cur, nxt))
            path.append(nxt)
            prev, cur = cur, nxt
            if nxt == start:
                break
        if path[-1] != start:
            path.append(start)
        paths.append(path)
        remaining -= set(path)
    return merge_continuations(paths, sk) if merge else paths


def _node_clusters(sk, deg):
    """8-connected groups of non-degree-2 pixels -> cluster id per pixel.

    Junctions are rarely a single pixel: thinning leaves little knots of
    degree-3 pixels, and spur pruning leaves a knot behind when it erases a
    branch. Treating each such pixel as its own node shatters a stroke into
    2-pixel fragments, so cluster them first."""
    nodes = {p for p in map(tuple, np.argwhere(sk)) if deg[p] != 2}
    cid, n = {}, 0
    for p in sorted(nodes):
        if p in cid:
            continue
        stack = [p]
        cid[p] = n
        while stack:
            u = stack.pop()
            for v in neighbors(*u, sk):
                if v in nodes and v not in cid:
                    cid[v] = n
                    stack.append(v)
        n += 1
    return cid


def _leave_dir(path, end, span=24):
    """Direction the polyline heads in, leaving the given end."""
    pts = path if end == 0 else path[::-1]
    far = pts[min(len(pts) - 1, span)]
    return unit_vec((far[0] - pts[0][0], far[1] - pts[0][1]))


CONTINUE_COS = -0.7   # branches this close to opposite are one stroke


def merge_continuations(paths, sk):
    """Stitch polylines that merely continue through a junction cluster.

    Two cases, and both matter for the outline:

    * a cluster with only **two** attached branches is not a junction at all —
      it is a corner left over from spur pruning (every V/W/A/N/M vertex).
      Merging turns it into one polyline through a sharp turn, which the pen
      then mitres into a point instead of capping both arms round and blunt.
    * a cluster with **three or more** branches merges the straightest opposite
      pair (the I/T/H bar running through the stem, the two strokes of an x).

    A very sharp apex (A, N, M) also grows a third branch: the medial axis of a
    wedge runs all the way to the point. It survives here, but the narrow ink
    it lives in gets it trimmed away downstream (see `build.py::_terminal`).

    Fragments that live entirely inside one cluster are dropped; they are a
    pixel or two of skeleton knot, already inside the ink the arms cover.
    """
    deg = degree_map(sk)
    cid = _node_clusters(sk, deg)
    keep = [p for p in paths
            if not (all(q in cid for q in p) and len({cid[q] for q in p}) == 1)]
    while True:
        att = {}
        for i, p in enumerate(keep):
            for end, q in ((0, p[0]), (1, p[-1])):
                if q in cid:
                    att.setdefault(cid[q], []).append((i, end))
        pair = None
        for _c, lst in sorted(att.items()):
            if len(lst) == 2 and lst[0][0] != lst[1][0]:
                pair = (lst[0], lst[1])
            elif len(lst) >= 3:
                best = CONTINUE_COS
                for x in range(len(lst)):
                    for y in range(x + 1, len(lst)):
                        if lst[x][0] == lst[y][0]:
                            continue
                        d1 = _leave_dir(keep[lst[x][0]], lst[x][1])
                        d2 = _leave_dir(keep[lst[y][0]], lst[y][1])
                        dot = d1[0] * d2[0] + d1[1] * d2[1]
                        if dot < best:
                            best, pair = dot, (lst[x], lst[y])
            if pair:
                break
        if pair is None:
            return keep
        (i, e1), (j, e2) = pair
        a = keep[i] if e1 == 1 else keep[i][::-1]
        b = keep[j] if e2 == 0 else keep[j][::-1]
        keep = [p for x, p in enumerate(keep) if x not in (i, j)] + [a + b]


def components(mask):
    """8-connected components of the ink mask, largest first."""
    lab, n = label(mask, structure=CONN8)
    out = []
    for i in range(1, n + 1):
        comp = lab == i
        out.append((int(comp.sum()), comp))
    out.sort(key=lambda t: -t[0])
    return out


# --------------------------------------------------------------------------
# Emitters carried over from trace_extract (debug sheet + the Rust table).
# --------------------------------------------------------------------------

def write_rust(path, glyphs, upem, x_height, ascent, descent, pen_width,
               fingerprint, header):
    L = list(header)
    L.append("use crate::tracing::GlyphTrace;")
    L.append("")
    L.append(f"pub const UPEM: f32 = {float(upem):.1f};")
    L.append(f"pub const X_HEIGHT: f32 = {x_height:.1f};")
    L.append(f"pub const ASCENT: f32 = {ascent:.1f};")
    L.append(f"pub const DESCENT: f32 = {descent:.1f};")
    L.append("/// Width of the pen these centerlines were measured with — the")
    L.append("/// stroke the built font paints over them is this thick.")
    L.append(f"pub const PEN_WIDTH: f32 = {pen_width:.1f};")
    L.append("/// FNV-1a-64 of the handwriting.ttf these traces were baked")
    L.append("/// from; a mismatch means the data is stale.")
    L.append(f"pub const SOURCE_FONT_FNV1A64: u64 = 0x{fingerprint:016x};")
    L.append("")
    L.append(f"pub static GLYPHS: [GlyphTrace; {len(glyphs)}] = [")
    for ch, adv, strokes in glyphs:
        L.append("    GlyphTrace {")
        L.append(f"        ch: '{ch}',")
        L.append(f"        advance: {adv:.1f},")
        L.append("        strokes: &[")
        for st in strokes:
            pts = ", ".join(f"({x:.1f}, {y:.1f})" for x, y in st)
            L.append(f"            &[{pts}],")
        L.append("        ],")
        L.append("    },")
    L.append("];")
    with open(path, "w") as f:
        f.write("\n".join(L) + "\n")


def write_debug(path, cells, px2u):
    """Contact sheet: every routed stroke drawn dark->light along its direction,
    green ring at the start, red dot at the end, over the glyph fill."""
    colors = [((40, 90, 220), (40, 200, 230)), ((230, 120, 30), (240, 200, 40)),
              ((30, 160, 60), (140, 220, 80))]
    pils = []
    for ch, mask, left, top, strokes in cells:
        h, w = mask.shape
        out = np.full((h, w, 3), 255, np.uint8)
        out[mask] = (215, 225, 240)
        pil = Image.fromarray(out)
        d = ImageDraw.Draw(pil)

        def to_px(p, left=left, top=top):
            ux, uy = p
            return (ux / px2u - left, top - uy / px2u)

        for si, st in enumerate(strokes):
            c0, c1 = colors[si % len(colors)]
            pts = [to_px(p) for p in st]
            if len(pts) == 1:
                x, y = pts[0]
                d.ellipse([x - 8, y - 8, x + 8, y + 8], fill=c0)
                continue
            n = len(pts) - 1
            for i in range(n):
                t = i / max(1, n - 1)
                col = tuple(int(a + (b - a) * t) for a, b in zip(c0, c1))
                d.line([pts[i], pts[i + 1]], fill=col, width=4)
            sx, sy = pts[0]
            ex, ey = pts[-1]
            d.ellipse([sx - 9, sy - 9, sx + 9, sy + 9], outline=(0, 160, 0), width=4)
            d.ellipse([ex - 7, ey - 7, ex + 7, ey + 7], fill=(220, 30, 30))
        pils.append((ch, pil))

    cw = max(p.width for _, p in pils) + 24
    chh = max(p.height for _, p in pils) + 36
    cols_n = 7
    rows_n = (len(pils) + cols_n - 1) // cols_n
    sheet = Image.new("RGB", (cols_n * cw, rows_n * chh), "white")
    d = ImageDraw.Draw(sheet)
    for i, (ch, p) in enumerate(pils):
        x = (i % cols_n) * cw
        y = (i // cols_n) * chh
        sheet.paste(p, (x + 12, y + 30))
        d.text((x + 6, y + 4), ch, fill=(0, 0, 0))
    sheet.save(path)


__all__ = [
    "SPUR_PX_AT_512", "TRACE_SPUR_PX_AT_512", "SMOOTH_WIN_AT_512",
    "RESAMPLE_AT_3000",
    "components", "degree_map", "dijkstra", "distance_transform_edt",
    "ink_run", "load_face", "neighbors", "process_stroke", "prune_spurs",
    "ray_extend", "render", "route_stroke", "sharp_turns", "skeletonize",
    "turns_and_vertices",
    "skeleton_polylines", "smooth_resample", "unit_vec", "write_debug",
    "write_rust",
]
