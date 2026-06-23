//! Scene dressing: the phonics rainbow, sky elements (cloud/sun), the drawn
//! igloo exemplar, and the rainbow-garden plant pool.
use super::prim::{disc, fill_ellipse, mix, shade, stroke_path};
use crate::palette;
use macroquad::prelude::*;

const COS75: f32 = 0.258_819;
const RAD75: f32 = 1.308_997;

/// One rainbow arc band (semicircle bow on a horizon). `sagitta` and the
/// derived radius/half-width follow the exact spec geometry, scaled.
pub fn rainbow_arc(cx: f32, horizon_y: f32, sagitta: f32, stroke: f32, color: Color) {
    let r = sagitta / (1.0 - COS75);
    let center_y = horizon_y - sagitta + r;
    const N: usize = 60;
    let mut pts = Vec::with_capacity(N + 1);
    for i in 0..=N {
        let theta = -RAD75 + (2.0 * RAD75) * (i as f32 / N as f32);
        pts.push(vec2(cx + r * theta.sin(), center_y - r * theta.cos()));
    }
    stroke_path(&pts, stroke, color);
}

/// The phonics rainbow: `filled` outermost bands drawn in ROYGBIV.
/// `scale` maps the 240×80 design viewBox to screen units.
pub fn rainbow(cx: f32, horizon_y: f32, scale: f32, stroke: f32, filled: usize) {
    // Draw inner→outer so outer (red) sits on top at the horizon ends.
    for i in (0..7).rev() {
        if i >= filled {
            continue;
        }
        let t = i as f32 / 6.0;
        let sagitta = (65.0 - 40.0 * t) * scale;
        rainbow_arc(cx, horizon_y, sagitta, stroke, palette::RAINBOW[i]);
    }
}

/// A pale "to be filled" rainbow: all 7 bands washed toward the background, so
/// the meter's shape is visible from zero stars and fills over it in color.
/// Opaque (mixed, not alpha) — translucent stroked paths would self-overlap
/// into blotchy joints.
pub fn rainbow_ghost(cx: f32, horizon_y: f32, scale: f32, stroke: f32, bg: Color) {
    for i in (0..7).rev() {
        let t = i as f32 / 6.0;
        let sagitta = (65.0 - 40.0 * t) * scale;
        let c = mix(palette::RAINBOW[i], bg, 0.82);
        rainbow_arc(cx, horizon_y, sagitta, stroke, c);
    }
}

/// A soft white cloud: a few overlapping round puffs on a flat base.
pub fn cloud(cx: f32, cy: f32, scale: f32) {
    let c = Color::new(1.0, 1.0, 1.0, 0.9);
    disc(cx, cy, scale, c);
    disc(cx - scale * 1.1, cy + scale * 0.25, scale * 0.72, c);
    disc(cx + scale * 1.1, cy + scale * 0.25, scale * 0.78, c);
    disc(cx + scale * 0.45, cy - scale * 0.5, scale * 0.7, c);
    disc(cx - scale * 0.5, cy - scale * 0.35, scale * 0.6, c);
    // flat-ish bottom so it reads as a cloud, not a blob cluster
    draw_rectangle(cx - scale * 1.7, cy + scale * 0.1, scale * 3.4, scale * 0.55, c);
}

/// Soft glowing sun.
pub fn sun(cx: f32, cy: f32, r: f32) {
    disc(cx, cy, r * 1.6, Color::new(1.0, 0.84, 0.4, 0.22));
    disc(cx, cy, r, palette::SUN_EDGE);
    disc(cx, cy, r * 0.82, palette::SUN_MID);
    disc(cx - r * 0.18, cy - r * 0.18, r * 0.5, palette::SUN_CORE);
}

