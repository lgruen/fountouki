//! The build-a-house — the tracing game's progress meter and finale set piece,
//! plus the demo pencil. Six parts go up over a six-letter session in real
//! construction order — foundation → brick walls → roof → chimney → windows →
//! door — so the meter doubles as a how-houses-get-built picture book: the
//! digger digs and the mixer pours the slab, the bricks rise course by course
//! (openings left for the door and windows), and the site's tower crane lifts
//! everything else in. Pure vector, identical on every target. The crane +
//! machines + install timing live in `site.rs`.
use super::prim::{arc, disc, fill_ellipse, mix, rounded_rect, shade, stroke_path};
use super::site;
use super::train::steam_puff;
use crate::{anim, palette};
use macroquad::prelude::*;
use std::f32::consts::PI;

/// Number of build stages — matches `core::tracing::SESSION_GOAL` so one
/// session finishes exactly one house.
pub const HOUSE_PARTS: usize = 6;

/// Animation state for one drawn house. The footprint width is `s`; a finished
/// build stands ~1.06·s above `base_y` (the site's crane reaches higher — see
/// [`site::site_height`]).
#[derive(Clone, Copy)]
pub struct HousePose {
    /// Fully installed parts, `0..=HOUSE_PARTS`.
    pub parts: usize,
    /// 0..1 install animation for part `parts` (the next one); `None` = idle.
    pub installing: Option<f32>,
    /// Show the construction site (crane, spoil heap, blueprint ghost) —
    /// in-play yes; the finale shows the finished house with the site cleared.
    pub site: bool,
    /// Door swing 0..1 (0 = shut) — the finale's tappable door.
    pub door_open: f32,
    /// Per-window lamp warmth 0..1 (left, right) — the finale's tappable lights.
    pub lit: [f32; 2],
    /// Seconds the chimney has been smoking; negative = no smoke.
    pub smoke_t: f32,
    /// Ambient clock (smoke drift wiggle).
    pub time: f32,
}

impl Default for HousePose {
    fn default() -> Self {
        HousePose {
            parts: 0,
            installing: None,
            site: false,
            door_open: 0.0,
            lit: [0.0, 0.0],
            smoke_t: -1.0,
            time: 0.0,
        }
    }
}

// Part geometry, in fractions of `s` relative to (cx, base_y).
const WALL_W: f32 = 0.78;
const WALL_H: f32 = 0.54;
/// Brick courses in the walls (course height = WALL_H / COURSES).
const COURSES: usize = 7;
const BRICK_L: f32 = 0.155;
const MORTAR: f32 = 0.010;
const DOOR_W: f32 = 0.20;
const DOOR_H: f32 = 0.33;
/// The door's rough opening in the brickwork (slightly larger than the leaf).
const DOOR_OPEN_H: f32 = 0.36;
const WIN_S: f32 = 0.17;
const WIN_DX: f32 = 0.245;
const WIN_CY: f32 = -0.36; // window center above base
const ROOF_APEX: f32 = -0.94;
const CHIM_X: f32 = 0.27;
const CHIM_W: f32 = 0.13;
const CHIM_TOP: f32 = -1.06;
/// Foundation slab: wider than the walls (a visible footing), dug below the
/// ground line.
const SLAB_W: f32 = 0.88;
const SLAB_D: f32 = 0.05;
/// Mixer-truck park / approach x. Its tail reaches 0.18 further left — the
/// card side, so the whole transit must stay inside the site extents.
const MIXER_PARK: f32 = -0.58;
const MIXER_IN: f32 = -0.66;

/// Door hit rect (finale tap target), slightly inflated for small fingers.
pub fn house_door_rect(cx: f32, base_y: f32, s: f32) -> Rect {
    let w = DOOR_W * s * 1.6;
    let h = DOOR_H * s * 1.25;
    Rect::new(cx - w / 2.0, base_y - h, w, h)
}

/// Window centers (left, right) — finale tap targets.
pub fn house_window_centers(cx: f32, base_y: f32, s: f32) -> [Vec2; 2] {
    [
        vec2(cx - WIN_DX * s, base_y + WIN_CY * s),
        vec2(cx + WIN_DX * s, base_y + WIN_CY * s),
    ]
}

/// Where part `part` lands — confetti/dust anchor for the install moment.
/// Order: slab, walls, roof, chimney, windows, door.
pub fn house_part_anchor(cx: f32, base_y: f32, s: f32, part: usize) -> Vec2 {
    match part {
        0 => vec2(cx, base_y - 0.02 * s),
        1 => vec2(cx, base_y - WALL_H * s * 0.5),
        2 => vec2(cx, base_y + (ROOF_APEX + 0.2) * s),
        3 => vec2(cx + CHIM_X * s, base_y + (CHIM_TOP + 0.10) * s),
        4 => vec2(cx, base_y + WIN_CY * s),
        _ => vec2(cx, base_y - DOOR_H * s * 0.5),
    }
}

