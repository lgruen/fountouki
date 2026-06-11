//! Patterns finale — the "Pattern Train". A golden-hour dusk arrival: a friendly
//! steam engine driven by the frog mascot, pulling cars that carry the kid's
//! just-solved pattern, to a checkered finish flag. Reuses the phonics reward's
//! frog (same character) but in its own scene (travel + arrival vs jumping).
use super::frog::{frog, FrogPose};
use super::prim::{arc, disc, fill_ellipse, rounded_rect, soft_shadow, star, stroke_path};
use crate::palette;
use macroquad::prelude::*;

/// A pose for the engine body (scoot/bob/squash). Scales about the track base.
#[derive(Clone, Copy)]
pub struct EnginePose {
    pub dx: f32,
    pub dy: f32,
    pub sx: f32,
    pub sy: f32,
}
impl Default for EnginePose {
    fn default() -> Self {
        EnginePose { dx: 0.0, dy: 0.0, sx: 1.0, sy: 1.0 }
    }
}

/// A spoked train wheel: INK rim, colored face, cream spokes, gold hub. `ang`
/// (radians) rotates the spokes — for roll-without-slip pass `-x / r`.
pub fn train_wheel(cx: f32, cy: f32, r: f32, ang: f32, face: Color, spoke: Color) {
    disc(cx, cy, r, palette::INK);
    disc(cx, cy, r * 0.82, face);
    let w = (r * 0.13).max(2.0);
    for k in 0..4 {
        let a = ang + k as f32 * std::f32::consts::FRAC_PI_4;
        let (s, c) = a.sin_cos();
        stroke_path(
            &[vec2(cx - c * r * 0.74, cy - s * r * 0.74), vec2(cx + c * r * 0.74, cy + s * r * 0.74)],
            w,
            spoke,
        );
    }
    disc(cx, cy, r * 0.24, palette::GOLD);
    arc(cx, cy, r * 0.9, -0.7, 0.5, (r * 0.1).max(1.5), Color::new(1.0, 1.0, 1.0, 0.45));
}

/// Engine bounding box (the generous tap target) for a boiler radius `R` whose
/// base (where the wheels meet the track) is at `(ex, by)`. Shared by the scene
/// hit-test and the cross-device guard test, so geometry lives in one place.
pub fn engine_hit_rect(ex: f32, by: f32, r_boiler: f32) -> Rect {
    // Tall enough to include the funnel + the frog's head poking up at the cab
    // window — a generous, forgiving tap target, and the guard test then also
    // guarantees its apex clears the viewport top.
    // A touch wider than the loco so a re-tap during a forward chuff-scoot still lands.
    Rect::new(ex - r_boiler * 2.2, by - r_boiler * 3.1, r_boiler * 4.6, r_boiler * 3.1)
}

/// Where steam leaves the funnel (boiler radius `R`, base at `(ex, by)`).
pub fn engine_funnel_tip(ex: f32, by: f32, r_boiler: f32) -> Vec2 {
    vec2(ex + r_boiler * 0.95, by - r_boiler * 2.85)
}

