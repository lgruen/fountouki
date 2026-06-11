#!/usr/bin/env python3
"""Extract pen-stroke centerlines for the tracing game from VicModernCursive.

macroquad only rasterizes fonts (no outline access at runtime), so stroke
paths are precomputed here and baked into `core/src/tracing_data.rs`.

Pipeline per glyph:
  render (FreeType, no hinting) -> threshold -> skeletonize -> prune spurs ->
  route pen strokes via waypoint-guided Dijkstra over the skeleton pixels ->
  extend tips to the visual stroke ends -> smooth + resample -> font units.

Stroke order/direction follows the Victorian Modern Cursive handwriting
chart (green start dot, red end dot, numbered arrows) — committed alongside
as `vmc-stroke-order-chart.png`; it also covers digits + capitals for when
those get routes. Waypoints below are normalized to the glyph bbox:
x 0..1 left->right, y 0..1 bottom->top.

Deps: pip install freetype-py numpy scikit-image scipy pillow
Run:  python3 tools/trace_extract/extract.py
Outputs core/src/tracing_data.rs and /tmp/trace_debug.png (eyeball it).
"""

import heapq
import math
import os

import freetype
import numpy as np
from PIL import Image, ImageDraw
from scipy.ndimage import convolve, distance_transform_edt, label
from skimage.morphology import skeletonize

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
FONT = os.path.join(REPO, "app/assets/fonts/VicModernCursive-Regular.ttf")
OUT_RS = os.path.join(REPO, "core/src/tracing_data.rs")
DEBUG_PNG = "/tmp/trace_debug.png"

PPEM = 512  # raster size; big enough for a clean skeleton
SPUR_PX = 18  # prune skeleton branches shorter than this (corner artifacts)
DOT_AREA_PX = 2500  # components smaller than this are "dots" (i, j)
RESAMPLE_UNITS = 16.0  # output point spacing in font units
SMOOTH_WIN = 9  # boxcar window (px samples) to de-jag the skeleton