/// Total height of the finished build above `base_y` (for layout fit checks);
/// the standing crane is taller — use [`site::site_height`] for the full site.
pub fn house_height(s: f32) -> f32 {
    -CHIM_TOP * s
}

/// Crane-lift spec for the lifted parts: (target x, installed attach-point y),
/// both fractions of `s`. The attach point rides the hook; slings drop from it
/// to the part.
fn lift_spec(part: usize) -> (f32, f32) {
    match part {
        2 => (0.0, ROOF_APEX - 0.05),   // truss, slung by the ridge
        3 => (CHIM_X, CHIM_TOP - 0.06), // chimney top
        4 => (0.0, WIN_CY - 0.16),      // spreader bar over the window pair
        _ => (0.0, -DOOR_OPEN_H - 0.06), // door head
    }
}

/// Draw the house at footprint center `cx`, ground line `base_y`, width `s`.
pub fn house(cx: f32, base_y: f32, s: f32, pose: &HousePose) {
    // The building site: a soft grass mound that's there before any part is —
    // so even letter one starts "somewhere".
    fill_ellipse(cx, base_y + 0.03 * s, 0.72 * s, 0.10 * s, 0.0, palette::HOUSE_GROUND);

    let upto = pose.parts.min(HOUSE_PARTS);
    let inst = pose.installing.map(|t| t.clamp(0.0, 1.0));
    let crane_busy = inst.is_some() && (2..HOUSE_PARTS).contains(&upto);

    if pose.site {
        site::crane_structure(cx, base_y, s);
        if !crane_busy {
            site::crane_hoist(cx, base_y, s, site::TROLLEY_PARK, site::HOOK_IDLE_Y);
        }
        if upto >= 1 {
            site::spoil_heap(cx, base_y, s, 1.0);
        }
        // Blueprint ghost of the unbuilt silhouette (walls + roof) — the kid
        // sees the goal from letter one, and each stage "fills in" the plan.
        if inst.is_none() {
            ghost(cx, base_y, s, upto);
        }
    }

    if upto >= 1 {
        draw_foundation(cx, base_y, s, 1.0, 1.0, 0.0);
    }
    for part in 1..upto {
        draw_part(cx, base_y, s, part, pose);
    }
    if let Some(t) = inst {
        if upto < HOUSE_PARTS {
            match upto {
                0 => install_foundation(cx, base_y, s, t),
                1 => install_walls(cx, base_y, s, t),
                _ => install_lift(cx, base_y, s, upto, t),
            }
        }
    }

    // Chimney smoke, once the house is whole: lazy puffs drifting up-right.
    if upto >= HOUSE_PARTS && pose.smoke_t >= 0.0 {
        let tip = vec2(cx + CHIM_X * s, base_y + CHIM_TOP * s);
        let cad = 0.85;
        let life = 2.0;
        let kmax = (pose.smoke_t / cad).floor() as i32;
        let kmin = (((pose.smoke_t - life) / cad).ceil() as i32).max(0);
        for k in kmin..=kmax {
            let age = pose.smoke_t - k as f32 * cad;
            if !(0.0..=life).contains(&age) {
                continue;
            }
            let a = age / life;
            let wiggle = ((pose.time + k as f32 * 1.7) * 1.4).sin() * 0.05 * s;
            steam_puff(
                tip.x + 0.10 * s * age + wiggle,
                tip.y - 0.22 * s * age,
                0.07 * s * (1.0 + a * 1.6),
                0.7 * (1.0 - a),
            );
        }
    }
}

/// Faint outline of the goal silhouette over the unbuilt stages.
fn ghost(cx: f32, base_y: f32, s: f32, upto: usize) {
    let g = palette::hexa(0x2b2c34, 0.10);
    let lw = (0.018 * s).max(2.0);
    if upto < 2 {
        draw_rectangle_lines(cx - WALL_W * s / 2.0, base_y - WALL_H * s, WALL_W * s, WALL_H * s, lw, g);
    }
    if upto < 3 {
        let y_eave = base_y - WALL_H * s;
        draw_triangle_lines(
            vec2(cx - 0.50 * s, y_eave),
            vec2(cx + 0.50 * s, y_eave),
            vec2(cx, base_y + ROOF_APEX * s),
            lw,
            g,
        );
    }
}