/// Rays bursting from a tapped sun: `t` 0..1 fades them out, `rot` spins the
/// spokes. Drawn over the sun's glow so the whole sky lights up when poked.
pub fn sun_rays(cx: f32, cy: f32, r: f32, t: f32, rot: f32) {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return;
    }
    const N: usize = 10;
    let len = r * (0.45 + 0.8 * t);
    let col = Color::new(1.0, 0.82, 0.32, 0.55 * t);
    let w = (r * 0.12).max(2.0);
    for k in 0..N {
        let a = rot + k as f32 / N as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        let r0 = r * 1.2;
        draw_line(cx + c * r0, cy + s * r0, cx + c * (r0 + len), cy + s * (r0 + len), w, col);
    }
}

/// A drawn igloo — vector art, not an emoji: there is no igloo glyph in Unicode
/// (so no Twemoji sprite exists), and "igloo" is the gold-standard preschool
/// 'i' word. Reads as a snow-block dome with an arched entrance; fills a box of
/// side `s` centered on (cx, cy).
pub fn igloo(cx: f32, cy: f32, s: f32) {
    use std::f32::consts::PI;
    let snow = palette::hex(0xeaf2fb);
    let seam = palette::hexa(0x9db6d2, 0.85);
    let edge = palette::hexa(0x6f8db0, 0.95);
    let door = palette::hex(0x5f7a9b);

    let base = cy + s * 0.20; // flat ground line the dome sits on
    let r = s * 0.46;
    let ew = (s * 0.016).max(1.6); // outline weight
    let sw = (s * 0.012).max(1.2); // block-seam weight

    // Upper half-disc (a snow dome with a flat bottom on `base`).
    let dome = |rr: f32, color: Color| {
        const N: usize = 80;
        let mut prev = vec2(cx + rr, base);
        for i in 1..=N {
            let a = PI * i as f32 / N as f32;
            let p = vec2(cx + rr * a.cos(), base - rr * a.sin());
            draw_triangle(vec2(cx, base), prev, p, color);
            prev = p;
        }
    };
    // Stroked dome arc over angles a0..a1 in [0,PI] (0 = right base, PI = left).
    let arc = |rr: f32, a0: f32, a1: f32, width: f32, color: Color| {
        const N: usize = 64;
        let mut pts = Vec::with_capacity(N + 1);
        for i in 0..=N {
            let a = a0 + (a1 - a0) * i as f32 / N as f32;
            pts.push(vec2(cx + rr * a.cos(), base - rr * a.sin()));
        }
        stroke_path(&pts, width, color);
    };
    // Radial block seam at angle `a`, from radius r0 out to r1.
    let seam_at = |a: f32, r0: f32, r1: f32, width: f32, color: Color| {
        let p0 = vec2(cx + r0 * a.cos(), base - r0 * a.sin());
        let p1 = vec2(cx + r1 * a.cos(), base - r1 * a.sin());
        draw_line(p0.x, p0.y, p1.x, p1.y, width, color);
    };

    // Ground shadow grounds the dome (matches the frog/sun treatment).
    fill_ellipse(cx, base + s * 0.05, r * 1.04, s * 0.05, 0.0, Color::new(0.12, 0.18, 0.26, 0.12));

    // Dome body, then the snow-block courses (two horizontal arcs + staggered
    // radial seams between them) so it reads as stacked ice blocks.
    dome(r, snow);
    let c_mid = r * 0.66;
    let c_in = r * 0.34;
    for k in 0..5 {
        seam_at(PI * (0.5 + k as f32) / 5.0, c_mid, r, sw, seam);
    }
    for k in 0..4 {
        seam_at(PI * (0.5 + k as f32) / 4.0, c_in, c_mid, sw, seam);
    }
    arc(c_mid, 0.0, PI, sw, seam);
    arc(c_in, 0.0, PI, sw, seam);
    arc(r, 0.0, PI, ew, edge);
    draw_line(cx - r, base, cx + r, base, ew, edge);

    // Entrance: a smaller snow tunnel with a dark arched opening at the front.
    let re = r * 0.44;
    dome(re, snow);
    arc(re, 0.0, PI, ew, edge);
    dome(r * 0.28, door);
}

