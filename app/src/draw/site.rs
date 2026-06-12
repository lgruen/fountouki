//! The construction site around the build-a-house: the resident tower crane,
//! the foundation-stage machines (excavator + mixer truck) and the shared
//! install choreography — stage durations, sound-cue tables and the crane's
//! lift motion. The building itself is drawn by `house.rs`; everything here is
//! pure vector like the rest of `draw`. All geometry is in fractions of the
//! house footprint width `s`, relative to `(cx, base_y)` on the ground line.
use super::prim::{arc, disc, fill_ellipse, mix, rounded_rect, shade, stroke_path};
use crate::{anim, palette};
use macroquad::prelude::*;
use std::f32::consts::PI;

// --- crane geometry (fractions of s) ----------------------------------------
pub(super) const MAST_X: f32 = -0.52;
const MAST_W: f32 = 0.075;
pub(super) const JIB_Y: f32 = -1.42; // underside of the jib
const JIB_END: f32 = 0.46; // jib tip (covers every drop point incl. chimney)
const JIB_DEPTH: f32 = 0.055;
const CJIB_END: f32 = -0.72; // counter-jib tip
const HEAD_Y: f32 = -1.58; // tower-head apex
pub(super) const TROLLEY_PARK: f32 = -0.38;
pub(super) const HOOK_IDLE_Y: f32 = -1.22;
const CABLE_MIN: f32 = 0.14; // shortest hoist cable under the jib

/// Hoist cable / hook / sling steel.
pub(super) const CABLE: Color = Color::new(0.38, 0.35, 0.31, 0.92);

/// Site bounding extents in fractions of `s`: (left, right) of the footprint
/// center — covers the crane's counter-jib, the parked machines and the spoil
/// heap (the drive-in/out transits may clip the screen edge: the machines
/// arrive from "off site").
pub fn site_extents() -> (f32, f32) {
    (-0.80, 0.72)
}

/// Total height of the site above `base_y` (the crane's tower head — taller
/// than the finished house, like every real building site).
pub fn site_height(s: f32) -> f32 {
    (-HEAD_Y + 0.03) * s
}

// --- install timing + sound cues ---------------------------------------------

/// What a [`install_cues`] entry asks the scene to play. `Thunk` is the
/// part-lands beat (hammer + confetti at the part's anchor).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildCue {
    Thunk,
    Tap,
    Digger,
    TruckBeep,
    Twinkle,
    Doorbell,
}

/// Seconds each build stage's install animation runs. Stage-specific: the
/// foundation's dig + pour tells a longer story than a single crane lift.
pub fn install_dur(stage: usize) -> f32 {
    const DURS: [f32; 6] = [3.6, 3.0, 2.8, 2.0, 2.2, 2.4];
    DURS[stage.min(DURS.len() - 1)]
}

/// The crane lift's touch-down moment within its 0..1 timeline.
pub(super) const LIFT_LAND: f32 = 0.76;
/// The roof stage compresses its lift into the front portion so the tile rows
/// can sweep up afterwards.
pub(super) const ROOF_LIFT_END: f32 = 0.66;

/// Sound cues per stage as (fraction of [`install_dur`], cue). Exactly one
/// `Thunk` per stage — the lands-home beat the scene celebrates.
pub fn install_cues(stage: usize) -> &'static [(f32, BuildCue)] {
    use BuildCue::*;
    const S0: &[(f32, BuildCue)] =
        &[(0.16, Digger), (0.36, Digger), (0.60, TruckBeep), (0.93, Thunk)];
    const S1: &[(f32, BuildCue)] = &[(0.27, Tap), (0.48, Tap), (0.69, Tap), (0.88, Thunk)];
    const S2: &[(f32, BuildCue)] = &[(0.50, Thunk), (0.72, Tap), (0.84, Tap)];
    const S3: &[(f32, BuildCue)] = &[(0.76, Thunk)];
    const S4: &[(f32, BuildCue)] = &[(0.76, Thunk), (0.90, Twinkle)];
    const S5: &[(f32, BuildCue)] = &[(0.76, Thunk), (0.92, Doorbell)];
    [S0, S1, S2, S3, S4, S5][stage.min(5)]
}

// --- the crane lift motion ----------------------------------------------------