/// A finished part (the install animations draw their own moving versions).
fn draw_part(cx: f32, base_y: f32, s: f32, part: usize, pose: &HousePose) {
    match part {
        1 => draw_brick_wall(cx, base_y, s, COURSES as f32),
        2 => draw_roof(cx, base_y, s, 0.0, 0.0, 1.0),
        3 => draw_chimney(cx, base_y, s, 0.0, 0.0),
        4 => {
            for (i, sx) in [-1.0f32, 1.0].iter().enumerate() {
                draw_window(cx + sx * WIN_DX * s, base_y + WIN_CY * s, WIN_S * s, pose.lit[i], true);
            }
        }
        _ => {
            draw_door(cx, base_y, s, 0.0, 0.0, pose.door_open);
            draw_door_step(cx, base_y, s);
        }
    }
}

// --- stage 0: the foundation ---------------------------------------------------

/// Trench + slab. `trench`/`slab` reveal left→right (the dig and the pour both
/// travel that way); `wet` 1→0 cures the concrete from dark to set.
fn draw_foundation(cx: f32, base_y: f32, s: f32, trench: f32, slab: f32, wet: f32) {
    let trench = trench.clamp(0.0, 1.0);
    if trench <= 0.0 {
        return;
    }
    let wl = cx - SLAB_W / 2.0 * s;
    let d = SLAB_D * s;
    draw_rectangle(wl, base_y - 0.004 * s, SLAB_W * s * trench, d, palette::DIRT_DARK);
    let slab = slab.clamp(0.0, 1.0);
    if slab > 0.0 {
        // The pour stands a little proud of the trench so the fresh concrete
        // reads as a light band against both the dirt and the grass.
        let c = mix(palette::CONCRETE, palette::CONCRETE_WET, wet.clamp(0.0, 1.0));
        draw_rectangle(wl + 0.006 * s, base_y - 0.012 * s, (SLAB_W - 0.012) * s * slab, d + 0.008 * s, c);
    }
    if slab >= 1.0 && wet < 0.6 {
        // The set slab stands a little proud: the footing line under the walls.
        rounded_rect(wl, base_y - 0.014 * s, SLAB_W * s, 0.030 * s, 0.008 * s, palette::CONCRETE);
    }
}

/// The foundation install: the excavator backs in and digs the trench in two
/// scoops (the spoil heap grows beside the site), drives off, then the mixer
/// truck reverses in and pours the slab.
fn install_foundation(cx: f32, base_y: f32, s: f32, t: f32) {
    let dig_p = ((t - 0.14) / 0.36).clamp(0.0, 1.0);
    let pour_p = ((t - 0.70) / 0.22).clamp(0.0, 1.0);
    let set_p = ((t - 0.92) / 0.08).clamp(0.0, 1.0);
    draw_foundation(cx, base_y, s, dig_p, pour_p, 1.0 - set_p);
    site::spoil_heap(cx, base_y, s, dig_p);

    // Excavator: in from the right, two scoops left→right, off to the right.
    let (ex, ea) = if t < 0.12 {
        (anim::lerp(0.78, 0.55, anim::ease_out_cubic(t / 0.12)), (t / 0.06).min(1.0))
    } else if t < 0.52 {
        (0.55, 1.0)
    } else {
        let p = ((t - 0.52) / 0.12).clamp(0.0, 1.0);
        (anim::lerp(0.55, 0.78, p), 1.0 - p)
    };
    if ea > 0.0 {
        let bucket = if (0.14..0.52).contains(&t) {
            let bx = anim::lerp(-0.40, 0.35, dig_p);
            // Two scoops: the bucket bites the ground at each cycle boundary
            // and lifts high between them.
            let lift01 = ((dig_p * 2.0).fract() * PI).sin();
            vec2(bx, 0.04 - 0.22 * lift01)
        } else {
            vec2(0.30, -0.16) // boom tucked for the drive
        };
        site::excavator(cx, base_y, s, ex, Some(bucket), ea);
    }

    // Mixer truck: reverses in from the left (beep beep), pours, pulls away.
    // The approach stays a short scoot — its tail must never cross the card.
    if t >= 0.56 {
        let (mx, ma) = if t < 0.70 {
            (anim::lerp(MIXER_IN, MIXER_PARK, anim::ease_out_cubic((t - 0.56) / 0.14)), ((t - 0.56) / 0.07).min(1.0))
        } else if t < 0.92 {
            (MIXER_PARK, 1.0)
        } else {
            let p = ((t - 0.92) / 0.08).clamp(0.0, 1.0);
            (anim::lerp(MIXER_PARK, MIXER_IN, p), 1.0 - p)
        };
        let chute = (0.70..0.92).contains(&t).then_some(vec2(-0.38, 0.005));
        site::mixer_truck(cx, base_y, s, mx, t * 1.3, chute, ma);
    }

    // The slab sets with a little dust kick.
    if set_p > 0.0 && set_p < 1.0 {
        let d = 1.0 - set_p;
        steam_puff(cx - 0.24 * s, base_y - 0.02 * s, 0.08 * s * (1.0 + set_p), 0.5 * d);
        steam_puff(cx + 0.24 * s, base_y - 0.02 * s, 0.08 * s * (1.0 + set_p), 0.5 * d);
    }
}

