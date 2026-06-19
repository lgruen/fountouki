//! The Sing Back choir — a family of rigged vector critters drawn in the *same*
//! style as the frog mascot (filled discs/ellipses, INK/WHITE details, a base /
//! feet transform-origin so a row of them lines up). The pitch→color→character
//! map means each critter is drawn in a caller-supplied tint (the rainbow-pitch
//! color), so the body is parameterized exactly like `frog`'s `color` arg; the
//! eyes/beak/whiskers stay neutral so any tint reads.
//!
//! The Frog variant *reuses* `super::frog` — it is not redrawn here.
use super::frog::{frog, FrogPose};
use super::prim::{disc, fill_ellipse, mix, shade, stroke_path};
use crate::palette;
use macroquad::prelude::*;

/// Which choir member to draw. Each maps a pitch to an instantly-readable
/// silhouette; all share the frog's proportions + feet-pivot so they line up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Critter {
    Frog,
    Duck,
    Cat,
    Owl,
}

/// A rigged pose shared by every critter. Like [`FrogPose`] the transform origin
/// is the critter's *base* (feet on the ground, 0.92r below the body center), so
/// squash/stretch and spins pivot on the ground and a row stays aligned.
#[derive(Clone, Copy)]
pub struct CritterPose {
    /// Vertical offset in px (negative = airborne).
    pub dy: f32,
    /// Rotation about the base, radians.
    pub rot: f32,
    /// Horizontal scale (squash/stretch).
    pub sx: f32,
    /// Vertical scale.
    pub sy: f32,
    /// Eyelid closure: 0 = wide open .. 1 = shut (a happy squint).
    pub blink: f32,
    /// Singing, 0..1: opens the mouth/beak and adds a small upward bounce — the
    /// "I'm singing my note now" pose.
    pub sing: f32,
}

impl Default for CritterPose {
    fn default() -> Self {
        CritterPose { dy: 0.0, rot: 0.0, sx: 1.0, sy: 1.0, blink: 0.0, sing: 0.0 }
    }
}

/// Map a feature given body-center-relative (lx,ly) through the pose: scale +
/// rotate about the base (feet, 0.92r below center), then the hop offset. At rest
/// the body center (0,0) maps back to (cx,cy) — the identity that keeps the drawn
/// critter aligned with its tap target (same convention + guard as the frog).
fn crit_point(cx: f32, cy: f32, r: f32, pose: &CritterPose, lx: f32, ly: f32) -> Vec2 {
    let (sn, cs) = pose.rot.sin_cos();
    let ox = lx * pose.sx;
    let oy = (ly - 0.92 * r) * pose.sy;
    vec2(cx + ox * cs - oy * sn, cy + 0.92 * r + pose.dy + ox * sn + oy * cs)
}

/// Draw `kind` with its body center at (cx,cy), body radius `r`, tinted `color`,
/// rigged by `pose`. Frog reuses the existing mascot art; Duck/Cat/Owl are new
/// vector art honoring dy/rot/sx/sy/blink/sing on the same feet-pivot.
pub fn critter(kind: Critter, cx: f32, cy: f32, r: f32, color: Color, pose: &CritterPose) {
    match kind {
        Critter::Frog => {
            // The "sing" beat reads as a tongue-out ribbit + a tiny extra bounce.
            let fp = FrogPose {
                dy: pose.dy - 0.10 * r * pose.sing,
                rot: pose.rot,
                sx: pose.sx,
                sy: pose.sy,
                blink: pose.blink,
                tongue: pose.sing,
            };
            frog(cx, cy, r, color, fp);
        }
        Critter::Duck => duck(cx, cy, r, color, pose),
        Critter::Cat => cat(cx, cy, r, color, pose),
        Critter::Owl => owl(cx, cy, r, color, pose),
    }
}