/// One frame of a crane lift. Positions are fractions of `s`; `dx`/`dy` offset
/// the carried part from its installed home.
pub(super) struct Lift {
    pub trolley_x: f32,
    pub hook_y: f32,
    pub dx: f32,
    pub dy: f32,
    pub carrying: bool,
    /// 0..1 touch-down dust kick (peaks right at landing).
    pub dust: f32,
}

/// The lift timeline for a part whose attach point installs at
/// (`target_x`, `attach_y`): the trolley carries it out from the mast (the
/// load pendulum-sways a little), lowers it home, touch-down with a dust kick,
/// then the empty hook winds back up while the trolley returns to park.
pub(super) fn lift(target_x: f32, attach_y: f32, t: f32) -> Lift {
    let t = t.clamp(0.0, 1.0);
    let hang_y = JIB_Y + CABLE_MIN; // attach point while traversing
    if t < 0.30 {
        let p = anim::ease_in_out_cubic(t / 0.30);
        let x = anim::lerp(TROLLEY_PARK, target_x, p);
        let sway = (t * 21.0).sin() * 0.030 * (1.0 - t / 0.30);
        Lift {
            trolley_x: x,
            hook_y: hang_y,
            dx: x + sway - target_x,
            dy: hang_y - attach_y,
            carrying: true,
            dust: 0.0,
        }
    } else if t < LIFT_LAND {
        let p = anim::ease_in_out_cubic((t - 0.30) / (LIFT_LAND - 0.30));
        let y = anim::lerp(hang_y, attach_y, p);
        let sway = ((t - 0.30) * 17.0).sin() * 0.012 * (1.0 - p);
        Lift { trolley_x: target_x, hook_y: y, dx: sway, dy: y - attach_y, carrying: true, dust: 0.0 }
    } else if t < 0.88 {
        // Touch-down: a tiny springy compress + the dust kick.
        let p = (t - LIFT_LAND) / (0.88 - LIFT_LAND);
        let dy = -(p * PI).sin() * 0.015 * (1.0 - p);
        Lift { trolley_x: target_x, hook_y: attach_y, dx: 0.0, dy, carrying: false, dust: 1.0 - p }
    } else {
        // Released: the hook winds up while the trolley heads back to park.
        let p = anim::ease_in_out_cubic((t - 0.88) / 0.12);
        Lift {
            trolley_x: anim::lerp(target_x, TROLLEY_PARK, p),
            hook_y: anim::lerp(attach_y - 0.06, HOOK_IDLE_Y, p),
            dx: 0.0,
            dy: 0.0,
            carrying: false,
            dust: 0.0,
        }
    }
}

// --- crane drawing -------------------------------------------------------------