// --- stage 1: the brick walls ----------------------------------------------------

/// The wall openings in the brickwork (door + both windows), in px.
fn wall_openings(cx: f32, base_y: f32, s: f32) -> [Rect; 3] {
    [
        Rect::new(cx - 0.12 * s, base_y - DOOR_OPEN_H * s, 0.24 * s, DOOR_OPEN_H * s),
        Rect::new(cx - WIN_DX * s - 0.10 * s, base_y + WIN_CY * s - 0.10 * s, 0.20 * s, 0.20 * s),
        Rect::new(cx + WIN_DX * s - 0.10 * s, base_y + WIN_CY * s - 0.10 * s, 0.20 * s, 0.20 * s),
    ]
}

/// The brick walls at `courses` (0..=COURSES, fractional = the next course
/// dropping in): mortar backing, running-bond bricks with cut bricks around
/// the openings, and — once topped out — lintels over every opening.
fn draw_brick_wall(cx: f32, base_y: f32, s: f32, courses: f32) {
    let courses = courses.clamp(0.0, COURSES as f32);
    let n_full = courses.floor() as usize;
    let frac = courses - n_full as f32;
    let ch = WALL_H / COURSES as f32 * s;
    let wl = cx - WALL_W / 2.0 * s;
    let ww = WALL_W * s;
    let h_built = n_full as f32 * ch;
    if n_full > 0 {
        draw_rectangle(wl, base_y - h_built, ww, h_built, palette::HOUSE_MORTAR);
        // Unfitted openings read as the dark inside of the build.
        for o in wall_openings(cx, base_y, s) {
            let y0 = o.y.max(base_y - h_built);
            let y1 = (o.y + o.h).min(base_y);
            if y1 > y0 {
                draw_rectangle(o.x, y0, o.w, y1 - y0, palette::HOUSE_INSIDE);
            }
        }
    }
    for row in 0..n_full {
        draw_course(cx, base_y, s, row, 0.0, 1.0);
    }
    if frac > 0.0 && n_full < COURSES {
        let drop = -(1.0 - anim::ease_out_cubic(frac)) * 0.06 * s;
        draw_course(cx, base_y, s, n_full, drop, (frac * 3.0).min(1.0));
    }
    if courses >= COURSES as f32 {
        draw_rectangle_lines(wl, base_y - WALL_H * s, ww, WALL_H * s, (0.014 * s).max(1.5), palette::HOUSE_WALL_EDGE);
        // Concrete lintels bridging the openings.
        for o in wall_openings(cx, base_y, s) {
            rounded_rect(o.x - 0.012 * s, o.y - 0.026 * s, o.w + 0.024 * s, 0.026 * s, 0.006 * s, palette::CONCRETE);
        }
    }
}

/// One brick course (`row` 0 = bottom), running bond, bricks cut around the
/// openings. `dy` offsets the dropping-in course; `alpha` fades it in.
fn draw_course(cx: f32, base_y: f32, s: f32, row: usize, dy: f32, alpha: f32) {
    let ch = WALL_H / COURSES as f32 * s;
    let y1 = base_y - row as f32 * ch + dy;
    let bh = ch - MORTAR * s;
    let by = y1 - ch + MORTAR * s * 0.5;
    let wl = cx - WALL_W / 2.0 * s;
    let wr = cx + WALL_W / 2.0 * s;
    let bl = BRICK_L * s;
    let g = MORTAR * s;
    let band_mid = y1 - ch * 0.5;
    // An opening cuts this course when it covers the course's centerline.
    // Fixed-size scratch (≤3 cuts → ≤4 segments per brick): this runs every
    // frame for the rest of the session, so no per-brick heap allocation.
    let mut cuts = [(0.0f32, 0.0f32); 3];
    let mut ncuts = 0;
    for o in wall_openings(cx, base_y, s) {
        if o.y < band_mid && o.y + o.h > band_mid {
            cuts[ncuts] = (o.x, o.x + o.w);
            ncuts += 1;
        }
    }
    let mut bx = wl + if row % 2 == 1 { -bl * 0.5 } else { 0.0 };
    let mut col = 0usize;
    while bx < wr - 1.0 {
        let (x0, x1) = (bx.max(wl), (bx + bl - g).min(wr));
        // Each brick gets a deterministic shade so the wall reads as brickwork.
        let k = 0.94 + 0.10 * (((row * 31 + col * 17) % 7) as f32 / 6.0);
        let c = shade(palette::HOUSE_WALL, k);
        let c = Color::new(c.r, c.g, c.b, alpha);
        let mut segs = [(x0, x1), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)];
        let mut nseg = 1;
        for &(ox0, ox1) in &cuts[..ncuts] {
            let mut out = [(0.0f32, 0.0f32); 4];
            let mut nout = 0;
            for &(a, b) in &segs[..nseg] {
                if ox1 <= a || ox0 >= b {
                    out[nout] = (a, b);
                    nout += 1;
                    continue;
                }
                if ox0 > a {
                    out[nout] = (a, ox0);
                    nout += 1;
                }
                if ox1 < b {
                    out[nout] = (ox1, b);
                    nout += 1;
                }
            }
            segs = out;
            nseg = nout;
        }
        for &(a, b) in &segs[..nseg] {
            if b - a >= 0.018 * s {
                draw_rectangle(a, by, b - a, bh, c);
            }
        }
        bx += bl;
        col += 1;
    }
}

