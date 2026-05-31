//! Reusable vector drawing primitives + first scene composition.
//! Everything is drawn by us (no platform widgets) so pixels are identical
//! across targets. Marks (✓ ✗ →, chevron) are stroked vectors centered on
//! true geometric center — this deletes all the old iOS glyph-bearing CSS debt.
use crate::{palette, text};
use macroquad::prelude::*;

const SIN75: f32 = 0.965_926;
const COS75: f32 = 0.258_819;
const RAD75: f32 = 1.308_997;

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

/// Generic stroked arc (a0..a1 radians, 0 = +x, CCW), round-capped.
pub fn arc(cx: f32, cy: f32, radius: f32, a0: f32, a1: f32, width: f32, color: Color) {
    const N: usize = 24;
    let mut pts = Vec::with_capacity(N + 1);
    for i in 0..=N {
        let a = a0 + (a1 - a0) * (i as f32 / N as f32);
        pts.push(vec2(cx + radius * a.cos(), cy + radius * a.sin()));
    }
    stroke_path(&pts, width, color);
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
        stroke_path(&[vec2(cx + 0.1 * r, cy - 0.4 * r), vec2(cx + 0.55 * r, cy + 0.4 * r)], w, color);
    } else {
        arc(right + 0.12 * r, cy, 0.18 * r, -0.9, 0.9, w, color);
        arc(right + 0.12 * r, cy, 0.34 * r, -0.8, 0.8, w, color);
    }
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
/// which reads as visibly faceted at large sizes — use ~64 segments.
pub fn disc(cx: f32, cy: f32, r: f32, color: Color) {
    draw_poly(cx, cy, 64, r, 0.0, color);
}

