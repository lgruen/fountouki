//! Stroked UI marks (✓ ✗ →, chevron, speaker, replay, home) — vectors centered
//! on true geometric center, never font glyphs.
use super::prim::{arc, disc, stroke_path};
use crate::palette;
use macroquad::prelude::*;

/// Filled circle "button" base.
pub fn circle_btn(cx: f32, cy: f32, r: f32, fill: Color) {
    // tiny shadow
    disc(cx, cy + 4.0, r, Color::new(0.17, 0.17, 0.20, 0.10));
    disc(cx, cy, r, fill);
}

/// ✓ check mark centered on (cx,cy), sized to radius r.
pub fn mark_check(cx: f32, cy: f32, r: f32, color: Color) {
    let w = (r * 0.16).max(4.0);
    let pts = [
        vec2(cx - 0.42 * r, cy + 0.02 * r),
        vec2(cx - 0.10 * r, cy + 0.34 * r),
        vec2(cx + 0.46 * r, cy - 0.34 * r),
    ];
    stroke_path(&pts, w, color);
}

/// ✗ cross mark centered on (cx,cy).
pub fn mark_cross(cx: f32, cy: f32, r: f32, color: Color) {
    let w = (r * 0.16).max(4.0);
    let d = 0.34 * r;
    stroke_path(&[vec2(cx - d, cy - d), vec2(cx + d, cy + d)], w, color);
    stroke_path(&[vec2(cx + d, cy - d), vec2(cx - d, cy + d)], w, color);
}

/// → advance mark centered on (cx,cy).
pub fn mark_arrow(cx: f32, cy: f32, r: f32, color: Color) {
    let w = (r * 0.16).max(4.0);
    stroke_path(&[vec2(cx - 0.42 * r, cy), vec2(cx + 0.30 * r, cy)], w, color);
    let tip = vec2(cx + 0.50 * r, cy);
    draw_triangle(
        vec2(cx + 0.18 * r, cy - 0.30 * r),
        vec2(cx + 0.18 * r, cy + 0.30 * r),
        tip,
        color,
    );
}

/// Left-pointing back chevron ("<") centered on (cx,cy), sized to r.
pub fn chevron_left(cx: f32, cy: f32, r: f32, color: Color) {
    let w = (r * 0.18).max(3.0);
    stroke_path(
        &[
            vec2(cx + 0.28 * r, cy - 0.36 * r),
            vec2(cx - 0.22 * r, cy),
            vec2(cx + 0.28 * r, cy + 0.36 * r),
        ],
        w,
        color,
    );
}

/// Speaker glyph (mute button). Cone pointing right + sound waves; a slash when muted.
pub fn speaker(cx: f32, cy: f32, r: f32, color: Color, muted: bool) {
    let bx = cx - 0.55 * r;
    // magnet box
    draw_rectangle(bx, cy - 0.16 * r, 0.18 * r, 0.32 * r, color);
    // cone (trapezoid as two triangles)
    let l = bx + 0.18 * r;
    let right = cx - 0.05 * r;
    draw_triangle(
        vec2(l, cy - 0.16 * r),
        vec2(l, cy + 0.16 * r),
        vec2(right, cy + 0.42 * r),
        color,
    );
    draw_triangle(
        vec2(l, cy - 0.16 * r),
        vec2(right, cy + 0.42 * r),
        vec2(right, cy - 0.42 * r),
        color,
    );
    let w = (r * 0.1).max(2.0);
    if muted {
        // Diagonal strike-through across the whole glyph (top-right → bottom-left).
        // Knock it out in the button color first so it reads as a cut *through* the
        // same-colored cone rather than a stray line beside it.
        let a = vec2(cx + 0.5 * r, cy - 0.45 * r);
        let b = vec2(cx - 0.5 * r, cy + 0.45 * r);
        stroke_path(&[a, b], w * 2.2, palette::CARD);
        stroke_path(&[a, b], w, color);
    } else {
        arc(right + 0.12 * r, cy, 0.18 * r, -0.9, 0.9, w, color);
        arc(right + 0.12 * r, cy, 0.34 * r, -0.8, 0.8, w, color);
    }
}

/// Circular-arrow "replay" glyph.
pub fn replay_icon(cx: f32, cy: f32, r: f32, color: Color) {
    let w = (r * 0.16).max(2.5);
    arc(cx, cy, r * 0.5, -2.0, 1.6, w, color);
    let a = 1.6_f32;
    let ex = cx + r * 0.5 * a.cos();
    let ey = cy + r * 0.5 * a.sin();
    draw_triangle(
        vec2(ex - r * 0.18, ey - r * 0.04),
        vec2(ex + r * 0.12, ey - r * 0.16),
        vec2(ex + r * 0.04, ey + r * 0.2),
        color,
    );
}

/// Little house "home" glyph.
pub fn house_icon(cx: f32, cy: f32, r: f32, color: Color) {
    draw_triangle(vec2(cx, cy - r * 0.5), vec2(cx - r * 0.55, cy), vec2(cx + r * 0.55, cy), color);
    draw_rectangle(cx - r * 0.38, cy, r * 0.76, r * 0.5, color);
}
