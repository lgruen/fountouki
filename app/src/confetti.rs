//! Celebratory confetti burst — a native particle system (the old app used a
//! canvas routine). Physics from docs/port-spec/audio-fx.md: upward fan, gravity
//! 600 px/s², ~1.2s life with alpha fade, rotated chips in the rainbow palette.
use crate::palette;
use fountouki_core::rng::Mulberry32;
use macroquad::prelude::*;

const GRAVITY: f32 = 600.0;
const LIFE: f32 = 1.2;
const COLORS: [u32; 6] = [0xff4d6d, 0xff8c42, 0xffd166, 0x2bd5a0, 0x38b3e2, 0xb364e5];

struct Chip {
    p: Vec2,
    v: Vec2,
    rot: f32,
    vrot: f32,
    size: f32,
    color: Color,
    life: f32,
}

pub struct Confetti {
    chips: Vec<Chip>,
    rng: Mulberry32,
    /// Separate stream for the steady `rain`, so a celebratory drizzle can't
    /// perturb the deterministic `burst` sequence (keeps goldens reproducible).
    rain_rng: Mulberry32,
}

impl Confetti {
    pub fn new(seed: u32) -> Confetti {
        Confetti {
            chips: Vec::new(),
            rng: Mulberry32::new(seed),
            rain_rng: Mulberry32::new(seed ^ 0x9e37_79b9),
        }
    }

    pub fn burst(&mut self, at: Vec2, n: usize, spread_x: f32) {
        for _ in 0..n {
            let color = palette::hex(COLORS[self.rng.below(COLORS.len())]);
            self.chips.push(Chip {
                p: vec2(at.x + self.rng.range(-spread_x, spread_x), at.y),
                v: vec2(self.rng.range(-220.0, 220.0), self.rng.range(-360.0, -180.0)),
                rot: self.rng.range(0.0, std::f32::consts::TAU),
                vrot: self.rng.range(-6.0, 6.0),
                size: self.rng.range(6.0, 10.0),
                color,
                life: LIFE,
            });
        }
    }

    /// Gentle celebratory rain: spawn `n` chips spread across the top edge
    /// `[0,width]`, drifting *down* (vs `burst`'s upward fan). Longer-lived so
    /// they fall most of the screen before fading — for a sustained finale
    /// trickle, call a few each frame.
    pub fn rain(&mut self, width: f32, top_y: f32, n: usize) {
        for _ in 0..n {
            let r = &mut self.rain_rng;
            let color = palette::hex(COLORS[r.below(COLORS.len())]);
            self.chips.push(Chip {
                p: vec2(r.range(0.0, width), top_y - r.range(0.0, 40.0)),
                v: vec2(r.range(-50.0, 50.0), r.range(40.0, 130.0)),
                rot: r.range(0.0, std::f32::consts::TAU),
                vrot: r.range(-5.0, 5.0),
                size: r.range(6.0, 11.0),
                color,
                life: LIFE * r.range(1.6, 2.6),
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        let dt = dt.min(0.05);
        for c in self.chips.iter_mut() {
            c.v.y += GRAVITY * dt;
            c.p += c.v * dt;
            c.rot += c.vrot * dt;
            c.life -= dt;
        }
        self.chips.retain(|c| c.life > 0.0);
    }

    pub fn draw(&self) {
        for c in &self.chips {
            let a = c.life.clamp(0.0, 1.0);
            let col = Color::new(c.color.r, c.color.g, c.color.b, a);
            rotated_rect(c.p, c.size, c.size * 0.6, c.rot, col);
        }
    }

    pub fn active(&self) -> bool {
        !self.chips.is_empty()
    }
}

fn rotated_rect(center: Vec2, w: f32, h: f32, rot: f32, color: Color) {
    let (s, c) = rot.sin_cos();
    let (hw, hh) = (w / 2.0, h / 2.0);
    let corner = |x: f32, y: f32| center + vec2(x * c - y * s, x * s + y * c);
    let p0 = corner(-hw, -hh);
    let p1 = corner(hw, -hh);
    let p2 = corner(hw, hh);
    let p3 = corner(-hw, hh);
    draw_triangle(p0, p1, p2, color);
    draw_triangle(p0, p2, p3, color);
}
