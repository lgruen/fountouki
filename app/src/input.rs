//! Unified pointer (mouse or first touch) with press/release edges, held time,
//! long-press detection (500ms), and hit-testing. A completed long-press
//! suppresses the trailing tap. Built so a scripted Pointer can be injected for
//! deterministic play-tests.
use macroquad::prelude::*;

pub const LONG_PRESS_SECS: f32 = 0.5;

#[derive(Clone, Default)]
pub struct Pointer {
    pub pos: Vec2,
    pub down: bool,
    pub just_pressed: bool,
    pub just_released: bool,
    pub press_pos: Vec2,
    /// Seconds the current press has been held (0 when up).
    pub held: f32,
    /// True the single frame a held press crosses the long-press threshold.
    pub long_fired: bool,
    /// This release follows a long-press → suppress the synthetic tap.
    suppress_tap: bool,
}

impl Pointer {
    /// Evolve from the previous frame by reading macroquad (touch preferred).
    pub fn poll(prev: &Pointer, dt: f32) -> Pointer {
        let ts = touches();
        let (pos, down) = if let Some(t) = ts.first() {
            let active = matches!(
                t.phase,
                TouchPhase::Started | TouchPhase::Moved | TouchPhase::Stationary
            );
            (t.position, active)
        } else {
            let mp = mouse_position();
            (vec2(mp.0, mp.1), is_mouse_button_down(MouseButton::Left))
        };
        let just_pressed = down && !prev.down;
        let just_released = !down && prev.down;
        let press_pos = if just_pressed { pos } else { prev.press_pos };
        let held = if down {
            if just_pressed { 0.0 } else { prev.held + dt }
        } else {
            0.0
        };
        let was_long_prev = prev.down && prev.held >= LONG_PRESS_SECS;
        let is_long_now = down && held >= LONG_PRESS_SECS;
        let long_fired = is_long_now && !was_long_prev;
        let suppress_tap = just_released && was_long_prev;
        Pointer {
            pos,
            down,
            just_pressed,
            just_released,
            press_pos,
            held,
            long_fired,
            suppress_tap,
        }
    }

    /// A "tap" = released without travelling far and not following a long-press.
    pub fn tapped(&self) -> bool {
        self.just_released && (self.pos - self.press_pos).length() < 16.0 && !self.suppress_tap
    }
}

pub fn hit_circle(p: Vec2, cx: f32, cy: f32, r: f32) -> bool {
    (p - vec2(cx, cy)).length() <= r
}
pub fn hit_rect(p: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
}
