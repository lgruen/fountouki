//! The drawn frog mascot — a small rigged vector character (not an emoji, so
//! it's identical on every target and can squash, spin, blink and poke its
//! tongue out).
use super::prim::{disc, fill_ellipse, mix, shade, stroke_path};
use crate::palette;
use macroquad::prelude::*;

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

/// Draw the frog with its body center at (cx,cy), body radius `r`. Every
/// feature pivots on the frog's base through `pose`.
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
        // A smooth closed smile: 17 points (16 segments) so the curve reads
        // round, not faceted — the 9-point version looked angular on-device.
        let mut smile = [Vec2::ZERO; 17];
        let n = (smile.len() - 1) as f32;
        for (i, sp) in smile.iter_mut().enumerate() {
            let a = 0.30 + (pi - 0.60) * (i as f32 / n);
            *sp = tf(0.5 * r * a.cos(), 0.06 * r + 0.46 * r * a.sin());
        }
        stroke_path(&smile, (0.085 * r * rs).max(2.0), palette::INK);
    }
}

/// A party cone hat perched on the frog's eye bulges, pompom on top — for the
/// house-warming guests. Draw AFTER [`frog`] with the same anchor + pose.
pub fn frog_party_hat(cx: f32, cy: f32, r: f32, pose: FrogPose, color: Color) {
    let rs = (pose.sx * pose.sy).sqrt();
    let tf = |lx: f32, ly: f32| frog_point(cx, cy, r, pose, lx, ly);
    let l = tf(-0.30 * r, -1.02 * r);
    let rr = tf(0.30 * r, -1.02 * r);
    let apex = tf(0.0, -1.58 * r);
    draw_triangle(l, rr, apex, color);
    // A jaunty stripe + the pompom.
    let s0 = tf(-0.19 * r, -1.22 * r);
    let s1 = tf(0.19 * r, -1.22 * r);
    stroke_path(&[s0, s1], (0.07 * r * rs).max(2.0), palette::WHITE);
    disc(apex.x, apex.y, 0.11 * r * rs, mix(color, palette::WHITE, 0.55));
}

/// DJ headphones for the frog: two ear cups on the sides of the head plus a band
/// arcing over the top, so the frog visibly "runs the music" (a hat alone reads
/// as just another party-goer). Draw AFTER [`frog`] with the same anchor + pose
/// so it rides every hop, squash and tilt. `accent` tints the cup faces.
pub fn frog_headphones(cx: f32, cy: f32, r: f32, pose: FrogPose, accent: Color) {
    let rs = (pose.sx * pose.sy).sqrt();
    let tf = |lx: f32, ly: f32| frog_point(cx, cy, r, pose, lx, ly);
    let band = palette::INK;
    // The headband: an arc from the left cup, OVER the crown (well above the eye
    // bulges, which top out ~ -1.08r), to the right cup. Sampled as a poly-line so
    // it reads round through any rotation/squash.
    let cup_x = 1.04; // ear-cup centre, in units of r, at the widest temple
    let cup_y = -0.18; // dropped to the side of the head (clear of the eyes)
    let n = 11usize;
    let mut arc = Vec::with_capacity(n);
    for i in 0..n {
        let u = i as f32 / (n - 1) as f32;
        let a = std::f32::consts::PI * u; // 0 (right cup) .. PI (left cup)
        // Ellipse from cup to cup, bowing high over the crown.
        let lx = cup_x * r * a.cos();
        let ly = cup_y * r - (1.34 * r - cup_y.abs() * r) * a.sin();
        arc.push(tf(lx, ly));
    }
    stroke_path(&arc, (0.11 * r * rs).max(3.0), band);
    // The two ear cups: a dark outer shell with an accent-tinted face, clamped on
    // the SIDES of the head (clear of the eyes so it reads as cans, not goggles).
    for s in [-1.0_f32, 1.0] {
        let p = tf(s * cup_x * r, cup_y * r);
        disc(p.x, p.y, 0.30 * r * rs, band);
        disc(p.x, p.y, 0.18 * r * rs, accent);
        disc(p.x, p.y, 0.07 * r * rs, mix(accent, palette::WHITE, 0.5));
    }
}

/// A builder's hard hat perched on the frog's eye bulges — draw AFTER [`frog`]
/// with the same anchor + pose so it rides every hop, squash and tilt.
pub fn frog_hard_hat(cx: f32, cy: f32, r: f32, pose: FrogPose) {
    let rs = (pose.sx * pose.sy).sqrt();
    let rot_deg = pose.rot.to_degrees();
    let tf = |lx: f32, ly: f32| frog_point(cx, cy, r, pose, lx, ly);
    let dome = tf(0.0, -1.10 * r);
    let brim = tf(0.0, -1.02 * r);
    let ridge = tf(0.0, -1.34 * r);
    fill_ellipse(dome.x, dome.y, 0.56 * r * rs, 0.36 * r * rs, rot_deg, palette::GOLD);
    fill_ellipse(ridge.x, ridge.y, 0.17 * r * rs, 0.09 * r * rs, rot_deg, mix(palette::GOLD, palette::WHITE, 0.35));
    fill_ellipse(brim.x, brim.y, 0.78 * r * rs, 0.11 * r * rs, rot_deg, shade(palette::GOLD, 0.86));
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