/// The wall install: courses rise bottom-up like time-lapse bricklaying, with
/// a dust kick when the last course tops out.
fn install_walls(cx: f32, base_y: f32, s: f32, t: f32) {
    // The top course lands right on the Thunk cue (0.88, see site::S1).
    let courses = ((t - 0.06) / 0.82 * COURSES as f32).clamp(0.0, COURSES as f32);
    draw_brick_wall(cx, base_y, s, courses);
    if t >= 0.88 {
        let d = ((t - 0.88) / 0.12).clamp(0.0, 1.0);
        if d < 1.0 {
            let y = base_y - WALL_H * s;
            steam_puff(cx - 0.30 * s, y + 0.06 * s, 0.08 * s * (1.0 + d), 0.5 * (1.0 - d));
            steam_puff(cx + 0.30 * s, y + 0.06 * s, 0.08 * s * (1.0 + d), 0.5 * (1.0 - d));
        }
    }
}

// --- stages 2..5: the crane lifts -------------------------------------------------

/// A crane-lift install: the trolley carries the part out, lowers it home
/// (with slings to the part), touch-down dust, then the hook winds back. The
/// roof stage compresses its lift so the tile rows can sweep up afterwards.
fn install_lift(cx: f32, base_y: f32, s: f32, part: usize, t: f32) {
    let (lift_t, tiles_t) = if part == 2 {
        ((t / site::ROOF_LIFT_END).min(1.0), ((t - 0.68) / 0.28).clamp(0.0, 1.0))
    } else {
        (t, 0.0)
    };
    let (txf, ayf) = lift_spec(part);
    let l = site::lift(txf, ayf, lift_t);
    let (dx, dy) = (l.dx * s, l.dy * s);

    match part {
        2 => {
            // Rafters first, then the tiles sweep up over them.
            draw_truss(cx, base_y, s, dx, dy);
            if tiles_t > 0.0 {
                draw_roof(cx, base_y, s, 0.0, 0.0, tiles_t);
            }
        }
        3 => draw_chimney(cx, base_y, s, dx, dy),
        4 => {
            for sx in [-1.0f32, 1.0] {
                draw_window(cx + sx * WIN_DX * s + dx, base_y + WIN_CY * s + dy, WIN_S * s, 0.0, !l.carrying);
            }
        }
        _ => draw_door(cx, base_y, s, dx, dy, 0.0),
    }

    if l.carrying {
        // Slings from the hook down to the part's lift points.
        let hx = cx + l.trolley_x * s;
        let hy = base_y + l.hook_y * s;
        let sl = (0.012 * s).max(1.2);
        let sling = |p: Vec2| draw_line(hx, hy, p.x, p.y, sl, site::CABLE);
        match part {
            2 => {
                for sx in [-1.0f32, 1.0] {
                    sling(vec2(cx + sx * 0.25 * s + dx, base_y + (ROOF_APEX + 0.20) * s + dy));
                }
            }
            3 => {
                for sx in [-1.0f32, 1.0] {
                    sling(vec2(cx + (CHIM_X + sx * 0.055) * s + dx, base_y + CHIM_TOP * s + dy));
                }
            }
            4 => {
                // Spreader bar: one hook lifts both window frames.
                draw_line(cx - WIN_DX * s + dx, hy, cx + WIN_DX * s + dx, hy, (0.020 * s).max(2.0), site::CABLE);
                for sx in [-1.0f32, 1.0] {
                    let wx = cx + sx * WIN_DX * s + dx;
                    draw_line(wx, hy, wx, base_y + (WIN_CY - 0.10) * s + dy, sl, site::CABLE);
                }
            }
            _ => {
                for sx in [-1.0f32, 1.0] {
                    sling(vec2(cx + sx * 0.085 * s + dx, base_y - (DOOR_H + 0.02) * s + dy));
                }
            }
        }
    }

    if l.dust > 0.0 {
        let a = house_part_anchor(cx, base_y, s, part);
        let grow = 1.0 - l.dust;
        steam_puff(a.x - 0.14 * s, a.y + 0.08 * s, 0.09 * s * (1.0 + grow), 0.55 * l.dust);
        steam_puff(a.x + 0.14 * s, a.y + 0.08 * s, 0.09 * s * (1.0 + grow), 0.55 * l.dust);
    }

    site::crane_hoist(cx, base_y, s, l.trolley_x, l.hook_y);
}