/// The standing crane: footing, lattice mast, cab, tower head, jib +
/// counter-jib with its counterweight. The moving trolley/cable/hook are
/// drawn separately by [`crane_hoist`] so a carried part can sit between.
pub(super) fn crane_structure(cx: f32, base_y: f32, s: f32) {
    let lw = (0.016 * s).max(1.5);
    let thin = lw * 0.62;
    let xl = cx + (MAST_X - MAST_W / 2.0) * s;
    let xr = cx + (MAST_X + MAST_W / 2.0) * s;
    let jib_y = base_y + JIB_Y * s;
    let jib_top = jib_y - JIB_DEPTH * s;

    // Concrete footing block.
    rounded_rect(cx + (MAST_X - 0.075) * s, base_y - 0.035 * s, 0.15 * s, 0.055 * s, 0.012 * s, palette::CONCRETE);

    // Lattice mast: two rails + zigzag bracing.
    draw_line(xl, base_y, xl, jib_y, lw, palette::CRANE);
    draw_line(xr, base_y, xr, jib_y, lw, palette::CRANE);
    let step = 0.11 * s;
    let mut y = base_y - 0.02 * s;
    let mut left = true;
    while y - step > jib_y {
        let (x0, x1) = if left { (xl, xr) } else { (xr, xl) };
        draw_line(x0, y, x1, y - step, thin, palette::CRANE_DARK);
        y -= step;
        left = !left;
    }

    // Counter-jib + hanging counterweight (balances the working arm).
    let cj = cx + CJIB_END * s;
    draw_line(cx + MAST_X * s, jib_y, cj, jib_y, lw, palette::CRANE);
    rounded_rect(cj - 0.005 * s, jib_y + 0.012 * s, 0.085 * s, 0.10 * s, 0.012 * s, palette::CONCRETE);
    draw_rectangle_lines(cj - 0.005 * s, jib_y + 0.012 * s, 0.085 * s, 0.10 * s, thin, shade(palette::CONCRETE, 0.8));

    // Working jib: lattice of two chords + diagonals, out over the house.
    let je = cx + JIB_END * s;
    draw_line(xl, jib_y, je, jib_y, lw, palette::CRANE);
    draw_line(xl, jib_top, je - 0.025 * s, jib_top, lw * 0.9, palette::CRANE);
    draw_line(je - 0.025 * s, jib_top, je, jib_y, thin, palette::CRANE_DARK);
    let dstep = 0.10 * s;
    let mut x = xr + 0.01 * s;
    let mut up = true;
    while x + dstep < je - 0.02 * s {
        let (y0, y1) = if up { (jib_y, jib_top) } else { (jib_top, jib_y) };
        draw_line(x, y0, x + dstep, y1, thin, palette::CRANE_DARK);
        x += dstep;
        up = !up;
    }

    // Tower head + tie bars holding both arms.
    let apex = vec2(cx + MAST_X * s, base_y + HEAD_Y * s);
    draw_line(xl, jib_top, apex.x, apex.y, lw * 0.9, palette::CRANE);
    draw_line(xr, jib_top, apex.x, apex.y, lw * 0.9, palette::CRANE);
    draw_line(apex.x, apex.y, cx + JIB_END * 0.58 * s, jib_top, thin, palette::CRANE_DARK);
    draw_line(apex.x, apex.y, cj + 0.04 * s, jib_y, thin, palette::CRANE_DARK);

    // Operator cab tucked under the jib beside the mast (window toward work).
    rounded_rect(cx + (MAST_X + 0.045) * s, jib_y + 0.008 * s, 0.105 * s, 0.085 * s, 0.014 * s, palette::CRANE);
    rounded_rect(cx + (MAST_X + 0.085) * s, jib_y + 0.018 * s, 0.055 * s, 0.048 * s, 0.010 * s, palette::HOUSE_GLASS);
}

/// The crane's moving gear: trolley under the jib, hoist cable, hook block.
/// `trolley_x` / `hook_y` in fractions of `s` (hook_y = where a part's lift
/// point hangs).
pub(super) fn crane_hoist(cx: f32, base_y: f32, s: f32, trolley_x: f32, hook_y: f32) {
    let tx = cx + trolley_x * s;
    let jib_y = base_y + JIB_Y * s;
    let hy = base_y + hook_y * s;
    let cw = (0.013 * s).max(1.4);
    rounded_rect(tx - 0.030 * s, jib_y - 0.006 * s, 0.060 * s, 0.030 * s, 0.008 * s, palette::CRANE_DARK);
    draw_line(tx, jib_y + 0.018 * s, tx, hy - 0.052 * s, cw, CABLE);
    // Hook block + the open hook itself.
    rounded_rect(tx - 0.019 * s, hy - 0.072 * s, 0.038 * s, 0.032 * s, 0.008 * s, palette::CRANE_DARK);
    arc(tx, hy - 0.014 * s, (0.024 * s).max(2.2), 0.18, PI - 0.18, cw, CABLE);
}

// --- the foundation-stage machines ----------------------------------------------