/// Shared contact shadow under the feet — shrinks + fades as the critter lifts.
fn contact_shadow(cx: f32, cy: f32, r: f32, dy: f32) {
    let lift = (-dy / (1.4 * r)).clamp(0.0, 1.0);
    fill_ellipse(
        cx,
        cy + 0.92 * r + 0.05 * r,
        0.85 * r * (1.0 - 0.35 * lift),
        0.16 * r,
        0.0,
        Color::new(0.10, 0.16, 0.10, 0.18 * (1.0 - 0.6 * lift)),
    );
}

/// A pair of bright eyes with a pupil + glint, or a happy closed curve when
/// `blink` shuts them. Centered on the body-relative (ex,±) anchors carried in
/// `eye = (ex, ey, er)`, scaled by `rs` (the area-ish mean so round features
/// ride squash without ballooning).
fn eyes(
    tf: &impl Fn(f32, f32) -> Vec2,
    r: f32,
    rs: f32,
    rot_deg: f32,
    blink: f32,
    eye: (f32, f32, f32),
) {
    let (ex, ey, er) = eye;
    let open = (1.0 - blink).clamp(0.0, 1.0);
    for s in [-1.0_f32, 1.0] {
        let c = tf(s * ex, ey);
        if open > 0.12 {
            fill_ellipse(c.x, c.y, er * rs, er * rs * open, rot_deg, palette::WHITE);
            let pupil = tf(s * ex, ey + 0.02 * r);
            let pr = 0.55 * er;
            fill_ellipse(pupil.x, pupil.y, pr * rs, pr * rs * open, rot_deg, palette::INK);
            let glint = tf(s * ex - 0.04 * r, ey - 0.04 * r);
            disc(glint.x, glint.y, 0.30 * er * rs, palette::WHITE);
        } else {
            let a = tf(s * ex - er, ey);
            let b = tf(s * ex, ey + 0.05 * r);
            let c2 = tf(s * ex + er, ey);
            stroke_path(&[a, b, c2], (0.06 * r * rs).max(2.0), palette::INK);
        }
    }
}

/// DUCK — a rounded body, a flat orange bill, a small folded wing. The bill +
/// feet are always warm orange so the tint stays on the body/head.
fn duck(cx: f32, cy: f32, r: f32, color: Color, pose: &CritterPose) {
    let &CritterPose { dy, rot, sx, sy, blink, sing } = pose;
    let rot_deg = rot.to_degrees();
    let rs = (sx * sy).sqrt();
    let tf = |lx: f32, ly: f32| crit_point(cx, cy, r, pose, lx, ly);
    let wing_col = shade(color, 0.92);
    let wing_edge = shade(color, 0.78);
    let belly = mix(color, palette::WHITE, 0.34);
    let bill = palette::hex(0xff8c1a);
    let bill_dark = shade(bill, 0.86);

    contact_shadow(cx, cy, r, dy);

    // Webbed feet (behind the body), bright orange.
    for s in [-1.0_f32, 1.0] {
        let p = tf(s * 0.34 * r, 0.78 * r);
        fill_ellipse(p.x, p.y, 0.26 * r * sx, 0.13 * r * sy, rot_deg, bill);
    }
    // Plump round body (a touch smaller so the head reads as a separate ball)
    // + a lighter belly patch.
    let bc = tf(0.0, 0.26 * r);
    fill_ellipse(bc.x, bc.y, 0.86 * r * sx, 0.78 * r * sy, rot_deg, color);
    let bl = tf(0.0, 0.44 * r);
    fill_ellipse(bl.x, bl.y, 0.52 * r * sx, 0.42 * r * sy, rot_deg, belly);
    // Two folded wings, one tucked along each body-side EDGE (overlapping the
    // body, not floating in the gap), each with a soft darker rim so it reads as
    // a separate wing, not a smudge. Sit them a touch HIGHER on the body (so they
    // read as folded wings, not low paws) and finish each with a small pointed
    // lower tip (a flight-feather flick). Mirror the x offset AND the tilt so the
    // pair reads symmetric — a balanced, chunky duck.
    for s in [-1.0_f32, 1.0] {
        let wing = tf(s * 0.58 * r, 0.14 * r);
        fill_ellipse(wing.x, wing.y, 0.32 * r * sx, 0.21 * r * sy, rot_deg + s * 34.0, wing_edge);
        fill_ellipse(wing.x, wing.y, 0.26 * r * sx, 0.16 * r * sy, rot_deg + s * 34.0, wing_col);
        // A pointed feather tip at the wing's lower-outer end, so it reads as a
        // folded wing rather than a rounded paw.
        let tip = tf(s * 0.66 * r, 0.42 * r);
        let ti = tf(s * 0.44 * r, 0.30 * r);
        let to = tf(s * 0.62 * r, 0.24 * r);
        draw_triangle(tip, ti, to, wing_edge);
    }

    // Round head sitting high on the body, with a cowlick tuft.
    let head = tf(0.0, -0.66 * r);
    disc(head.x, head.y, 0.52 * r * rs, color);
    let tuft = tf(0.10 * r, -1.16 * r);
    fill_ellipse(tuft.x, tuft.y, 0.13 * r * rs, 0.20 * r * rs, rot_deg + 16.0, color);

    // Flat bill, opening downward as it sings.
    let open = 0.20 * sing;
    let bu = tf(0.0, -0.56 * r - open * r);
    fill_ellipse(bu.x, bu.y, 0.46 * r * sx, 0.15 * r * sy, rot_deg, bill);
    let bd = tf(0.0, -0.46 * r + open * r);
    fill_ellipse(bd.x, bd.y, 0.40 * r * sx, (0.09 + 0.06 * sing) * r * sy, rot_deg, bill_dark);

    eyes(&tf, r, rs, rot_deg, blink, (0.22 * r, -0.80 * r, 0.15 * r));
}

