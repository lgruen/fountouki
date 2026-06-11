//! The build-a-house — the tracing game's progress meter and finale set piece,
//! plus the demo pencil. Five parts go up over a five-letter session (walls,
//! door, windows, roof, chimney): each finished letter lowers the next part in
//! on a crane cable and taps it home. Pure vector, identical on every target.
use super::prim::{disc, fill_ellipse, mix, rounded_rect, shade, stroke_path};
use super::train::steam_puff;
use crate::{anim, palette};
use macroquad::prelude::*;

/// Number of build stages — matches the tracing session length so one session
/// finishes exactly one house.
pub const HOUSE_PARTS: usize = 5;

/// Animation state for one drawn house. The footprint width is `s`; a finished
/// build stands ~1.06·s above `base_y` (walls + roof + chimney).
#[derive(Clone, Copy)]
pub struct HousePose {
    /// Fully installed parts, `0..=HOUSE_PARTS`.
    pub parts: usize,
    /// 0..1 install animation for part `parts` (the next one); `None` = idle.
    pub installing: Option<f32>,
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
        HousePose { parts: 0, installing: None, door_open: 0.0, lit: [0.0, 0.0], smoke_t: -1.0, time: 0.0 }
    }
}

// Part geometry, in fractions of `s` relative to (cx, base_y).
const WALL_W: f32 = 0.78;
const WALL_H: f32 = 0.54;
const DOOR_W: f32 = 0.20;
const DOOR_H: f32 = 0.33;
const WIN_S: f32 = 0.17;
const WIN_DX: f32 = 0.245;
const WIN_CY: f32 = -0.36; // window center above base
const ROOF_APEX: f32 = -0.94;
const CHIM_X: f32 = 0.27;
const CHIM_W: f32 = 0.13;
const CHIM_TOP: f32 = -1.06;

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
pub fn house_part_anchor(cx: f32, base_y: f32, s: f32, part: usize) -> Vec2 {
    match part {
        0 => vec2(cx, base_y - WALL_H * s * 0.5),
        1 => vec2(cx, base_y - DOOR_H * s * 0.5),
        2 => vec2(cx, base_y + WIN_CY * s),
        3 => vec2(cx, base_y + (ROOF_APEX + 0.2) * s),
        _ => vec2(cx + CHIM_X * s, base_y + CHIM_TOP * s + 0.1 * s),
    }
}

/// Total height of the finished build above `base_y` (for layout fit checks).
pub fn house_height(s: f32) -> f32 {
    -CHIM_TOP * s
}

/// Install motion: the part rides a crane cable down from ~0.9·s above
/// (eased), then gets tapped home with a springy little hop + a dust kick.
/// Returns `(dy, cable_alpha, dust)` for install progress `t` in 0..1.
fn install_motion(t: f32, s: f32) -> (f32, f32, f32) {
    if t < 0.72 {
        let p = anim::ease_out_cubic(t / 0.72);
        (-(1.0 - p) * 0.9 * s, 1.0, 0.0)
    } else {
        let p = ((t - 0.72) / 0.28).clamp(0.0, 1.0);
        let dy = -(p * std::f32::consts::PI).sin() * 0.04 * s * (1.0 - p);
        (dy, (1.0 - p * 3.0).max(0.0), 1.0 - p)
    }
}

