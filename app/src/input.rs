//! Unified pointer over macroquad's mouse API (touch is mirrored onto the mouse
//! in logical coordinates), with press/release edges, held time,
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
    /// Evolve from the previous frame. Reads ONLY the mouse API: macroquad
    /// mirrors touch onto the mouse in **logical** (screen_width) coordinates
    /// (`simulate_mouse_with_touch`, on by default), while raw `touches()`
    /// positions come back in **physical/DPR** pixels. Mixing the two is what
    /// broke hit-testing on mobile — taps landed at a ~DPR× offset, missing
    /// every target. The mouse path is the same space the layout uses.
    pub fn poll(prev: &Pointer, dt: f32) -> Pointer {
        let mp = mouse_position();
        let pos = vec2(mp.0, mp.1);
        let down = is_mouse_button_down(MouseButton::Left);
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

/// Minimum spacing between two accepted taps. A single physical press can stutter
/// into two `tapped()` edges on some touch stacks; below this gap they collapse
/// into one. ~150ms is far under a deliberate double-tap yet above any jitter.
pub const TAP_DEBOUNCE_S: f32 = 0.15;

/// A reusable PER-TARGET tap debounce: gate every consumed tap through
/// [`accept`] so one physical press never double-registers as two taps on the
/// SAME target — while a fast tap on a *different* target lands immediately
/// (it's not a bounce). Driven off `ctx.time`, which in interactive play is the
/// wall clock (`get_time()` in `main.rs`), exactly what a stutter/bounce filter
/// wants; captures/play-tests inject `time` explicitly, so it's deterministic
/// there. Hold one per logical tap region (e.g. the whole Sing Back choir +
/// replay + finale corners share one, keyed by a distinct id per target).
///
/// [`accept`]: TapDebounce::accept
#[derive(Clone, Copy)]
pub struct TapDebounce {
    /// Time of the last accepted tap (`-∞` until the first accept, so the very
    /// first tap always passes regardless of the starting clock).
    last: f32,
    /// Which target id the last accepted tap hit (only meaningful once `last`
    /// is finite). A different target this frame is never a bounce.
    last_target: u32,
}

impl TapDebounce {
    pub fn new() -> Self {
        TapDebounce { last: f32::NEG_INFINITY, last_target: u32::MAX }
    }

    /// Accept a tap on `target` at time `now`: returns true (and records
    /// `now`+`target`) unless it's a near-instant re-fire of the SAME target
    /// within [`TAP_DEBOUNCE_S`] — i.e. the spurious second edge of one physical
    /// press. A tap on a *different* target always lands (it can't be a bounce
    /// of the previous one), so a fast distinct-pad tap is never swallowed.
    pub fn accept(&mut self, target: u32, now: f32) -> bool {
        if target != self.last_target || now - self.last >= TAP_DEBOUNCE_S {
            self.last = now;
            self.last_target = target;
            true
        } else {
            false
        }
    }
}

impl Default for TapDebounce {
    fn default() -> Self {
        TapDebounce::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_debounce_swallows_double_press() {
        let mut d = TapDebounce::new();
        // First tap always accepts.
        assert!(d.accept(0, 10.0));
        // An immediate second edge of the SAME target is a bounce → swallowed.
        assert!(!d.accept(0, 10.0 + TAP_DEBOUNCE_S * 0.5));
        // A genuine later tap on the same target, past the window, accepts again.
        assert!(d.accept(0, 10.0 + TAP_DEBOUNCE_S + 0.01));
    }

    #[test]
    fn tap_debounce_allows_immediate_different_target() {
        let mut d = TapDebounce::new();
        // First tap on target 0 accepts.
        assert!(d.accept(0, 10.0));
        // A near-instant tap on a DIFFERENT target is NOT a bounce → accepts,
        // even well inside the debounce window (a fast distinct-pad tap).
        assert!(d.accept(1, 10.0 + TAP_DEBOUNCE_S * 0.1));
        // But an immediate re-fire of that new target IS swallowed.
        assert!(!d.accept(1, 10.0 + TAP_DEBOUNCE_S * 0.2));
    }
}