/// CAT — a round head with two triangle ears, a small muzzle, whiskers. The body
/// is a chunky lozenge so the silhouette differs from the duck/owl.
fn cat(cx: f32, cy: f32, r: f32, color: Color, pose: &CritterPose) {
    let &CritterPose { dy, rot, sx, sy, blink, sing } = pose;
    let rot_deg = rot.to_degrees();
    let rs = (sx * sy).sqrt();
    let tf = |lx: f32, ly: f32| crit_point(cx, cy, r, pose, lx, ly);
    let dark = shade(color, 0.84);
    let belly = mix(color, palette::WHITE, 0.34);
    let inner_ear = palette::hexa(0xff8cbe, 0.95);
    let muzzle = mix(color, palette::WHITE, 0.55);

    contact_shadow(cx, cy, r, dy);

    // Paws peeking under the body.
    for s in [-1.0_f32, 1.0] {
        let p = tf(s * 0.42 * r, 0.74 * r);
        disc(p.x, p.y, 0.18 * r * rs, dark);
    }
    // Curled tail to the LEFT side (away from the owl neighbor) so it reads as
    // attached, not a stray squiggle in the gap. Drawn BEFORE the body in a
    // slightly darker shade + thicker, so a small curl peeks up from behind.
    let t0 = tf(-0.50 * r, 0.50 * r);
    let t1 = tf(-0.86 * r, 0.30 * r);
    let t2 = tf(-0.78 * r, -0.06 * r);
    stroke_path(&[t0, t1, t2], (0.16 * r * rs).max(3.0), dark);
    // Body lozenge + belly.
    let bc = tf(0.0, 0.30 * r);
    fill_ellipse(bc.x, bc.y, 0.78 * r * sx, 0.62 * r * sy, rot_deg, color);
    let bl = tf(0.0, 0.42 * r);
    fill_ellipse(bl.x, bl.y, 0.42 * r * sx, 0.34 * r * sy, rot_deg, belly);

    // Triangle ears (behind the head).
    for s in [-1.0_f32, 1.0] {
        let outer = tf(s * 0.70 * r, -0.96 * r);
        let base_in = tf(s * 0.18 * r, -0.62 * r);
        let base_out = tf(s * 0.58 * r, -0.50 * r);
        draw_triangle(outer, base_in, base_out, color);
        // Pink inner ear.
        let io = tf(s * 0.60 * r, -0.86 * r);
        let ii = tf(s * 0.28 * r, -0.60 * r);
        let ie = tf(s * 0.52 * r, -0.54 * r);
        draw_triangle(io, ii, ie, inner_ear);
    }
    // Round head.
    let head = tf(0.0, -0.52 * r);
    disc(head.x, head.y, 0.60 * r * rs, color);

    eyes(&tf, r, rs, rot_deg, blink, (0.26 * r, -0.62 * r, 0.16 * r));

    // Muzzle patch + nose; mouth opens as it sings.
    let mz = tf(0.0, -0.34 * r);
    fill_ellipse(mz.x, mz.y, 0.30 * r * sx, 0.22 * r * sy, rot_deg, muzzle);
    let nose = tf(0.0, -0.42 * r);
    let np = [
        tf(-0.07 * r, -0.45 * r),
        tf(0.07 * r, -0.45 * r),
        tf(0.0, -0.38 * r),
    ];
    draw_triangle(np[0], np[1], np[2], palette::hexa(0xff5e93, 1.0));
    let _ = nose;
    if sing > 0.02 {
        let m = tf(0.0, -0.26 * r);
        fill_ellipse(m.x, m.y, 0.13 * r * sx, (0.07 + 0.09 * sing) * r * sy, rot_deg, palette::INK);
    }

    // Whiskers — three a side, springing from the muzzle.
    for s in [-1.0_f32, 1.0] {
        for k in 0..3 {
            let yy = -0.40 * r + k as f32 * 0.07 * r;
            let a = tf(s * 0.18 * r, yy);
            let b = tf(s * 0.62 * r, yy - 0.04 * r + k as f32 * 0.03 * r);
            stroke_path(&[a, b], (0.03 * r * rs).max(1.5), palette::INK);
        }
    }
}