/// Draw the house at footprint center `cx`, ground line `base_y`, width `s`.
pub fn house(cx: f32, base_y: f32, s: f32, pose: &HousePose) {
    // The building site: a soft grass mound that's there before any part is —
    // so even letter one starts "somewhere".
    fill_ellipse(cx, base_y + 0.03 * s, 0.72 * s, 0.10 * s, 0.0, palette::HOUSE_GROUND);

    let upto = pose.parts.min(HOUSE_PARTS);

    // Blueprint ghost of the unbuilt silhouette (walls + roof) — the kid sees
    // the goal from letter one, and each install "fills in" the plan.
    if pose.installing.is_none() {
        let ghost = palette::hexa(0x2b2c34, 0.10);
        let lw = (0.018 * s).max(2.0);
        if upto < 1 {
            draw_rectangle_lines(cx - WALL_W * s / 2.0, base_y - WALL_H * s, WALL_W * s, WALL_H * s, lw, ghost);
        }
        if upto < 4 {
            let y_eave = base_y - WALL_H * s;
            draw_triangle_lines(
                vec2(cx - 0.50 * s, y_eave),
                vec2(cx + 0.50 * s, y_eave),
                vec2(cx, base_y + ROOF_APEX * s),
                lw,
                ghost,
            );
        }
    }
    for part in 0..upto {
        draw_part(cx, base_y, s, part, 0.0, pose);
    }
    if let Some(t) = pose.installing {
        if upto < HOUSE_PARTS {
            let (dy, cable, dust) = install_motion(t.clamp(0.0, 1.0), s);
            if cable > 0.0 {
                draw_cable(cx, base_y, s, upto, dy, cable);
            }
            draw_part(cx, base_y, s, upto, dy, pose);
            if dust > 0.0 {
                let a = house_part_anchor(cx, base_y, s, upto);
                let r = 0.10 * s;
                steam_puff(a.x - 0.16 * s, a.y + 0.10 * s, r * (1.0 + (1.0 - dust)), 0.55 * dust);
                steam_puff(a.x + 0.16 * s, a.y + 0.10 * s, r * (1.0 + (1.0 - dust)), 0.55 * dust);
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

/// The crane cable: a line dropping from high above to the part's lift point,
/// with a little hook + spreader bar at the attach end.
fn draw_cable(cx: f32, base_y: f32, s: f32, part: usize, dy: f32, alpha: f32) {
    let attach = match part {
        0 => vec2(cx, base_y - WALL_H * s),
        1 => vec2(cx, base_y - DOOR_H * s),
        2 => vec2(cx, base_y + (WIN_CY - WIN_S * 0.75) * s),
        3 => vec2(cx, base_y + ROOF_APEX * s),
        _ => vec2(cx + CHIM_X * s, base_y + CHIM_TOP * s),
    } + vec2(0.0, dy);
    let col = Color::new(0.42, 0.38, 0.33, 0.85 * alpha);
    // The cable runs from above the screen top (y=0) so the crane itself stays
    // out of frame — the part simply arrives "from the sky".
    draw_line(attach.x, -20.0, attach.x, attach.y - 0.05 * s, (0.018 * s).max(2.0), col);
    // Spreader bar for the window pair (one cable lifts both frames).
    if part == 2 {
        draw_line(cx - WIN_DX * s, attach.y, cx + WIN_DX * s, attach.y, (0.025 * s).max(2.5), col);
        for sx in [-1.0f32, 1.0] {
            let wx = cx + sx * WIN_DX * s;
            draw_line(wx, attach.y, wx, attach.y + 0.08 * s, (0.018 * s).max(2.0), col);
        }
    }
    // Hook: a small open arc under the cable end.
    super::prim::arc(
        attach.x,
        attach.y - 0.03 * s,
        (0.035 * s).max(3.0),
        0.2,
        std::f32::consts::PI - 0.2,
        (0.018 * s).max(2.0),
        col,
    );
}

fn draw_part(cx: f32, base_y: f32, s: f32, part: usize, dy: f32, pose: &HousePose) {
    match part {
        0 => draw_walls(cx, base_y, s, dy),
        1 => draw_door(cx, base_y, s, dy, pose.door_open),
        2 => {
            for (i, sx) in [-1.0f32, 1.0].iter().enumerate() {
                draw_window(cx + sx * WIN_DX * s, base_y + WIN_CY * s + dy, WIN_S * s, pose.lit[i]);
            }
        }
        3 => draw_roof(cx, base_y, s, dy),
        _ => draw_chimney(cx, base_y, s, dy),
    }
}

fn draw_walls(cx: f32, base_y: f32, s: f32, dy: f32) {
    let w = WALL_W * s;
    let h = WALL_H * s;
    let (x, y) = (cx - w / 2.0, base_y - h + dy);
    rounded_rect(x, y, w, h, 0.02 * s, palette::HOUSE_WALL);
    draw_rectangle_lines(x, y, w, h, (0.022 * s).max(2.5), palette::HOUSE_WALL_EDGE);
}

fn draw_door(cx: f32, base_y: f32, s: f32, dy: f32, open: f32) {
    let w = DOOR_W * s;
    let h = DOOR_H * s;
    let (x, y) = (cx - w / 2.0, base_y - h + dy);
    let r = 0.45 * w;
    // Frame (slightly proud of the leaf), then the dark interior when open.
    rounded_rect(x - 0.10 * w, y - 0.10 * w, w * 1.2, h + 0.10 * w, r * 1.1, palette::HOUSE_DOOR_DARK);
    if open > 0.02 {
        rounded_rect(x, y, w, h, r, palette::HOUSE_DOORWAY);
        // Warm light spilling out of the open doorway.
        let g = palette::HOUSE_GLASS_LIT;
        disc(cx, y + h * 0.55, w * (0.8 + 0.5 * open), Color::new(g.r, g.g, g.b, 0.30 * open));
    }
    // The leaf swings on its left hinge — drawn as a horizontal squeeze toward
    // the hinge with a shade ramp, which reads as a swing at toy scale.
    let leaf_w = w * (1.0 - 0.82 * open.clamp(0.0, 1.0));
    let leaf_col = mix(palette::HOUSE_DOOR, shade(palette::HOUSE_DOOR, 0.7), open * 0.8);
    rounded_rect(x, y, leaf_w, h, r * (leaf_w / w).max(0.3), leaf_col);
    // Knob rides the leading edge.
    disc(x + leaf_w * 0.82, y + h * 0.55, (0.016 * s).max(2.0), palette::GOLD);
}

fn draw_window(wx: f32, wy: f32, ws: f32, lit: f32) {
    let lit = lit.clamp(0.0, 1.0);
    if lit > 0.0 {
        let g = palette::HOUSE_GLASS_LIT;
        disc(wx, wy, ws * (0.9 + 0.5 * lit), Color::new(g.r, g.g, g.b, 0.35 * lit));
    }
    let half = ws / 2.0;
    rounded_rect(wx - half * 1.18, wy - half * 1.18, ws * 1.18, ws * 1.18, ws * 0.16, palette::WHITE);
    let glass = mix(palette::HOUSE_GLASS, palette::HOUSE_GLASS_LIT, lit);
    rounded_rect(wx - half * 0.92, wy - half * 0.92, ws * 0.92, ws * 0.92, ws * 0.10, glass);
    // Cross panes.
    let pw = (ws * 0.07).max(1.5);
    draw_line(wx - half * 0.92, wy, wx + half * 0.92, wy, pw, palette::WHITE);
    draw_line(wx, wy - half * 0.92, wx, wy + half * 0.92, pw, palette::WHITE);
}

fn draw_roof(cx: f32, base_y: f32, s: f32, dy: f32) {
    let y_eave = base_y - WALL_H * s + dy;
    let apex = vec2(cx, base_y + ROOF_APEX * s + dy);
    let l = vec2(cx - 0.50 * s, y_eave);
    let r = vec2(cx + 0.50 * s, y_eave);
    draw_triangle(l, r, apex, palette::HOUSE_ROOF);
    // Fascia along the eave + ridge lines for a crisp silhouette.
    let lw = (0.030 * s).max(3.0);
    stroke_path(&[l, r], lw, palette::HOUSE_ROOF_EDGE);
    stroke_path(&[l, apex, r], lw * 0.8, palette::HOUSE_ROOF_EDGE);
    // A little round attic window in the gable.
    let g = vec2(cx, y_eave - 0.16 * s);
    disc(g.x, g.y, 0.062 * s, palette::WHITE);
    disc(g.x, g.y, 0.046 * s, palette::HOUSE_GLASS);
}

fn draw_chimney(cx: f32, base_y: f32, s: f32, dy: f32) {
    let w = CHIM_W * s;
    let x = cx + CHIM_X * s - w / 2.0;
    let top = base_y + CHIM_TOP * s + dy;
    // The stack reaches down into the roof slope; the roof is drawn first so
    // the overlap reads as "set into the roof".
    let h = 0.34 * s;
    draw_rectangle(x, top, w, h, palette::HOUSE_BRICK);
    draw_rectangle_lines(x, top, w, h, (0.018 * s).max(2.0), shade(palette::HOUSE_BRICK, 0.78));
    // Cap lip.
    rounded_rect(x - 0.018 * s, top - 0.035 * s, w + 0.036 * s, 0.06 * s, 0.015 * s, shade(palette::HOUSE_BRICK, 0.85));
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

    /// The install ride starts ~0.9·s above home, ends parked at 0, and the
    /// dust only kicks in for the settle.
    #[test]
    fn install_motion_lands_home() {
        let s = 200.0;
        let (dy0, cable0, dust0) = install_motion(0.0, s);
        assert!(dy0 < -0.8 * s && cable0 == 1.0 && dust0 == 0.0);
        let (dy1, cable1, _dust1) = install_motion(1.0, s);
        assert!(dy1.abs() < 1e-3, "part must land at home: {dy1}");
        assert_eq!(cable1, 0.0);
        let (_, _, dust_mid) = install_motion(0.8, s);
        assert!(dust_mid > 0.0);
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
}