// --- the parts -----------------------------------------------------------------

/// The bare roof truss: rafters, bottom chord, king post + struts — what the
/// crane actually lifts; the tiles cover it afterwards.
fn draw_truss(cx: f32, base_y: f32, s: f32, dx: f32, dy: f32) {
    let y_eave = base_y - WALL_H * s + dy;
    let apex = vec2(cx + dx, base_y + ROOF_APEX * s + dy);
    let l = vec2(cx - 0.50 * s + dx, y_eave);
    let r = vec2(cx + 0.50 * s + dx, y_eave);
    let lw = (0.030 * s).max(2.5);
    stroke_path(&[l, apex], lw, palette::HOUSE_RAFTER);
    stroke_path(&[r, apex], lw, palette::HOUSE_RAFTER);
    stroke_path(&[l, r], lw * 0.85, palette::HOUSE_RAFTER);
    let inner = shade(palette::HOUSE_RAFTER, 0.86);
    stroke_path(&[vec2(cx + dx, y_eave), apex], lw * 0.7, inner);
    for sx in [-1.0f32, 1.0] {
        stroke_path(
            &[
                vec2(cx + sx * 0.08 * s + dx, y_eave),
                vec2(cx + sx * 0.25 * s + dx, base_y + (ROOF_APEX + 0.20) * s + dy),
            ],
            lw * 0.6,
            inner,
        );
    }
}

/// The tiled roof. `tiles` 0..1 sweeps the terracotta rows up from the eave;
/// at 1 the fascia, ridge and attic window finish it off.
fn draw_roof(cx: f32, base_y: f32, s: f32, dx: f32, dy: f32, tiles: f32) {
    let f = tiles.clamp(0.0, 1.0);
    if f <= 0.0 {
        return;
    }
    let y_eave = base_y - WALL_H * s + dy;
    let apex_y = base_y + ROOF_APEX * s + dy;
    let hh = y_eave - apex_y;
    let half = |y: f32| 0.50 * s * ((y - apex_y) / hh).max(0.0);
    let y_r = y_eave - hh * f;
    let bl = vec2(cx - 0.50 * s + dx, y_eave);
    let br = vec2(cx + 0.50 * s + dx, y_eave);
    let tl = vec2(cx - half(y_r) + dx, y_r);
    let tr = vec2(cx + half(y_r) + dx, y_r);
    draw_triangle(bl, br, tr, palette::HOUSE_ROOF);
    draw_triangle(bl, tr, tl, palette::HOUSE_ROOF);
    // Tile courses: scalloped rows, half-step offset like the brick bond.
    let edge = shade(palette::HOUSE_ROOF, 0.82);
    let lw_t = (0.012 * s).max(1.2);
    let rows = 5;
    for k in 0..rows {
        let y_k = y_eave - hh * (k as f32 + 0.55) / rows as f32;
        if y_k < y_r {
            break;
        }
        let w = half(y_k) - 0.025 * s;
        let step = 0.085 * s;
        let r_t = step * 0.5;
        if w <= r_t {
            continue;
        }
        let mut x = -w + if k % 2 == 1 { step * 0.5 } else { 0.0 };
        while x <= w - r_t {
            arc(cx + x + dx, y_k, r_t, 0.0, PI, lw_t, edge);
            x += step;
        }
    }
    if f >= 1.0 {
        let apex = vec2(cx + dx, apex_y);
        let lw = (0.030 * s).max(3.0);
        stroke_path(&[bl, br], lw, palette::HOUSE_ROOF_EDGE);
        stroke_path(&[bl, apex, br], lw * 0.8, palette::HOUSE_ROOF_EDGE);
        // A little round attic window in the gable.
        let gw = vec2(cx + dx, y_eave - 0.16 * s);
        disc(gw.x, gw.y, 0.062 * s, palette::WHITE);
        disc(gw.x, gw.y, 0.046 * s, palette::HOUSE_GLASS);
    }
}