/// The steam engine + its frog driver, base (wheels on track) at `(ex, by)`.
/// `r_boiler` (== 2× wheel radius) sets the scale; `ep` scoots/bobs/squashes the
/// whole loco about the base; `wheel_ang` spins the spokes; `headlamp` 0..1 adds
/// glow; `cond` poses the frog mascot leaning out of the cab window.
pub fn train_engine(
    ex: f32,
    by: f32,
    r_boiler: f32,
    ep: EnginePose,
    wheel_ang: f32,
    headlamp: f32,
    cond: FrogPose,
) {
    let r = r_boiler;
    let wr = r * 0.5; // small wheel radius
    let pt = |lx: f32, ly: f32| vec2(ex + ep.dx + lx * ep.sx, by + ep.dy + ly * ep.sy);
    let red = palette::ENGINE_RED;
    let red_d = palette::ENGINE_RED_DARK;
    let face = red_d;
    let spoke = palette::CARD;

    // Contact shadow.
    fill_ellipse(ex + ep.dx, by + ep.dy + 0.06 * r, r * 1.95, 0.16 * r, 0.0, Color::new(0.1, 0.08, 0.12, 0.18));

    // Wheels (behind the bodies). Rear small, driving big, front small.
    let rear = pt(-r * 1.35, -wr);
    train_wheel(rear.x, rear.y, wr, wheel_ang, face, spoke);
    let front = pt(r * 0.95, -wr);
    train_wheel(front.x, front.y, wr, wheel_ang, face, spoke);
    let drive = pt(-r * 0.25, -r * 0.7);
    train_wheel(drive.x, drive.y, r * 0.7, wheel_ang, face, spoke);

    // Cowcatcher wedge at the front.
    draw_triangle(pt(r * 1.4, -0.9 * r), pt(r * 2.1, -0.08 * r), pt(r * 1.4, -0.08 * r), palette::hex(0x3a3140));

    // Boiler (horizontal rounded cylinder) + dark front face.
    let bw = r * 2.6;
    let bh = r * 1.3;
    let bcx = pt(r * 0.2, -r * 1.45);
    rounded_rect(bcx.x - bw / 2.0, bcx.y - bh / 2.0, bw, bh, bh * 0.5, red);
    let bf = pt(r * 1.5, -r * 1.45);
    disc(bf.x, bf.y, r * 0.65, red_d);
    // Boiler bands.
    for &lx in &[-r * 0.5, r * 0.5] {
        let a = pt(lx, -r * 1.45 - bh / 2.0);
        let b = pt(lx, -r * 1.45 + bh / 2.0);
        stroke_path(&[a, b], (r * 0.06).max(2.0), red_d);
    }

    // Cab + roof + window (glass), with the frog driver leaning out.
    let cw = r * 1.6;
    let ch = r * 1.95;
    let ccx = pt(-r * 1.25, -r * 1.65);
    rounded_rect(ccx.x - cw / 2.0, ccx.y - ch / 2.0, cw, ch, r * 0.3, red);
    let roof = pt(-r * 1.25, -r * 2.72);
    rounded_rect(roof.x - r * 1.0, roof.y - r * 0.18, r * 2.0, r * 0.42, r * 0.18, red_d);
    let win = pt(-r * 1.25, -r * 1.95);
    rounded_rect(win.x - r * 0.62, win.y - r * 0.55, r * 1.24, r * 1.1, r * 0.22, palette::SKY_DUSK_MID);
    // The frog mascot (same character as the phonics reward) drives the train,
    // sized + seated to fill the glass with its head near the top; the sill
    // stroke below then reads it as leaning out of the window.
    let frog_c = pt(-r * 1.25, -r * 1.9);
    frog(frog_c.x, frog_c.y, r * 0.56, palette::RAINBOW[3], cond);
    // Window frame over the lower sill so the frog reads as leaning out.
    let sill_l = pt(-r * 1.25 - r * 0.62, -r * 1.4);
    let sill_r = pt(-r * 1.25 + r * 0.62, -r * 1.4);
    stroke_path(&[sill_l, sill_r], (r * 0.12).max(2.0), red_d);

    // Funnel (widening smokestack) + lip.
    let fb = r * 2.1; // funnel base height (boiler top)
    let ftop = r * 2.85;
    let bl = pt(r * 0.95 - r * 0.28, -fb);
    let br = pt(r * 0.95 + r * 0.28, -fb);
    let tl = pt(r * 0.95 - r * 0.42, -ftop);
    let tr = pt(r * 0.95 + r * 0.42, -ftop);
    draw_triangle(bl, br, tr, red_d);
    draw_triangle(bl, tr, tl, red_d);
    let lip = pt(r * 0.95, -ftop);
    rounded_rect(lip.x - r * 0.48, lip.y - r * 0.1, r * 0.96, r * 0.2, r * 0.1, palette::GOLD);

    // Brass dome on the boiler — toward the boiler centre so it never crowds the
    // frog driver's face (which sits off to the cab/left).
    let dome = pt(r * 0.3, -fb + r * 0.05);
    fill_ellipse(dome.x, dome.y, r * 0.3, r * 0.26, 0.0, palette::GOLD);

    // Headlamp + a warm concentric glow (reads lit even idle; flares on tap).
    let lamp = pt(r * 1.18, -r * 1.5);
    let hl = headlamp.clamp(0.0, 1.0);
    disc(lamp.x, lamp.y, r * 0.62, palette::hexa(0xfff0b8, 0.16 + 0.4 * hl));
    disc(lamp.x, lamp.y, r * 0.42, palette::hexa(0xfff0b8, 0.42 + 0.5 * hl));
    disc(lamp.x, lamp.y, r * 0.24, palette::LIGHT_GLOW);
    disc(lamp.x, lamp.y, r * 0.1, palette::WHITE);

    // A little star badge on the cab.
    let badge = pt(-r * 1.25, -r * 1.05);
    star(badge.x, badge.y, r * 0.22, palette::GOLD);
}