# Pen routes per letter, read off the VMC stroke-order chart.
# Each stroke is a list of (nx, ny) waypoints (bbox-normalized, y up), or
# the string "dot" for the dot of i/j (nearest small component).
ROUTES = {
    "a": [[(0.95, 0.97), (0.45, 1.0), (0.02, 0.55), (0.15, 0.1), (0.35, 0.02),
           (0.7, 0.15), (0.86, 0.55), (0.88, 0.88), (0.86, 0.3), (1.0, 0.3)]],
    "b": [[(0.3, 1.0), (0.2, 0.05), (0.6, 0.0), (0.95, 0.25), (0.5, 0.55)]],
    "c": [[(0.95, 0.85), (0.4, 1.0), (0.03, 0.5), (0.45, 0.02), (0.95, 0.2)]],
    "d": [[(0.8, 0.55), (0.4, 0.62), (0.03, 0.3), (0.35, 0.02), (0.78, 0.3),
           (0.82, 1.0), (0.8, 0.2), (1.0, 0.2)]],
    "e": [[(0.1, 0.45), (0.6, 0.55), (0.45, 1.0), (0.05, 0.6), (0.35, 0.02),
           (0.95, 0.25)]],
    "f": [
        [(1.0, 1.0), (0.55, 0.97), (0.35, 0.5), (0.15, 0.02)],
        [(0.02, 0.5), (0.98, 0.57)],
    ],
    "g": [[(0.95, 0.97), (0.45, 1.0), (0.03, 0.75), (0.45, 0.5), (0.85, 0.8),
           (0.9, 0.95), (0.75, 0.3), (0.45, 0.02), (0.05, 0.1)]],
    "h": [[(0.15, 1.0), (0.05, 0.02), (0.5, 0.6), (0.8, 0.05), (1.0, 0.15)]],
    "i": [
        [(0.4, 0.97), (0.25, 0.1), (0.95, 0.25)],
        "dot",
    ],
    "j": [
        [(0.9, 0.97), (0.6, 0.3), (0.05, 0.02)],
        "dot",
    ],
    "k": [[(0.3, 1.0), (0.05, 0.03), (0.3, 0.55), (0.55, 0.62), (0.45, 0.45),
           (0.75, 0.2), (1.0, 0.1)]],
    "l": [[(0.4, 1.0), (0.2, 0.1), (1.0, 0.2)]],
    "m": [[(0.02, 0.7), (0.15, 0.95), (0.12, 0.05), (0.35, 0.95), (0.5, 0.05),
           (0.75, 0.95), (0.85, 0.05), (1.0, 0.3)]],
    "n": [[(0.03, 0.7), (0.2, 0.95), (0.15, 0.05), (0.55, 0.95), (0.8, 0.05),
           (1.0, 0.3)]],
    "o": [[(0.85, 0.85), (0.4, 1.0), (0.03, 0.5), (0.5, 0.02), (0.97, 0.5),
           (0.85, 0.88), (1.0, 0.92)]],
    "p": [[(0.2, 0.85), (0.05, 0.02), (0.15, 0.45), (0.55, 0.88), (0.7, 0.4),
           (1.0, 0.62)]],
    "q": [[(0.9, 0.97), (0.4, 1.0), (0.03, 0.75), (0.45, 0.5), (0.8, 0.8),
           (0.85, 0.95), (0.75, 0.3), (1.0, 0.2)]],
    "r": [[(0.05, 0.75), (0.2, 0.95), (0.15, 0.05), (0.3, 0.7), (0.7, 0.95),
           (1.0, 0.85)]],
    "s": [[(0.9, 0.95), (0.4, 1.0), (0.2, 0.7), (0.7, 0.4), (0.4, 0.02),
           (0.05, 0.15)]],
    "t": [
        [(0.5, 1.0), (0.3, 0.4), (0.55, 0.02), (0.9, 0.15)],
        [(0.05, 0.55), (0.95, 0.65)],
    ],
    "u": [[(0.1, 0.95), (0.05, 0.3), (0.4, 0.02), (0.75, 0.4), (0.85, 0.95),
           (0.8, 0.3), (1.0, 0.35)]],
    "v": [[(0.05, 0.95), (0.35, 0.05), (0.85, 0.9), (1.0, 0.95)]],
    "w": [[(0.03, 0.95), (0.2, 0.05), (0.45, 0.9), (0.65, 0.05), (0.9, 0.9),
           (1.0, 0.95)]],
    "x": [
        [(0.05, 0.6), (0.3, 0.9), (0.5, 0.5), (0.75, 0.1), (0.95, 0.35)],
        [(0.9, 0.9), (0.5, 0.5), (0.25, 0.1), (0.05, 0.05)],
    ],
    "y": [[(0.1, 0.95), (0.05, 0.65), (0.35, 0.45), (0.7, 0.65), (0.8, 0.95),
           (0.6, 0.3), (0.3, 0.02), (0.05, 0.1)]],
    "z": [[(0.05, 0.8), (0.3, 0.95), (0.75, 0.9), (0.35, 0.55), (0.6, 0.45),
           (0.8, 0.2), (0.4, 0.02), (0.05, 0.1)]],
}

LETTERS = "abcdefghijklmnopqrstuvwxyz"


def render(face, ch):
    face.load_char(ch, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_NO_HINTING)
    g = face.glyph
    bm = g.bitmap
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
    k = np.ones((3, 3), dtype=np.uint8)
    return convolve(sk.astype(np.uint8), k, mode="constant") - sk


def prune_spurs(sk):
    """Remove short endpoint branches (skeleton artifacts at stroke corners)."""
    sk = sk.copy()
    changed = True
    while changed:
        changed = False
        deg = degree_map(sk)
        endpoints = list(zip(*np.where(sk & (deg == 1))))
        for ep in endpoints:
            # walk from the endpoint until a junction (deg>=3) or SPUR_PX steps
            chain = [ep]
            prev = None
            cur = ep
            while len(chain) <= SPUR_PX:
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


def extend_tip(path, sk, radius, at_start):
    """Extend a stroke tip to the visual end of the ink (skeletons stop one
    stroke-radius short). Only applied at true skeleton endpoints."""
    pts = path if not at_start else path[::-1]
    tip = pts[-1]
    deg = degree_map(sk)
    if deg[tip] != 1:
        return path
    # tangent from the last few pixels
    back = pts[max(0, len(pts) - 8)]
    v = (tip[0] - back[0], tip[1] - back[1])
    n = math.hypot(*v)
    if n < 1e-6:
        return path
    r = radius[tip] * 0.85
    ext = (tip[0] + v[0] / n * r, tip[1] + v[1] / n * r)
    if at_start:
        return [ext] + path
    return path + [ext]