/// OWL — upright egg body, two big round eye discs, a small triangle beak, ear
/// tufts. The tallest silhouette of the four.
fn owl(cx: f32, cy: f32, r: f32, color: Color, pose: &CritterPose) {
    let &CritterPose { dy, rot, sx, sy, blink, sing } = pose;
    let rot_deg = rot.to_degrees();
    let rs = (sx * sy).sqrt();
    let tf = |lx: f32, ly: f32| crit_point(cx, cy, r, pose, lx, ly);
    let dark = shade(color, 0.82);
    let belly = mix(color, palette::WHITE, 0.30);
    let beak = palette::hex(0xff8c1a);
    let disc_face = mix(color, palette::WHITE, 0.46);

    contact_shadow(cx, cy, r, dy);

    // Two clawed feet poking out below — bigger + more separated, sitting a touch
    // forward of the contact shadow, with a small toe split so they don't merge
    // into the shadow blob.
    for s in [-1.0_f32, 1.0] {
        let p = tf(s * 0.32 * r, 0.90 * r);
        fill_ellipse(p.x, p.y, 0.22 * r * sx, 0.11 * r * sy, rot_deg, beak);
        // A central notch (BG-cream) splits the foot into two toes.
        let notch = tf(s * 0.32 * r, 0.94 * r);
        fill_ellipse(notch.x, notch.y, 0.03 * r * sx, 0.06 * r * sy, rot_deg, palette::BG);
    }
    // Tall egg body + speckled belly.
    let bc = tf(0.0, -0.04 * r);
    fill_ellipse(bc.x, bc.y, 0.78 * r * sx, 1.0 * r * sy, rot_deg, color);
    let bl = tf(0.0, 0.30 * r);
    fill_ellipse(bl.x, bl.y, 0.50 * r * sx, 0.56 * r * sy, rot_deg, belly);
    // Folded wings down the sides.
    for s in [-1.0_f32, 1.0] {
        let w = tf(s * 0.66 * r, 0.10 * r);
        fill_ellipse(w.x, w.y, 0.22 * r * sx, 0.56 * r * sy, rot_deg, dark);
    }

    // Ear tufts.
    for s in [-1.0_f32, 1.0] {
        let apex = tf(s * 0.46 * r, -1.16 * r);
        let bi = tf(s * 0.14 * r, -0.78 * r);
        let bo = tf(s * 0.50 * r, -0.74 * r);
        draw_triangle(apex, bi, bo, color);
    }

    // The two big face discs the owl is known for, with eyes inside.
    let open = (1.0 - blink).clamp(0.0, 1.0);
    for s in [-1.0_f32, 1.0] {
        let f = tf(s * 0.34 * r, -0.58 * r);
        disc(f.x, f.y, 0.40 * r * rs, disc_face);
        if open > 0.12 {
            let er = 0.26 * r * rs;
            disc(f.x, f.y, er, palette::WHITE);
            fill_ellipse(f.x, f.y, er, er * open, rot_deg, palette::WHITE);
            let pr = 0.62 * er;
            fill_ellipse(f.x, f.y + 0.02 * r, pr, pr * open, rot_deg, palette::INK);
            disc(f.x - 0.05 * r, f.y - 0.05 * r, 0.30 * er, palette::WHITE);
        } else {
            let a = tf(s * 0.34 * r - 0.18 * r, -0.58 * r);
            let b = tf(s * 0.34 * r, -0.52 * r);
            let c = tf(s * 0.34 * r + 0.18 * r, -0.58 * r);
            stroke_path(&[a, b, c], (0.07 * r * rs).max(2.0), palette::INK);
        }
    }

    // Small triangle beak between the discs; it opens downward as it sings.
    let open_b = 0.10 * sing;
    let bk = [
        tf(-0.10 * r, -0.50 * r),
        tf(0.10 * r, -0.50 * r),
        tf(0.0, -0.34 * r - open_b * r),
    ];
    draw_triangle(bk[0], bk[1], bk[2], beak);
    if sing > 0.02 {
        let m = [
            tf(-0.08 * r, -0.34 * r),
            tf(0.08 * r, -0.34 * r),
            tf(0.0, -0.22 * r - open_b * r),
        ];
        draw_triangle(m[0], m[1], m[2], shade(beak, 0.78));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same guard as the frog: each critter's drawn body center must coincide
    /// with the (cx,cy) it's asked to draw at (the tap target), and feet sit
    /// below the center / head above it. A sign slip in the pose transform once
    /// pushed the frog far below its hit circle so taps missed.
    #[test]
    fn rest_pose_centers_on_anchor() {
        let p = CritterPose::default();
        let c = crit_point(100.0, 200.0, 50.0, &p, 0.0, 0.0);
        assert!((c.x - 100.0).abs() < 1e-3, "body center x off: {}", c.x);
        assert!((c.y - 200.0).abs() < 1e-3, "body center y off: {}", c.y);
        let feet = crit_point(100.0, 200.0, 50.0, &p, 0.0, 0.78 * 50.0);
        let head = crit_point(100.0, 200.0, 50.0, &p, 0.0, -0.70 * 50.0);
        assert!(feet.y > c.y && head.y < c.y, "feet {} head {}", feet.y, head.y);
    }

    /// A hop offset shifts the whole critter straight up, no horizontal drift —
    /// keeps a singing bounce from sliding the character off its mark.
    #[test]
    fn hop_lifts_straight_up() {
        let p = CritterPose { dy: -40.0, ..Default::default() };
        let c = crit_point(100.0, 200.0, 50.0, &p, 0.0, 0.0);
        assert!((c.x - 100.0).abs() < 1e-3);
        assert!((c.y - 160.0).abs() < 1e-3, "hop y: {}", c.y);
    }
}