fn draw_chimney(cx: f32, base_y: f32, s: f32, dx: f32, dy: f32) {
    let w = CHIM_W * s;
    let x = cx + CHIM_X * s - w / 2.0 + dx;
    let top = base_y + CHIM_TOP * s + dy;
    // The stack reaches down into the roof slope; the roof is drawn first so
    // the overlap reads as "set into the roof".
    let h = 0.34 * s;
    draw_rectangle(x, top, w, h, palette::HOUSE_BRICK);
    // Mortar joints: bed lines + staggered head joints, like the walls.
    let joint = shade(palette::HOUSE_BRICK, 0.80);
    let jw = (0.010 * s).max(1.0);
    for k in 0..4 {
        let y0 = top + h * k as f32 / 4.0;
        let y1 = top + h * (k as f32 + 1.0) / 4.0;
        if k > 0 {
            draw_line(x, y0, x + w, y0, jw, joint);
        }
        let hx = x + w * if k % 2 == 0 { 0.36 } else { 0.64 };
        draw_line(hx, y0, hx, y1, jw, joint);
    }
    draw_rectangle_lines(x, top, w, h, (0.018 * s).max(2.0), joint);
    // Cap lip.
    rounded_rect(x - 0.018 * s, top - 0.035 * s, w + 0.036 * s, 0.06 * s, 0.015 * s, shade(palette::HOUSE_BRICK, 0.85));
}

fn draw_window(wx: f32, wy: f32, ws: f32, lit: f32, sill: bool) {
    let lit = lit.clamp(0.0, 1.0);
    if lit > 0.0 {
        let g = palette::HOUSE_GLASS_LIT;
        disc(wx, wy, ws * (0.9 + 0.5 * lit), Color::new(g.r, g.g, g.b, 0.35 * lit));
    }
    let half = ws / 2.0;
    if sill {
        rounded_rect(wx - half * 1.44, wy + half * 1.18, ws * 1.44, ws * 0.13, ws * 0.05, shade(palette::WHITE, 0.93));
    }
    rounded_rect(wx - half * 1.18, wy - half * 1.18, ws * 1.18, ws * 1.18, ws * 0.16, palette::WHITE);
    let glass = mix(palette::HOUSE_GLASS, palette::HOUSE_GLASS_LIT, lit);
    rounded_rect(wx - half * 0.92, wy - half * 0.92, ws * 0.92, ws * 0.92, ws * 0.10, glass);
    // Cross panes.
    let pw = (ws * 0.07).max(1.5);
    draw_line(wx - half * 0.92, wy, wx + half * 0.92, wy, pw, palette::WHITE);
    draw_line(wx, wy - half * 0.92, wx, wy + half * 0.92, pw, palette::WHITE);
}

fn draw_door(cx: f32, base_y: f32, s: f32, dx: f32, dy: f32, open: f32) {
    let w = DOOR_W * s;
    let h = DOOR_H * s;
    let (x, y) = (cx - w / 2.0 + dx, base_y - h + dy);
    let r = 0.45 * w;
    // Frame fills the rough opening, then the dark interior when open.
    rounded_rect(x - 0.10 * w, y - 0.15 * w, w * 1.2, h + 0.15 * w, r * 1.1, palette::HOUSE_DOOR_DARK);
    if open > 0.02 {
        rounded_rect(x, y, w, h, r, palette::HOUSE_DOORWAY);
        // Warm light spilling out of the open doorway.
        let g = palette::HOUSE_GLASS_LIT;
        disc(cx + dx, y + h * 0.55, w * (0.8 + 0.5 * open), Color::new(g.r, g.g, g.b, 0.30 * open));
    }
    // The leaf swings on its left hinge — drawn as a horizontal squeeze toward
    // the hinge with a shade ramp, which reads as a swing at toy scale.
    let leaf_w = w * (1.0 - 0.82 * open.clamp(0.0, 1.0));
    let leaf_col = mix(palette::HOUSE_DOOR, shade(palette::HOUSE_DOOR, 0.7), open * 0.8);
    rounded_rect(x, y, leaf_w, h, r * (leaf_w / w).max(0.3), leaf_col);
    // Two inset panels while the leaf is (mostly) shut.
    if open < 0.3 {
        let pa = (1.0 - open / 0.3) * 0.8;
        let pc = shade(palette::HOUSE_DOOR, 0.82);
        let pc = Color::new(pc.r, pc.g, pc.b, pa);
        let plw = (0.010 * s).max(1.2);
        draw_rectangle_lines(x + leaf_w * 0.20, y + h * 0.12, leaf_w * 0.60, h * 0.30, plw, pc);
        draw_rectangle_lines(x + leaf_w * 0.20, y + h * 0.52, leaf_w * 0.60, h * 0.34, plw, pc);
    }
    // Knob rides the leading edge.
    disc(x + leaf_w * 0.82, y + h * 0.55, (0.016 * s).max(2.0), palette::GOLD);
}

/// The concrete doorstep — the finishing touch that lands with the door.
fn draw_door_step(cx: f32, base_y: f32, s: f32) {
    rounded_rect(cx - 0.14 * s, base_y - 0.006 * s, 0.28 * s, 0.038 * s, 0.010 * s, palette::CONCRETE);
}