/// The excavator, body centered at `x` (fraction of `s`). `bucket` is the dig
/// point (fractions relative to `(cx, base_y)`); the two-segment boom reaches
/// it with a simple raised-elbow bend. `alpha` fades the drive-in/out.
pub(super) fn excavator(cx: f32, base_y: f32, s: f32, x: f32, bucket: Option<Vec2>, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let a = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
    let bx = cx + x * s;
    // Tracks + road wheels.
    rounded_rect(bx - 0.15 * s, base_y - 0.075 * s, 0.30 * s, 0.075 * s, 0.037 * s, a(palette::MACHINE_TRACK));
    let wheel = a(mix(palette::MACHINE_TRACK, palette::WHITE, 0.45));
    for k in [-0.09f32, 0.0, 0.09] {
        disc(bx + k * s, base_y - 0.037 * s, 0.021 * s, wheel);
    }
    // Boom + bucket first so the body overlaps the shoulder joint.
    if let Some(b) = bucket {
        let sh = vec2(bx - 0.095 * s, base_y - 0.195 * s);
        let tip = vec2(cx + b.x * s, base_y + b.y * s);
        let d = tip - sh;
        let mut n = vec2(d.y, -d.x).normalize_or_zero();
        if n.y > 0.0 {
            n = -n;
        }
        let elbow = (sh + tip) * 0.5 + n * d.length() * 0.30;
        stroke_path(&[sh, elbow], 0.040 * s, a(palette::CRANE));
        stroke_path(&[elbow, tip + (elbow - tip).normalize_or_zero() * 0.02 * s], 0.030 * s, a(shade(palette::CRANE, 0.90)));
        // The bucket claw, curling back toward the machine.
        draw_triangle(tip, tip + vec2(0.065 * s, -0.055 * s), tip + vec2(0.085 * s, 0.005 * s), a(palette::MACHINE_TRACK));
    }
    // Body + cab (cab faces the boom).
    rounded_rect(bx - 0.13 * s, base_y - 0.185 * s, 0.26 * s, 0.115 * s, 0.022 * s, a(palette::CRANE));
    rounded_rect(bx - 0.13 * s, base_y - 0.275 * s, 0.115 * s, 0.105 * s, 0.018 * s, a(palette::CRANE));
    rounded_rect(bx - 0.122 * s, base_y - 0.265 * s, 0.062 * s, 0.058 * s, 0.012 * s, a(palette::HOUSE_GLASS));
}

/// The concrete mixer truck (cab left, drum behind — it backs toward the
/// trench, which is why it beeps). `spin` rotates the drum stripes; when
/// `chute_to` is set the chute swings out and concrete falls along it.
pub(super) fn mixer_truck(
    cx: f32,
    base_y: f32,
    s: f32,
    x: f32,
    spin: f32,
    chute_to: Option<Vec2>,
    alpha: f32,
) {
    if alpha <= 0.01 {
        return;
    }
    let a = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
    let tx = cx + x * s;
    // Chute + falling concrete go behind the drum.
    if let Some(to) = chute_to {
        let from = vec2(tx + 0.135 * s, base_y - 0.125 * s);
        let end = vec2(cx + to.x * s, base_y + to.y * s);
        let mid = from + (end - from) * 0.55;
        stroke_path(&[from, mid], (0.020 * s).max(2.0), a(palette::MACHINE_TRACK));
        // Concrete falling off the chute lip into the trench.
        for k in 0..3 {
            let p = ((spin * 1.7 + k as f32 / 3.0).fract()).clamp(0.0, 1.0);
            let drop = mid + (end - mid) * p;
            disc(drop.x, drop.y, 0.014 * s, a(palette::CONCRETE_WET));
        }
        fill_ellipse(end.x, end.y, 0.045 * s, 0.018 * s, 0.0, a(palette::CONCRETE_WET));
    }
    // Wheels + chassis.
    for k in [-0.11f32, 0.10] {
        disc(tx + k * s, base_y - 0.034 * s, 0.034 * s, a(palette::MACHINE_TRACK));
        disc(tx + k * s, base_y - 0.034 * s, 0.013 * s, a(mix(palette::MACHINE_TRACK, palette::WHITE, 0.5)));
    }
    draw_rectangle(tx - 0.18 * s, base_y - 0.088 * s, 0.35 * s, 0.030 * s, a(palette::MACHINE_TRACK));
    // Cab (left, direction of travel — it reverses in toward the trench).
    rounded_rect(tx - 0.18 * s, base_y - 0.20 * s, 0.105 * s, 0.115 * s, 0.018 * s, a(palette::HOUSE_DOOR));
    rounded_rect(tx - 0.172 * s, base_y - 0.192 * s, 0.055 * s, 0.055 * s, 0.012 * s, a(palette::HOUSE_GLASS));
    // The drum, tilted, with its rotating spiral stripes.
    let (dx, dy) = (tx + 0.05 * s, base_y - 0.155 * s);
    fill_ellipse(dx, dy, 0.095 * s, 0.062 * s, -16.0, a(palette::MIXER_DRUM));
    let rot = (-16.0f32).to_radians();
    let (rs, rc) = rot.sin_cos();
    for k in 0..3 {
        let p = (spin + k as f32 / 3.0).fract();
        let u = anim::lerp(-0.062, 0.062, p) * s; // along the drum axis
        let fade = (p * PI).sin();
        let cxs = dx + u * rc;
        let cys = dy + u * rs;
        let h = 0.045 * s * (1.0 - (p - 0.5).abs() * 0.8);
        draw_line(cxs - h * rs * 0.4, cys - h * rc, cxs + h * rs * 0.4, cys + h * rc, (0.016 * s).max(1.5), Color::new(palette::ACCENT.r, palette::ACCENT.g, palette::ACCENT.b, 0.75 * fade * alpha));
    }
    disc(tx + 0.138 * s, base_y - 0.175 * s, 0.020 * s, a(palette::CRANE_DARK));
}