/// A smooth filled ellipse (macroquad's `draw_ellipse` is also only 20 sides).
/// `rot_deg` rotates the ellipse clockwise.
pub fn fill_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, rot_deg: f32, color: Color) {
    const N: usize = 64;
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

/// A rigged pose for the frog mascot. Transform origin is the frog's *base*
/// (where the feet meet the ground), matching the old CSS `transform-origin:
/// 50% 100%` — so squash/stretch and spins pivot on the ground, not the middle.
#[derive(Clone, Copy)]
pub struct FrogPose {
    /// Vertical offset in px (negative = airborne).
    pub dy: f32,
    /// Rotation about the base, radians.
    pub rot: f32,
    /// Horizontal scale (squash/stretch).
    pub sx: f32,
    /// Vertical scale.
    pub sy: f32,
    /// Eyelid closure: 0 = wide open .. 1 = shut (also reads as a happy squint).
    pub blink: f32,
    /// Tongue extension, 0..1 (opens the mouth too).
    pub tongue: f32,
}

impl Default for FrogPose {
    fn default() -> Self {
        FrogPose { dy: 0.0, rot: 0.0, sx: 1.0, sy: 1.0, blink: 0.0, tongue: 0.0 }
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1.0)
}
fn shade(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

/// Map a frog feature given as a body-center-relative offset (lx,ly) through the
/// pose: scale + rotate about the *base* (the feet, 0.92r below the body center,
/// i.e. transform-origin 50% 100%), then the hop offset. At the rest pose the
/// body center (0,0) maps back to (cx,cy) — that identity is what keeps the drawn
/// frog aligned with its tap target, so it's unit-tested below.
pub(crate) fn frog_point(cx: f32, cy: f32, r: f32, pose: FrogPose, lx: f32, ly: f32) -> Vec2 {
    let (sn, cs) = pose.rot.sin_cos();
    let ox = lx * pose.sx;
    let oy = (ly - 0.92 * r) * pose.sy;
    vec2(cx + ox * cs - oy * sn, cy + 0.92 * r + pose.dy + ox * sn + oy * cs)
}

/// The drawn frog mascot — a small rigged character (vector, not emoji, so it's
/// identical on every target and can squash, spin, blink and poke its tongue
/// out). Every feature pivots on the frog's base through `pose`.
pub fn frog(cx: f32, cy: f32, r: f32, color: Color, pose: FrogPose) {
    let FrogPose { dy, rot, sx, sy, blink, tongue } = pose;
    let pi = std::f32::consts::PI;
    let rot_deg = rot.to_degrees();
    // Base = ground contact under the body (the transform origin), used for the
    // contact shadow + the lift factor.
    let base = vec2(cx, cy + 0.92 * r);
    let tf = |lx: f32, ly: f32| frog_point(cx, cy, r, pose, lx, ly);
    // Round features ride the squash in position but stay round; scale their
    // radius by the area-ish mean so they don't balloon.
    let rs = (sx * sy).sqrt();

    let dark = shade(color, 0.82);
    let belly = mix(color, palette::WHITE, 0.30);
    let cheek = palette::hexa(0xff8cbe, 0.92);
    let mouth = palette::INK;

    // Contact shadow: shrinks + fades as the frog leaves the ground.
    let lift = (-dy / (1.4 * r)).clamp(0.0, 1.0);
    fill_ellipse(
        base.x,
        base.y + 0.05 * r,
        0.85 * r * (1.0 - 0.35 * lift),
        0.16 * r,
        0.0,
        Color::new(0.10, 0.16, 0.10, 0.18 * (1.0 - 0.6 * lift)),
    );

    // Feet (behind the body).
    for s in [-1.0_f32, 1.0] {
        let p = tf(s * 0.55 * r, 0.60 * r);
        disc(p.x, p.y, 0.30 * r * rs, dark);
    }
    // Body + belly patch (ellipses so squash/stretch reads).
    let bc = tf(0.0, 0.0);
    fill_ellipse(bc.x, bc.y, r * sx, r * sy, rot_deg, color);
    let bl = tf(0.0, 0.32 * r);
    fill_ellipse(bl.x, bl.y, 0.6 * r * sx, 0.5 * r * sy, rot_deg, belly);
    // Rosy cheeks.
    for s in [-1.0_f32, 1.0] {
        let p = tf(s * 0.62 * r, 0.14 * r);
        disc(p.x, p.y, 0.15 * r * rs, cheek);
    }

    // Eyes (bulging on top).
    let open = (1.0 - blink).clamp(0.0, 1.0);
    for s in [-1.0_f32, 1.0] {
        let socket = tf(s * 0.5 * r, -0.72 * r);
        disc(socket.x, socket.y, 0.36 * r * rs, color);
        if open > 0.12 {
            let wr = 0.27 * r * rs;
            fill_ellipse(socket.x, socket.y, wr, wr * open, rot_deg, palette::WHITE);
            let pupil = tf(s * 0.5 * r, -0.68 * r);
            let pr = 0.14 * r * rs;
            fill_ellipse(pupil.x, pupil.y, pr, pr * open, rot_deg, palette::INK);
            let glint = tf(s * 0.5 * r - 0.06 * r, -0.80 * r);
            disc(glint.x, glint.y, 0.05 * r * rs, palette::WHITE);
        } else {
            // Happy closed eye: a small downward curve.
            let a = tf(s * 0.5 * r - 0.13 * r, -0.72 * r);
            let b = tf(s * 0.5 * r, -0.66 * r);
            let c = tf(s * 0.5 * r + 0.13 * r, -0.72 * r);
            stroke_path(&[a, b, c], (0.06 * r * rs).max(2.0), palette::INK);
        }
    }

    // Mouth: a gentle smile, or an open "ribbit" with the tongue out.
    if tongue > 0.02 {
        let m = tf(0.0, 0.20 * r);
        fill_ellipse(m.x, m.y, 0.40 * r * sx, (0.15 + 0.10 * tongue) * r * sy, rot_deg, mouth);
        let t = tf(0.0, (0.28 + 0.26 * tongue) * r);
        disc(t.x, t.y, 0.15 * r * rs, palette::hex(0xf0566e));
    } else {
        let mut smile = [Vec2::ZERO; 9];
        for (i, sp) in smile.iter_mut().enumerate() {
            let a = 0.30 + (pi - 0.60) * (i as f32 / 8.0);
            *sp = tf(0.5 * r * a.cos(), 0.06 * r + 0.46 * r * a.sin());
        }
        stroke_path(&smile, (0.085 * r * rs).max(2.0), palette::INK);
    }
}

/// A simple drawn flower-plant rising from `ground_y`.
pub fn plant(cx: f32, ground_y: f32, size: f32) {
    draw_line(cx, ground_y, cx, ground_y - size, (size * 0.12).max(2.0), palette::GROUND_BOT);
    let fy = ground_y - size;
    for k in 0..5 {
        let a = k as f32 / 5.0 * std::f32::consts::TAU;
        draw_circle(cx + a.cos() * size * 0.3, fy + a.sin() * size * 0.3, size * 0.22, palette::RAINBOW[0]);
    }
    draw_circle(cx, fy, size * 0.22, palette::RAINBOW[2]);
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

/// First milestone scene: a static Phonics card with rainbow + action buttons,
/// composed against the visual spec at the given pixel size. (Layout is
/// hardcoded for now; generalized into a layout system once it looks right.)
pub fn phonics_card_preview(fonts: &text::Fonts, w: f32, h: f32) {
    let card_w = (w * 0.34).clamp(300.0, 460.0);
    let card_h = (h * 0.42).clamp(260.0, 420.0);
    let cx = w / 2.0;
    let card_y = h * 0.47 - card_h / 2.0;

    // Rainbow above the card.
    let scale = card_w / 240.0 * 1.45;
    let horizon = card_y - 18.0;
    rainbow(cx, horizon, scale, (10.0 * scale).max(8.0), 3);

    // Card.
    card(cx - card_w / 2.0, card_y, card_w, card_h, palette::CARD);

    // Big cursive letter.
    let size = (card_h * 0.62) as u16;
    text::draw_centered("a", cx, card_y + card_h * 0.5, size, &fonts.cursive, palette::INK);

    // Action row, centered under the card axis. ✓ is the hero (bigger, green);
    // ✗ is smaller/neutral; both centers symmetric in equal slots.
    let got_r = 52.0;
    let miss_r = 34.0;
    let slot = got_r * 2.0;
    let gap = 34.0;
    let total = 2.0 * slot + gap;
    let x0 = cx - total / 2.0;
    let by = card_y + card_h + (h - (card_y + card_h)) * 0.42;
    let miss_cx = x0 + slot / 2.0;
    let got_cx = x0 + slot + gap + slot / 2.0;

    circle_btn(miss_cx, by, miss_r, palette::CARD);
    mark_cross(miss_cx, by, miss_r, palette::MUTED);
    circle_btn(got_cx, by, got_r, palette::OK);
    mark_check(got_cx, by, got_r, palette::OK_STRONG);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frog's drawn body center must coincide with the (cx,cy) it's asked to
    /// draw at — that's the tap target. A sign slip in the pose transform once
    /// pushed the visible frog ~1.84r below its hit circle, so taps missed.
    #[test]
    fn frog_rest_pose_centers_on_anchor() {
        let p = FrogPose::default();
        let c = frog_point(100.0, 200.0, 50.0, p, 0.0, 0.0);
        assert!((c.x - 100.0).abs() < 1e-3, "body center x off: {}", c.x);
        assert!((c.y - 200.0).abs() < 1e-3, "body center y off: {}", c.y);
        // Feet sit below the center; eyes sit above it.
        let feet = frog_point(100.0, 200.0, 50.0, p, 0.0, 0.60 * 50.0);
        let eyes = frog_point(100.0, 200.0, 50.0, p, 0.0, -0.72 * 50.0);
        assert!(feet.y > c.y && eyes.y < c.y, "feet {} eyes {}", feet.y, eyes.y);
    }

    /// A hop offset shifts the whole frog vertically with no horizontal drift.
    #[test]
    fn frog_hop_lifts_straight_up() {
        let p = FrogPose { dy: -40.0, ..Default::default() };
        let c = frog_point(100.0, 200.0, 50.0, p, 0.0, 0.0);
        assert!((c.x - 100.0).abs() < 1e-3);
        assert!((c.y - 160.0).abs() < 1e-3, "hop y: {}", c.y);
    }
}
