//! Geometric primitives: clipping, rounded rects, smooth discs/ellipses,
//! stroked paths, gradients, stars, and the shared color mix/shade helpers.
use crate::palette;
use macroquad::prelude::*;

/// Clip subsequent draws to a logical-coordinate rect (for scroll viewports).
/// macroquad's scissor wants *framebuffer* pixels: the default pass is
/// logical×dpi, but a capture render-target pass is 1:1 — so scale by dpi only
/// when no render pass is active (interactive), not in capture. Reset with
/// [`pop_clip`].
pub fn push_clip(x: f32, y: f32, w: f32, h: f32) {
    unsafe {
        let gl = get_internal_gl().quad_gl;
        let s = if gl.get_active_render_pass().is_some() { 1.0 } else { screen_dpi_scale() };
        gl.scissor(Some(((x * s) as i32, (y * s) as i32, (w * s).ceil() as i32, (h * s).ceil() as i32)));
    }
}

/// Remove any clip set by [`push_clip`].
pub fn pop_clip() {
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }
}

/// Filled rounded rectangle.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(w / 2.0).min(h / 2.0);
    draw_rectangle(x + r, y, w - 2.0 * r, h, color);
    draw_rectangle(x, y + r, w, h - 2.0 * r, color);
    draw_circle(x + r, y + r, r, color);
    draw_circle(x + w - r, y + r, r, color);
    draw_circle(x + r, y + h - r, r, color);
    draw_circle(x + w - r, y + h - r, r, color);
}

/// Filled rounded rectangle, rotated `rot` radians (clockwise, screen y-down)
/// about `pivot`. The unrotated rect is `(x, y, w, h)` in screen space; the
/// straight `rounded_rect` is the `rot == 0` case. Used for the house-warming
/// bunting flags, which hang orthogonal to a sagging string.
pub fn rounded_rect_rot(rect: Rect, r: f32, pivot: Vec2, rot: f32, color: Color) {
    let Rect { x, y, w, h } = rect;
    let r = r.min(w / 2.0).min(h / 2.0);
    let (s, c) = rot.sin_cos();
    let tf = |px: f32, py: f32| {
        let (dx, dy) = (px - pivot.x, py - pivot.y);
        vec2(pivot.x + dx * c - dy * s, pivot.y + dx * s + dy * c)
    };
    let quad = |ax, ay, bx, by, cx, cy, dx, dy| {
        draw_triangle(tf(ax, ay), tf(bx, by), tf(cx, cy), color);
        draw_triangle(tf(ax, ay), tf(cx, cy), tf(dx, dy), color);
    };
    // The plus-shaped core (two overlapping rects) as rotated quads…
    quad(x + r, y, x + w - r, y, x + w - r, y + h, x + r, y + h);
    quad(x, y + r, x + w, y + r, x + w, y + h - r, x, y + h - r);
    // …and the four corner fillets (circles are rotation-invariant).
    for (cx, cy) in [(x + r, y + r), (x + w - r, y + r), (x + r, y + h - r), (x + w - r, y + h - r)] {
        let p = tf(cx, cy);
        draw_circle(p.x, p.y, r, color);
    }
}

/// Soft drop shadow behind a rounded rect (layered translucent rects — no blur
/// in macroquad, so fake it with a few expanding low-alpha passes).
pub fn soft_shadow(x: f32, y: f32, w: f32, h: f32, r: f32) {
    for i in 0..4 {
        let s = i as f32 * 2.5;
        let a = 0.05 - i as f32 * 0.011;
        let c = Color::new(palette::SHADOW.r, palette::SHADOW.g, palette::SHADOW.b, a.max(0.0));
        rounded_rect(x - s, y - s + 6.0, w + 2.0 * s, h + 2.0 * s, r + s, c);
    }
}

/// Card = soft shadow + rounded surface.
pub fn card(x: f32, y: f32, w: f32, h: f32, surface: Color) {
    soft_shadow(x, y, w, h, palette::RADIUS);
    rounded_rect(x, y, w, h, palette::RADIUS, surface);
}

/// Thick round-capped stroked polyline.
pub fn stroke_path(pts: &[Vec2], width: f32, color: Color) {
    for w in pts.windows(2) {
        draw_line(w[0].x, w[0].y, w[1].x, w[1].y, width, color);
    }
    for p in pts {
        draw_circle(p.x, p.y, width / 2.0, color);
    }
}

/// Generic stroked arc (a0..a1 radians, 0 = +x, CCW), round-capped.
pub fn arc(cx: f32, cy: f32, radius: f32, a0: f32, a1: f32, width: f32, color: Color) {
    // Adaptive: pick segment count from arc length so big/wide arcs stay smooth
    // on iPad while small ones are unaffected (the old fixed 24 looked faceted).
    let n = ((radius * (a1 - a0).abs() / 6.0).ceil() as usize).clamp(24, 256);
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let a = a0 + (a1 - a0) * (i as f32 / n as f32);
        pts.push(vec2(cx + radius * a.cos(), cy + radius * a.sin()));
    }
    stroke_path(&pts, width, color);
}

/// Vertical gradient fill (macroquad has no gradient primitive — band it).
pub fn vgradient(x: f32, y: f32, w: f32, h: f32, top: Color, bot: Color) {
    const BANDS: usize = 48;
    let bh = h / BANDS as f32;
    for i in 0..BANDS {
        let t = i as f32 / (BANDS as f32 - 1.0);
        let c = Color::new(
            crate::anim::lerp(top.r, bot.r, t),
            crate::anim::lerp(top.g, bot.g, t),
            crate::anim::lerp(top.b, bot.b, t),
            1.0,
        );
        draw_rectangle(x, y + i as f32 * bh, w, bh + 1.0, c);
    }
}

/// A genuinely round filled disc. macroquad's `draw_circle` is only a 20-gon,
/// which reads as visibly faceted at large sizes — use 128 segments.
pub fn disc(cx: f32, cy: f32, r: f32, color: Color) {
    draw_poly(cx, cy, 128, r, 0.0, color);
}

/// A smooth filled ellipse (macroquad's `draw_ellipse` is also only 20 sides).
/// `rot_deg` rotates the ellipse clockwise.
pub fn fill_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, rot_deg: f32, color: Color) {
    const N: usize = 128;
    let rot = rot_deg.to_radians();
    let (sr, cr) = rot.sin_cos();
    let mut prev = Vec2::ZERO;
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let (px, py) = (rx * a.cos(), ry * a.sin());
        let p = vec2(cx + px * cr - py * sr, cy + py * cr + px * sr);
        if i > 0 {
            draw_triangle(vec2(cx, cy), prev, p, color);
        }
        prev = p;
    }
}

/// Filled 5-point star.
pub fn star(cx: f32, cy: f32, r: f32, color: Color) {
    let inner = r * 0.45;
    let mut pts = [Vec2::ZERO; 10];
    for (i, p) in pts.iter_mut().enumerate() {
        let rad = if i % 2 == 0 { r } else { inner };
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        *p = vec2(cx + rad * a.cos(), cy + rad * a.sin());
    }
    for i in 0..10 {
        draw_triangle(vec2(cx, cy), pts[i], pts[(i + 1) % 10], color);
    }
}

/// Linear blend of two colors (opaque result).
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1.0)
}

/// Darken (k < 1) or brighten (k > 1) a color, keeping alpha.
pub(crate) fn shade(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}