/// The spoil heap the dig leaves beside the site (stays through the session —
/// real building sites keep their dirt pile). `grow` 0..1 scales it up.
pub(super) fn spoil_heap(cx: f32, base_y: f32, s: f32, grow: f32) {
    if grow <= 0.02 {
        return;
    }
    let r = 0.075 * s * grow.clamp(0.0, 1.0);
    fill_ellipse(cx + 0.55 * s, base_y - r * 0.30, r * 1.5, r * 0.85, 0.0, palette::DIRT);
    disc(cx + 0.51 * s, base_y - r * 0.75, r * 0.62, palette::DIRT);
    disc(cx + 0.59 * s, base_y - r * 0.65, r * 0.55, shade(palette::DIRT, 0.90));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lift starts hanging high at the park, touches down exactly at home
    /// at the land beat, and ends released with the trolley back at park.
    #[test]
    fn lift_lands_home_and_returns() {
        let (tx, ay) = (0.27, -1.12);
        let start = lift(tx, ay, 0.0);
        assert!(start.carrying && start.dy < -0.05, "starts hanging above home: {}", start.dy);
        assert!((start.trolley_x - TROLLEY_PARK).abs() < 1e-3);
        let land = lift(tx, ay, LIFT_LAND);
        assert!(land.dy.abs() < 1e-3 && land.dx.abs() < 1e-3, "lands at home");
        assert!(land.dust > 0.9, "dust kicks at touch-down");
        let done = lift(tx, ay, 1.0);
        assert!(!done.carrying && done.dx == 0.0 && done.dy == 0.0);
        assert!((done.trolley_x - TROLLEY_PARK).abs() < 1e-3, "trolley parks");
        assert!((done.hook_y - HOOK_IDLE_Y).abs() < 1e-3, "hook winds up");
    }

    /// The lower phase is monotonic: the hook only ever descends toward home.
    #[test]
    fn lift_lower_is_monotonic() {
        let (tx, ay) = (0.0, -0.42);
        let mut prev = lift(tx, ay, 0.30).hook_y;
        for i in 1..=20 {
            let t = 0.30 + (LIFT_LAND - 0.30) * i as f32 / 20.0;
            let y = lift(tx, ay, t).hook_y;
            assert!(y >= prev - 1e-4, "hook went back up at t={t}");
            prev = y;
        }
        assert!((prev - ay).abs() < 1e-3, "ends at the attach point");
    }

    /// Every stage has a positive duration and exactly one Thunk (the land
    /// beat the scene celebrates), with all cues inside the stage's timeline.
    #[test]
    fn install_tables_are_sane() {
        for stage in 0..6 {
            assert!(install_dur(stage) > 0.0);
            let cues = install_cues(stage);
            let thunks = cues.iter().filter(|(_, c)| *c == BuildCue::Thunk).count();
            assert_eq!(thunks, 1, "stage {stage}: exactly one Thunk");
            let mut prev = 0.0;
            for &(at, _) in cues {
                assert!(at > 0.0 && at < 1.0, "stage {stage}: cue at {at} out of range");
                assert!(at >= prev, "stage {stage}: cues out of order");
                prev = at;
            }
        }
    }

    /// The site silhouette stays inside the extents the layout reserves.
    #[test]
    fn extents_cover_the_crane() {
        let (l, r) = site_extents();
        assert!(l <= CJIB_END - 0.06, "counterweight inside the left extent");
        assert!(r >= JIB_END, "jib tip inside the right extent");
        // The spoil heap parks at 0.55 with lumps out to ~1.5× its radius.
        assert!(r >= 0.55 + 0.075 * 1.5, "spoil heap inside the right extent");
        assert!(site_height(1.0) >= -JIB_Y, "site height covers the jib");
    }
}
