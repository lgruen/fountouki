//! Time-driven easing + tween helpers. The app's motion signature is a springy
//! overshoot (the old CSS cubic-bezier(0.34,1.6,0.64,1)); `back_out` approximates
//! it. Everything is driven by an explicit time/dt so golden frames are
//! deterministic (no real clock).
use std::f32::consts::TAU;

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}
pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u
}
pub fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = -2.0 * t + 2.0;
        1.0 - u * u * u / 2.0
    }
}
/// Springy "back out" overshoot (rises past 1.0 then settles) — the win-feedback feel.
pub fn back_out(t: f32) -> f32 {
    let c1 = 1.70158_f32;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}
/// Sine pulse in [-1, 1] for idle/pulse loops (pink slot, frog breathing).
pub fn pulse(time: f32, period: f32) -> f32 {
    (time * TAU / period).sin()
}

/// A one-shot interpolation from `from` to `to` over `dur` seconds.
#[derive(Clone)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub dur: f32,
    pub elapsed: f32,
    pub ease: fn(f32) -> f32,
}
impl Tween {
    pub fn new(from: f32, to: f32, dur: f32, ease: fn(f32) -> f32) -> Self {
        Self { from, to, dur, elapsed: 0.0, ease }
    }
    pub fn update(&mut self, dt: f32) {
        self.elapsed = (self.elapsed + dt).min(self.dur);
    }
    pub fn value(&self) -> f32 {
        let t = if self.dur <= 0.0 { 1.0 } else { self.elapsed / self.dur };
        lerp(self.from, self.to, (self.ease)(clamp01(t)))
    }
    pub fn done(&self) -> bool {
        self.elapsed >= self.dur
    }
}