def smooth_resample(path, to_units, spacing):
    pts = np.array([to_units(p) for p in path], dtype=np.float64)
    if len(pts) > SMOOTH_WIN:
        k = SMOOTH_WIN
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


def main():
    face = freetype.Face(FONT)
    face.set_pixel_sizes(0, PPEM)
    upem = face.units_per_EM
    px2u = upem / PPEM

    glyphs = []  # (ch, advance_units, [stroke point lists in font units])
    debug_cells = []
    extremes = {}

    for ch in LETTERS:
        mask, left, top, adv_px = render(face, ch)
        sk_all = skeletonize(mask)
        radius = distance_transform_edt(mask)
        lab, n_comp = label(mask, structure=np.ones((3, 3)))

        def to_units(p, left=left, top=top):
            r, c = p
            return ((left + c + 0.5) * px2u, (top - r - 0.5) * px2u)

        # split big components (routed) from dot components
        comp_sizes = [(i + 1, int((lab == i + 1).sum())) for i in range(n_comp)]
        big = [i for i, s in comp_sizes if s >= DOT_AREA_PX]
        dots = [i for i, s in comp_sizes if s < DOT_AREA_PX]
        body = np.isin(lab, big)
        sk = prune_spurs(skeletonize(body))

        rows, cols = np.where(body)
        bbox = (rows.min(), cols.min(), rows.max(), cols.max())
        rows_a, _ = np.where(mask)
        extremes[ch] = (to_units((rows_a.max(), 0))[1], to_units((rows_a.min(), 0))[1])

        strokes = []
        for spec in ROUTES[ch]:
            if spec == "dot":
                ci = dots[0]
                rr, cc = np.where(lab == ci)
                strokes.append([to_units((rr.mean(), cc.mean()))])
                continue
            path, fail = route_stroke(sk, bbox, spec)
            if path is None:
                print(f"!! {ch}: no path between {fail[0]} and {fail[1]}")
                strokes.append([])
                continue
            path = extend_tip(path, sk, radius, at_start=True)
            path = extend_tip(path, sk, radius, at_start=False)
            strokes.append(smooth_resample(path, to_units, RESAMPLE_UNITS))

        # coverage: skeleton pixels near the routed strokes
        route_px = []
        for spec, st in zip(ROUTES[ch], strokes):
            if spec == "dot":
                continue
            for ux, uy in st:
                route_px.append((top - uy / px2u - 0.5, ux / px2u - left - 0.5))
        skp = np.argwhere(sk)
        if route_px and len(skp):
            rp = np.array(route_px)
            d2 = ((skp[:, None, :] - rp[None, :, :]) ** 2).sum(-1).min(1)
            cover = float((d2 < (RESAMPLE_UNITS / px2u * 1.5) ** 2).mean())
        else:
            cover = 0.0
        print(f"{ch}: strokes={len(strokes)} cover={cover:.3f}")

        glyphs.append((ch, adv_px * px2u, strokes))
        debug_cells.append((ch, mask, left, top, strokes))

    # font-wide guide metrics from actual glyph extremes
    x_height = extremes["x"][1]
    ascent = extremes["l"][1]
    descent = extremes["g"][0]

    write_rust(glyphs, upem, x_height, ascent, descent)
    write_debug(debug_cells, px2u)
    print(f"upem={upem} x_height={x_height:.0f} ascent={ascent:.0f} descent={descent:.0f}")
    print(f"wrote {OUT_RS} and {DEBUG_PNG}")


def write_rust(glyphs, upem, x_height, ascent, descent):
    L = []
    L.append("// @generated by tools/trace_extract/extract.py — do not edit.")
    L.append("// Pen-stroke centerlines for VicModernCursive-Regular, in font units")
    L.append("// (y up, origin at the pen position on the baseline). Stroke order and")
    L.append("// direction follow the Victorian Modern Cursive handwriting chart.")
    L.append("use crate::tracing::GlyphTrace;")
    L.append("")
    L.append(f"pub const UPEM: f32 = {float(upem):.1f};")
    L.append(f"pub const X_HEIGHT: f32 = {x_height:.1f};")
    L.append(f"pub const ASCENT: f32 = {ascent:.1f};")
    L.append(f"pub const DESCENT: f32 = {descent:.1f};")
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
    with open(OUT_RS, "w") as f:
        f.write("\n".join(L) + "\n")


def write_debug(cells, px2u):
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
    sheet.save(DEBUG_PNG)


if __name__ == "__main__":
    main()