/// A simple drawn flower-plant rising from `ground_y`.
pub fn plant(cx: f32, ground_y: f32, size: f32) {
    draw_line(cx, ground_y, cx, ground_y - size, (size * 0.12).max(2.0), palette::GROUND_BOT);
    let fy = ground_y - size;
    for k in 0..5 {
        let a = k as f32 / 5.0 * std::f32::consts::TAU;
        disc(cx + a.cos() * size * 0.3, fy + a.sin() * size * 0.3, size * 0.22, palette::RAINBOW[0]);
    }
    disc(cx, fy, size * 0.22, palette::RAINBOW[2]);
}

// ── Garden (phonics rainbow-garden done scene) ──────────────────────────────
// A pool of vector-drawn plant species. Pre-port, the reward was a *random*
// emoji plant sprouting ("what grew this time?"); the port flattened that to two
// identical red flowers. These restore the variety as art-directed vectors —
// same silhouette charm, identical on every device, and re-rolled per session.

/// One garden plant species. Picked + colored per session in the done scene.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Plant {
    Daisy,
    Tulip,
    Rose,
    Bluebell,
    Sunflower,
    Mushroom,
    Berry,
}

/// The full species pool — shuffled per session so each plant that grows is a
/// different kind (variety lives in *which* plants sprout).
pub const GARDEN_SPECIES: [Plant; 7] = [
    Plant::Daisy,
    Plant::Tulip,
    Plant::Rose,
    Plant::Bluebell,
    Plant::Sunflower,
    Plant::Mushroom,
    Plant::Berry,
];

const STEM: u32 = 0x3f9d52;
const LEAF: u32 = 0x57b85f;

/// The top half of an ellipse with a flat base at `cy` (a mushroom cap / dome).
fn dome(cx: f32, cy: f32, rx: f32, ry: f32, color: Color) {
    use std::f32::consts::PI;
    const N: usize = 64;
    let mut prev = vec2(cx + rx, cy);
    for i in 1..=N {
        let a = i as f32 / N as f32 * PI;
        let p = vec2(cx + rx * a.cos(), cy - ry * a.sin());
        draw_triangle(vec2(cx, cy), prev, p, color);
        prev = p;
    }
}

/// A leaf as a pointed ellipse from `base` toward `tip`.
fn leaf(base: Vec2, tip: Vec2, half_w: f32, color: Color) {
    let mid = (base + tip) * 0.5;
    let d = tip - base;
    fill_ellipse(mid.x, mid.y, d.length() * 0.5, half_w, d.y.atan2(d.x).to_degrees(), color);
}

/// A petal: an ellipse pointing out from `center` at `ang` (radians).
fn petal(center: Vec2, ang: f32, len: f32, half_w: f32, color: Color) {
    let tip = center + vec2(ang.cos(), ang.sin()) * len;
    let mid = (center + tip) * 0.5;
    fill_ellipse(mid.x, mid.y, len * 0.5, half_w, ang.to_degrees(), color);
}

/// A curved stem from the ground to `top`, with the given half-bend already
/// baked into `top.x`. Returns nothing; the caller draws the bloom at `top`.
fn stem(cx: f32, ground_y: f32, top: Vec2, width: f32) {
    let mid = vec2((cx + top.x) * 0.5, (ground_y + top.y) * 0.5 - width * 0.4);
    stroke_path(&[vec2(cx, ground_y), mid, top], width, palette::hex(STEM));
}

