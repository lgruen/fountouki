//! Shared one-shot celebration effects the finale scenes fire on taps —
//! anything "surprise" that several games want (so the party language never
//! drifts between finales). Each effect is a pure draw parameterised by a
//! normalised progress `p` (0 at the tap, 1 when spent): the scenes own the
//! timers, this module owns the pixels.
use super::prim::{disc, star};
use crate::palette;
use macroquad::prelude::*;

/// A firework pop at `(cx, cy)`: a hot white core flash, then a ring of
/// `SPARKS` colored sparks flying outward (radius grows to `r`), each with a
/// bright center, all fading out by `p = 1`. Draw it every frame while the
/// effect's timer runs; a no-op outside 0..1.
pub fn firework(cx: f32, cy: f32, r: f32, p: f32, color: Color) {
    if !(0.0..1.0).contains(&p) {
        return;
    }
    // Ease the ring outward fast then coast (a decelerating shell).
    let ease = 1.0 - (1.0 - p) * (1.0 - p);
    let fade = 1.0 - p;
    // The opening core flash: a bright pop that dies in the first third.
    let core = (1.0 - p * 3.0).max(0.0);
    if core > 0.0 {
        disc(cx, cy, r * 0.22 * (0.4 + 0.6 * ease), Color::new(1.0, 1.0, 0.92, core));
    }
    const SPARKS: usize = 10;
    let pi = std::f32::consts::PI;
    for i in 0..SPARKS {
        let a = i as f32 * (2.0 * pi / SPARKS as f32) + 0.35; // fixed offset: never axis-aligned
        // Alternate near/far sparks so the shell reads round, not spoked.
        let rr = r * ease * if i % 2 == 0 { 1.0 } else { 0.78 };
        let sx = cx + a.cos() * rr;
        let sy = cy + a.sin() * rr;
        let pr = r * 0.09 * (0.6 + 0.4 * fade);
        disc(sx, sy, pr, Color { a: fade, ..color });
        disc(sx, sy, pr * 0.45, Color::new(1.0, 1.0, 0.95, fade));
    }
}

/// A twinkle pop: a little gold star that spins up + fades — a softer sibling
/// of [`firework`] for quiet scenes (the clock's night meadow). No-op outside
/// 0..1.
pub fn twinkle_pop(cx: f32, cy: f32, r: f32, p: f32, color: Color) {
    if !(0.0..1.0).contains(&p) {
        return;
    }
    let fade = 1.0 - p;
    let grow = 0.5 + 0.5 * (1.0 - (1.0 - p) * (1.0 - p));
    disc(cx, cy, r * 1.15 * grow, Color { a: fade * 0.25, ..color });
    star(cx, cy, r * grow, Color { a: fade, ..color });
    star(cx, cy, r * grow * 0.5, Color::new(1.0, 1.0, 0.95, fade));
}

/// A shooting star streaking from `(x, y)` toward `dir` (unit-ish vector):
/// a bright head + a fading tapered tail of discs, traveling `dist` over the
/// effect. For the clock finale's night sky. No-op outside 0..1.
pub fn shooting_star(x: f32, y: f32, dir: Vec2, dist: f32, r: f32, p: f32) {
    if !(0.0..1.0).contains(&p) {
        return;
    }
    let fade = 1.0 - p;
    let head = vec2(x, y) + dir * dist * p;
    // The tail: discs trailing the head, shrinking + fading toward the back.
    const TAIL: usize = 8;
    for i in 0..TAIL {
        let back = i as f32 / TAIL as f32;
        let c = head - dir * dist * 0.22 * back;
        let a = fade * (1.0 - back) * 0.8;
        disc(c.x, c.y, r * (1.0 - 0.7 * back), palette::hexa(0xfff3c4, a));
    }
    star(head.x, head.y, r * 1.6, Color { a: fade, ..palette::GOLD });
    disc(head.x, head.y, r * 0.7, Color::new(1.0, 1.0, 0.95, fade));
}