/// The demo pencil: a chunky kid's pencil leaning up-right at a writing angle,
/// its graphite point exactly on (`tip_x`, `tip_y`) — so the ink visibly comes
/// from a pencil during the watch demo, not from an abstract dot.
pub fn pencil(tip_x: f32, tip_y: f32, len: f32) {
    let dir = vec2(0.585, -0.811); // unit, up-right
    let perp = vec2(-dir.y, dir.x);
    let at = |d: f32| vec2(tip_x + dir.x * d * len, tip_y + dir.y * d * len);
    let w = 0.16 * len;
    let wood = palette::hex(0xeec98f);
    let body = palette::hex(0xffc94d);
    // Sharpened wood cone, then the graphite point on top of it.
    let shoulder = at(0.18);
    draw_triangle(at(0.0), shoulder + perp * (w * 0.5), shoulder - perp * (w * 0.5), wood);
    let neck = at(0.07);
    draw_triangle(at(0.0), neck + perp * (w * 0.20), neck - perp * (w * 0.20), palette::INK);
    // Body (round-capped capsule) + a lighter facet stripe.
    stroke_path(&[at(0.20), at(0.80)], w, body);
    stroke_path(&[at(0.22) + perp * (w * 0.26), at(0.78) + perp * (w * 0.26)], w * 0.28, palette::hex(0xffdf8a));
    // Ferrule + eraser.
    stroke_path(&[at(0.82), at(0.87)], w * 1.02, palette::hex(0xbcc6cf));
    stroke_path(&[at(0.90), at(0.96)], w * 0.94, palette::ACCENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tap targets must sit inside the build: the door rect and both windows
    /// land within the house footprint and above the ground line.
    #[test]
    fn hit_targets_inside_footprint() {
        let (cx, base_y, s) = (500.0, 600.0, 200.0);
        let d = house_door_rect(cx, base_y, s);
        assert!(d.x > cx - s * 0.5 && d.x + d.w < cx + s * 0.5);
        assert!(d.y > base_y - house_height(s) && d.y + d.h <= base_y + 1.0);
        for w in house_window_centers(cx, base_y, s) {
            assert!(w.x > cx - s * 0.5 && w.x < cx + s * 0.5);
            assert!(w.y < base_y && w.y > base_y - WALL_H * s);
        }
    }

    /// Every part's anchor sits within the finished silhouette.
    #[test]
    fn part_anchors_inside_house() {
        let (cx, base_y, s) = (0.0, 0.0, 100.0);
        for part in 0..HOUSE_PARTS {
            let a = house_part_anchor(cx, base_y, s, part);
            assert!(a.x.abs() <= s * 0.5, "part {part} anchor x: {}", a.x);
            assert!(a.y <= 0.0 && a.y >= -house_height(s), "part {part} anchor y: {}", a.y);
        }
    }

    /// Every lifted part's attach point hangs under the jib's reach and above
    /// its install height, so the lift always lowers into place.
    #[test]
    fn lift_specs_are_reachable() {
        for part in 2..HOUSE_PARTS {
            let (tx, ay) = lift_spec(part);
            assert!(tx.abs() <= 0.46, "part {part}: target {tx} beyond the jib");
            assert!(ay < 0.0 && ay > site::JIB_Y, "part {part}: attach {ay} out of range");
        }
    }

    /// The mixer truck's whole approach (tail included) stays inside the
    /// site's left extent — that side faces the tracing card, not a screen
    /// edge, so the transit must never overdraw it.
    #[test]
    fn mixer_transit_inside_the_site() {
        let (l, _) = super::super::site::site_extents();
        assert!(MIXER_IN - 0.18 >= l, "mixer tail {} past extent {l}", MIXER_IN - 0.18);
    }

    /// The wall openings (door + windows) sit inside the wall slab, and the
    /// fitted door/windows cover them.
    #[test]
    fn openings_inside_the_wall() {
        let (cx, base_y, s) = (0.0, 0.0, 100.0);
        let wall = Rect::new(cx - WALL_W * s / 2.0, base_y - WALL_H * s, WALL_W * s, WALL_H * s);
        for o in wall_openings(cx, base_y, s) {
            assert!(o.x >= wall.x && o.x + o.w <= wall.x + wall.w, "opening x out of wall");
            assert!(o.y >= wall.y && o.y + o.h <= wall.y + wall.h + 0.01, "opening y out of wall");
        }
        // The window pair lands centered on its openings.
        let wins = house_window_centers(cx, base_y, s);
        let ops = wall_openings(cx, base_y, s);
        for (w, o) in wins.iter().zip(&ops[1..]) {
            assert!((w.x - (o.x + o.w / 2.0)).abs() < 0.5);
            assert!((w.y - (o.y + o.h / 2.0)).abs() < 0.5);
        }
    }
}