/// Draw a single garden plant rooted at `(cx, ground_y)`. `size` ≈ stem/plant
/// height, `bloom` = its flower color, `sway` = a small signed breeze lean
/// (0 in goldens, so captures stay deterministic). The frog is drawn elsewhere.
pub fn garden_plant(cx: f32, ground_y: f32, size: f32, kind: Plant, bloom: Color, sway: f32) {
    use std::f32::consts::TAU;
    let leaf_c = palette::hex(LEAF);
    let bend = sway * size * 0.10;
    let top = vec2(cx + bend, ground_y - size);
    let sw = (size * 0.11).max(2.5);

    match kind {
        Plant::Daisy | Plant::Rose | Plant::Bluebell | Plant::Sunflower => {
            stem(cx, ground_y, top, sw);
            // A leaf each side, partway up the stem.
            let lb = vec2(cx + bend * 0.4, ground_y - size * 0.42);
            leaf(lb, lb + vec2(-size * 0.34, -size * 0.10), size * 0.10, leaf_c);
            leaf(lb, lb + vec2(size * 0.30, -size * 0.16), size * 0.09, leaf_c);
        }
        _ => {}
    }

    match kind {
        Plant::Daisy => {
            for k in 0..9 {
                petal(top, k as f32 / 9.0 * TAU, size * 0.46, size * 0.16, bloom);
            }
            disc(top.x, top.y, size * 0.20, palette::hex(0xffd23f));
            disc(top.x - size * 0.06, top.y - size * 0.06, size * 0.07, palette::hexa(0xffffff, 0.7));
        }
        Plant::Sunflower => {
            // Two staggered rings of pointed golden petals + a seeded brown disc.
            let gold = palette::hex(0xffc23d);
            for ring in 0..2 {
                let off = ring as f32 * 0.5;
                for k in 0..12 {
                    petal(top, (k as f32 + off) / 12.0 * TAU, size * 0.60, size * 0.12, gold);
                }
            }
            disc(top.x, top.y, size * 0.26, palette::hex(0x6e4326));
            disc(top.x, top.y, size * 0.20, palette::hex(0x4a2c18));
            disc(top.x - size * 0.07, top.y - size * 0.07, size * 0.08, palette::hexa(0xffe7a8, 0.5));
        }
        Plant::Rose => {
            // Concentric rosette: dark outer → bright inner swirl.
            for petals in [
                (6_usize, 0.0_f32, size * 0.40, shade(bloom, 0.82)),
                (5, 0.5, size * 0.28, bloom),
                (4, 0.0, size * 0.17, mix(bloom, palette::WHITE, 0.25)),
            ] {
                let (n, off, len, c) = petals;
                for k in 0..n {
                    petal(top, (k as f32 + off) / n as f32 * TAU, len, len * 0.62, c);
                }
            }
            disc(top.x, top.y, size * 0.08, shade(bloom, 0.7));
        }
        Plant::Tulip => {
            // No central stem-bloom; the tulip has its own short stem + cup.
            stem(cx, ground_y, top, sw);
            let lb = vec2(cx + bend * 0.4, ground_y - size * 0.38);
            leaf(lb, lb + vec2(-size * 0.20, size * 0.30), size * 0.12, leaf_c);
            leaf(lb, lb + vec2(size * 0.18, size * 0.34), size * 0.11, leaf_c);
            // Cup body + three lobes.
            let cup = vec2(top.x, top.y + size * 0.04);
            fill_ellipse(cup.x, cup.y, size * 0.26, size * 0.32, 0.0, bloom);
            for (dx, scl, c) in [(-0.20_f32, 0.9_f32, shade(bloom, 0.88)), (0.20, 0.9, shade(bloom, 0.88)), (0.0, 1.0, bloom)] {
                fill_ellipse(cup.x + dx * size, cup.y - size * 0.16, size * 0.13 * scl, size * 0.22 * scl, 0.0, c);
            }
        }
        Plant::Bluebell => {
            // A slender spike hung with little bell flowers, alternating sides
            // and shrinking toward a bud tip (a bluebell / foxglove stalk).
            let n = 5;
            for i in 0..n {
                let t = i as f32 / n as f32;
                let along = 0.34 + 0.60 * t; // fraction of height up the stem
                let ax = cx + bend * along;
                let ay = ground_y - size * along;
                let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                let px = ax + side * size * 0.17;
                let bs = size * (0.21 - 0.09 * t);
                let c = if i % 2 == 0 { bloom } else { mix(bloom, palette::WHITE, 0.16) };
                draw_line(ax, ay, px, ay + bs * 0.1, (size * 0.045).max(1.5), palette::hex(STEM));
                fill_ellipse(px, ay + bs * 0.45, bs * 0.55, bs * 0.80, 0.0, c);
                // Flared scalloped mouth at the bottom.
                for s in [-1.0_f32, 0.0, 1.0] {
                    disc(px + s * bs * 0.42, ay + bs, bs * 0.24, c);
                }
            }
            // Bud at the tip.
            fill_ellipse(top.x, top.y, size * 0.07, size * 0.11, 0.0, shade(bloom, 0.88));
        }
        Plant::Mushroom => {
            // Stout cream stalk + a domed spotted cap (no flower).
            let h = size * 0.60;
            let sb = vec2(cx + bend, ground_y);
            fill_ellipse(sb.x, sb.y - h * 0.5, size * 0.17, h * 0.5, 0.0, palette::hex(0xfaf0dd));
            fill_ellipse(sb.x + size * 0.07, sb.y - h * 0.5, size * 0.05, h * 0.45, 0.0, palette::hex(0xe9dcc0));
            let base_y = sb.y - h * 0.86;
            // Gill underside peeking below the cap, then the domed cap on top.
            fill_ellipse(sb.x, base_y, size * 0.46, size * 0.07, 0.0, palette::hex(0xf3e2c4));
            dome(sb.x, base_y, size * 0.48, size * 0.44, bloom);
            fill_ellipse(sb.x - size * 0.16, base_y - size * 0.22, size * 0.12, size * 0.07, -25.0, palette::hexa(0xffffff, 0.22));
            for (dx, dy, r) in [(-0.24_f32, 0.16_f32, 0.08_f32), (0.20, 0.10, 0.07), (0.0, 0.30, 0.06), (-0.04, 0.04, 0.05)] {
                disc(sb.x + dx * size, base_y - dy * size, size * r, palette::hexa(0xfff6ea, 0.95));
            }
        }
        Plant::Berry => {
            // A low leafy bush dotted with berries.
            let base = vec2(cx + bend, ground_y - size * 0.18);
            for (dx, dy, rx, ry, c) in [
                (-0.30_f32, 0.10_f32, 0.34_f32, 0.26_f32, shade(leaf_c, 0.9)),
                (0.30, 0.10, 0.34, 0.26, shade(leaf_c, 0.9)),
                (0.0, -0.10, 0.40, 0.34, leaf_c),
            ] {
                fill_ellipse(base.x + dx * size, base.y + dy * size, size * rx, size * ry, 0.0, c);
            }
            for (dx, dy) in [(-0.22_f32, 0.0_f32), (0.20, -0.06), (0.0, 0.16), (0.30, 0.16), (-0.10, -0.20)] {
                let b = vec2(base.x + dx * size, base.y + dy * size);
                disc(b.x, b.y, size * 0.12, bloom);
                disc(b.x - size * 0.04, b.y - size * 0.04, size * 0.04, palette::hexa(0xffffff, 0.55));
            }
        }
    }
}

/// A small fan of grass blades for ground texture, rooted at `(cx, ground_y)`.
pub fn grass_tuft(cx: f32, ground_y: f32, size: f32, color: Color, sway: f32) {
    for s in [-1.0_f32, -0.4, 0.3, 1.0] {
        let tip = vec2(cx + (s + sway * 0.4) * size * 0.5, ground_y - size * (1.0 - 0.18 * s.abs()));
        stroke_path(&[vec2(cx + s * size * 0.12, ground_y), tip], (size * 0.10).max(1.5), color);
    }
}