/// One train car chassis (no item): body + two PLAIN muted wheels + a coupling
/// stub. The caller places the pattern item in a `draw_cell` seat at the body
/// center. Car wheels are deliberately simpler/smaller than the engine's spoked
/// ones so the pattern cards stay the dominant rhythm along the bottom.
pub fn train_car_chassis(body: Rect, by: f32, wheel_r: f32) {
    soft_shadow(body.x, body.y, body.w, body.h, body.h * 0.24);
    rounded_rect(body.x, body.y, body.w, body.h, body.h * 0.24, palette::CAR_BODY);
    // Warm under-rim so the cream car reads against the cream-ish sky.
    rounded_rect(body.x, body.y + body.h - body.h * 0.16, body.w, body.h * 0.16, body.h * 0.12, palette::hexa(0xe7c9a0, 0.7));
    let cx = body.x + body.w / 2.0;
    let cwr = wheel_r * 0.8;
    let cyw = by - cwr;
    let maroon = Color::new(0.55, 0.31, 0.35, 1.0);
    for dx in [-body.w * 0.28, body.w * 0.28] {
        disc(cx + dx, cyw, cwr, palette::INK);
        disc(cx + dx, cyw, cwr * 0.78, maroon);
        disc(cx + dx, cyw, cwr * 0.26, palette::GOLD);
    }
    // Coupling stub toward the engine (right).
    disc(body.x + body.w + wheel_r * 0.2, by - wheel_r * 1.4, wheel_r * 0.24, palette::RAIL);
}

/// A soft steam puff: a few overlapping warm-white discs at `alpha`.
pub fn steam_puff(cx: f32, cy: f32, r: f32, alpha: f32) {
    let c = Color::new(palette::STEAM.r, palette::STEAM.g, palette::STEAM.b, alpha.clamp(0.0, 1.0));
    disc(cx, cy, r, c);
    disc(cx - r * 0.55, cy + r * 0.3, r * 0.7, c);
    disc(cx + r * 0.58, cy + r * 0.22, r * 0.66, c);
    disc(cx + r * 0.08, cy - r * 0.52, r * 0.6, c);
}

/// A waving checkered finish flag on a post rising from the track at `by`. The
/// flag (width `w`, height `h`) flutters via `time`; a gold star tops the post.
pub fn checker_flag(pole_x: f32, by: f32, flag_top: f32, w: f32, h: f32, time: f32) {
    let pole_w = (w * 0.09).clamp(4.0, 12.0);
    rounded_rect(pole_x - pole_w / 2.0, flag_top, pole_w, by - flag_top, pole_w / 2.0, palette::RAIL);
    star(pole_x, flag_top - h * 0.22, h * 0.2, palette::GOLD);
    // Flag hangs to the LEFT of the post (toward the arriving train).
    let cols = 6usize;
    let rows = 4usize;
    let cw = w / cols as f32;
    let chh = h / rows as f32;
    let x0 = pole_x - w;
    for r in 0..rows {
        for c in 0..cols {
            let wave = (time * 1.8 + c as f32 * 0.55).sin() * h * 0.06 * (c as f32 / cols as f32);
            let col = if (r + c) % 2 == 0 { palette::INK } else { palette::CARD };
            draw_rectangle(x0 + c as f32 * cw, flag_top + r as f32 * chh + wave, cw + 1.0, chh + 1.0, col);
        }
    }
}

/// A festive bunting swag of triangular pennants strung between `x0..x1` at top
/// edge `y`, dipping by `sag` at center; pennants cycle the rainbow + flutter.
pub fn bunting(x0: f32, x1: f32, y: f32, sag: f32, n: usize, time: f32) {
    const SEG: usize = 40;
    let yat = |t: f32| y + sag * 4.0 * t * (1.0 - t);
    let mut line = Vec::with_capacity(SEG + 1);
    for i in 0..=SEG {
        let t = i as f32 / SEG as f32;
        line.push(vec2(x0 + (x1 - x0) * t, yat(t)));
    }
    stroke_path(&line, 3.0, palette::hexa(0x6f5a4a, 0.8));
    let span = h_span(n);
    let dx = x1 - x0;
    for i in 0..n {
        let t = (i as f32 + 0.5) / n as f32;
        let x = x0 + dx * t;
        let yy = yat(t);
        // Rotate each pennant to the string's local tangent so its top edge sits
        // flush on the swag and it hangs perpendicular to it — on the sloped
        // sections the un-rotated (horizontal-topped) triangles floated off the
        // string by one corner. Slope = d(yat)/dx; dy(yat)/dt = sag*4*(1-2t).
        let slope = sag * 4.0 * (1.0 - 2.0 * t);
        let (sn, cs) = slope.atan2(dx).sin_cos();
        let rot = |lx: f32, ly: f32| vec2(x + lx * cs - ly * sn, yy + lx * sn + ly * cs);
        let flutter = (time * 2.0 + i as f32 * 0.7).sin() * 0.08;
        let s = span;
        let col = palette::RAINBOW[i % 7];
        draw_triangle(rot(-s, 0.0), rot(s, 0.0), rot(flutter * s, s * 2.2), col);
    }
}

fn h_span(n: usize) -> f32 {
    // Pennant half-width: shrink a touch as the count grows so they don't touch.
    (220.0 / (n.max(1) as f32)).clamp(8.0, 22.0)
}
