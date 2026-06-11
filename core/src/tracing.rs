//! Letter tracing — stroke-order data + pure trace-progress logic.
//!
//! Stroke centerlines are extracted offline from VicModernCursive by
//! `tools/trace_extract/extract.py` (macroquad can only rasterize fonts, so
//! the pen paths are baked into `tracing_data.rs`). Coordinates are font
//! units, y up, origin at the pen position on the baseline — the same frame
//! `draw_text_ex(glyph, pen_x, baseline_y, ..)` uses, so the app overlays
//! them on the rendered glyph with `px = pen + unit * font_size / UPEM`.
//!
//! Tracing is errorless coaching, not pass/fail: progress only ever moves
//! forward along the path, a wandering finger simply stops advancing, and
//! the teaching lives in the animated demo + start/end dots.

use crate::storage::KeyValueStore;
use nanoserde::{DeJson, SerJson};

pub use crate::tracing_data::{ASCENT, DESCENT, GLYPHS, UPEM, X_HEIGHT};

/// One letter's pen strokes. A single-point stroke is a "dot" (i / j): the kid
/// taps it instead of dragging.
pub struct GlyphTrace {
    pub ch: char,
    /// Horizontal advance in font units (pen origin → next pen origin).
    pub advance: f32,
    pub strokes: &'static [&'static [(f32, f32)]],
}

pub fn glyph(ch: char) -> Option<&'static GlyphTrace> {
    GLYPHS.iter().find(|g| g.ch == ch)
}

/// A dot stroke (the dot of i/j) is tapped, not traced.
pub fn is_dot(stroke: &[(f32, f32)]) -> bool {
    stroke.len() == 1
}

/// Ink bounding box of a glyph's strokes: (min_x, min_y, max_x, max_y).
pub fn ink_bbox(g: &GlyphTrace) -> (f32, f32, f32, f32) {
    let mut bb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for st in g.strokes {
        for &(x, y) in *st {
            bb.0 = bb.0.min(x);
            bb.1 = bb.1.min(y);
            bb.2 = bb.2.max(x);
            bb.3 = bb.3.max(y);
        }
    }
    bb
}

/// Total arc length of a stroke polyline (0 for dots).
pub fn stroke_len(pts: &[(f32, f32)]) -> f32 {
    pts.windows(2)
        .map(|w| dist(w[0], w[1]))
        .sum()
}

/// Point at arc length `s` along the polyline (clamped to the ends).
pub fn point_at(pts: &[(f32, f32)], s: f32) -> (f32, f32) {
    if pts.len() == 1 || s <= 0.0 {
        return pts[0];
    }
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = dist(w[0], w[1]);
        if acc + seg >= s && seg > 0.0 {
            let t = (s - acc) / seg;
            return (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
        }
        acc += seg;
    }
    *pts.last().unwrap()
}

/// How far ahead of the current progress the finger may pull the pen, in font
/// units. Small enough that a path returning near itself (the 'o' loop, the
/// 'a' retrace) can't be skipped across; big enough for a fast finger.
pub const ADVANCE_WINDOW: f32 = 150.0;

/// Advance trace progress along a stroke. `cur` is the arc length already
/// traced; the finger (font units) pulls progress forward to its projection
/// on the path if it is within `tol` of it, searching only `ADVANCE_WINDOW`
/// ahead. Progress never decreases (monotonic — errorless).
pub fn advance_progress(pts: &[(f32, f32)], cur: f32, finger: (f32, f32), tol: f32) -> f32 {
    let mut acc = 0.0;
    let mut best_s = cur;
    let mut best_d = tol;
    for w in pts.windows(2) {
        let seg = dist(w[0], w[1]);
        let s0 = acc;
        acc += seg;
        if seg <= 0.0 || acc < cur || s0 > cur + ADVANCE_WINDOW {
            continue;
        }
        // Project the finger onto this segment (clamped), in arc-length terms.
        let vx = w[1].0 - w[0].0;
        let vy = w[1].1 - w[0].1;
        let t = (((finger.0 - w[0].0) * vx + (finger.1 - w[0].1) * vy) / (seg * seg))
            .clamp(0.0, 1.0);
        let s = (s0 + t * seg).clamp(cur, cur + ADVANCE_WINDOW);
        let p = point_at(pts, s);
        let d = dist(p, finger);
        if d <= best_d && s > best_s {
            best_s = s;
            best_d = d;
        }
    }
    best_s
}

/// A stroke counts as finished within this many font units of its end (a
/// 4yo's flick rarely lands on the exact tip).
pub const END_SLACK: f32 = 90.0;

pub fn stroke_done(pts: &[(f32, f32)], cur: f32) -> bool {
    cur >= stroke_len(pts) - END_SLACK
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)).sqrt()
}

// --- teaching order + persisted progression ---------------------------------

/// Lowercase teaching order, grouped by stroke family (anticlockwise "magic c"
/// letters first, then straight-down letters, then down + hump, then the
/// trickier diagonals) — easiest motor patterns first, à la HWT, adapted to
/// VMC's lowercase-first curriculum.
pub const ORDER: [char; 26] = [
    'c', 'a', 'd', 'o', 'g', 'q', 'e', 's', // magic-c family
    'l', 'i', 't', 'u', 'j', 'y', // big/little lines down
    'n', 'm', 'h', 'r', 'b', 'p', 'k', 'f', // down, back up, over
    'v', 'w', 'x', 'z', // diagonals
];

/// Letters traced per session (~5 minutes at a preschool pace).
pub const SESSION_GOAL: usize = 5;

pub const SCHEMA_VERSION: u32 = 1;

/// Persisted progression: how far through `ORDER` the drip-in has reached.
/// Scores/stars stay session-only (never persisted), like every game here.
#[derive(Clone, Debug, PartialEq, Eq, SerJson, DeJson)]
pub struct TracingState {
    pub schema_version: u32,
    /// Index into `ORDER` of the next letter to introduce.
    pub next: u32,
}

pub fn empty_state() -> TracingState {
    TracingState { schema_version: SCHEMA_VERSION, next: 0 }
}

pub fn validate(json: &str) -> Option<TracingState> {
    let st: TracingState = DeJson::deserialize_json(json).ok()?;
    if st.schema_version != SCHEMA_VERSION {
        return None;
    }
    Some(TracingState { next: st.next.min(ORDER.len() as u32 - 1), ..st })
}

pub fn load<S: KeyValueStore + ?Sized>(store: &S) -> TracingState {
    store
        .get(&crate::storage::ns_key("tracing", "state"))
        .and_then(|raw| validate(&raw))
        .unwrap_or_else(empty_state)
}

pub fn save<S: KeyValueStore + ?Sized>(store: &mut S, st: &TracingState) {
    store.set(&crate::storage::ns_key("tracing", "state"), &st.serialize_json());
}

/// The letters for one session: `SESSION_GOAL` consecutive letters of `ORDER`
/// starting at `next`, wrapping back to the start once the alphabet is done
/// (steady review pass — generous repetition over novelty).
pub fn session_queue(st: &TracingState) -> Vec<char> {
    (0..SESSION_GOAL)
        .map(|i| ORDER[(st.next as usize + i) % ORDER.len()])
        .collect()
}

/// Advance the progression after a letter is completed.
pub fn letter_completed(st: &mut TracingState) {
    st.next = (st.next + 1) % ORDER.len() as u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_covers_the_alphabet() {
        assert_eq!(GLYPHS.len(), 26);
        for c in 'a'..='z' {
            let g = glyph(c).unwrap_or_else(|| panic!("missing glyph {c}"));
            assert!(!g.strokes.is_empty(), "{c}: no strokes");
            assert!(g.advance > 0.0, "{c}: bad advance");
            for st in g.strokes {
                assert!(!st.is_empty(), "{c}: empty stroke");
                if !is_dot(st) {
                    assert!(stroke_len(st) > 100.0, "{c}: implausibly short stroke");
                }
                for &(x, y) in *st {
                    assert!(x.is_finite() && y.is_finite(), "{c}: non-finite point");
                    assert!((-300.0..1200.0).contains(&x), "{c}: x out of range: {x}");
                    assert!((-600.0..1200.0).contains(&y), "{c}: y out of range: {y}");
                }
            }
        }
        // i and j carry their dot as a second, tappable stroke.
        for c in ['i', 'j'] {
            let g = glyph(c).unwrap();
            assert_eq!(g.strokes.len(), 2, "{c} should have body + dot");
            assert!(is_dot(g.strokes[1]), "{c}: second stroke should be the dot");
        }
        // f, t, x are the two-stroke letters of the chart.
        for c in ['f', 't', 'x'] {
            assert_eq!(glyph(c).unwrap().strokes.len(), 2, "{c} should have 2 strokes");
        }
    }

    #[test]
    fn order_is_a_permutation() {
        let mut seen = [false; 26];
        for c in ORDER {
            seen[(c as u8 - b'a') as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "ORDER must cover a..z exactly");
    }

    const LINE: [(f32, f32); 3] = [(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)];

    #[test]
    fn arc_length_and_point_at() {
        assert_eq!(stroke_len(&LINE), 200.0);
        assert_eq!(point_at(&LINE, 0.0), (0.0, 0.0));
        assert_eq!(point_at(&LINE, 150.0), (150.0, 0.0));
        assert_eq!(point_at(&LINE, 999.0), (200.0, 0.0));
    }

    #[test]
    fn finger_on_path_advances() {
        let s = advance_progress(&LINE, 0.0, (80.0, 5.0), 40.0);
        assert!((s - 80.0).abs() < 1.0, "should reach the projection: {s}");
    }

    #[test]
    fn far_finger_does_not_advance() {
        let s = advance_progress(&LINE, 50.0, (80.0, 120.0), 40.0);
        assert_eq!(s, 50.0);
    }

    #[test]
    fn progress_is_monotonic() {
        // A finger behind the current progress can't pull it backwards.
        let s = advance_progress(&LINE, 120.0, (60.0, 0.0), 40.0);
        assert!(s >= 120.0, "went backwards: {s}");
    }

    #[test]
    fn window_caps_a_jumping_finger() {
        // Finger at the far end while progress is at the start: only advances
        // if within the window (200 > ADVANCE_WINDOW → at most the window).
        let s = advance_progress(&LINE, 0.0, (200.0, 0.0), 40.0);
        assert!(s <= ADVANCE_WINDOW + 1.0, "skipped past the window: {s}");
    }

    #[test]
    fn real_glyph_traces_to_completion() {
        // Walk a finger down the first stroke of every letter at a coarse step;
        // it must reach stroke_done with a generous-but-finite tolerance.
        for g in GLYPHS.iter() {
            let pts = g.strokes[0];
            let total = stroke_len(pts);
            let mut cur = 0.0;
            let steps = (total / 60.0).ceil() as usize + 2;
            for i in 0..=steps {
                let target = point_at(pts, total * i as f32 / steps as f32);
                cur = advance_progress(pts, cur, target, 130.0);
            }
            assert!(stroke_done(pts, cur), "{}: stuck at {cur}/{total}", g.ch);
        }
    }

    #[test]
    fn state_roundtrip_and_validation() {
        let mut st = empty_state();
        assert_eq!(session_queue(&st), vec!['c', 'a', 'd', 'o', 'g']);
        letter_completed(&mut st);
        assert_eq!(st.next, 1);
        let json = st.serialize_json();
        assert_eq!(validate(&json), Some(st.clone()));
        // Wrong schema or garbage → None (caller falls back to empty_state).
        assert_eq!(validate("{\"schema_version\":99,\"next\":0}"), None);
        assert_eq!(validate("not json"), None);
        // Out-of-range index is clamped.
        let clamped = validate("{\"schema_version\":1,\"next\":999}").unwrap();
        assert!((clamped.next as usize) < ORDER.len());
    }

    #[test]
    fn session_queue_wraps() {
        let st = TracingState { schema_version: SCHEMA_VERSION, next: 24 };
        let q = session_queue(&st);
        assert_eq!(q.len(), SESSION_GOAL);
        assert_eq!(q[0], ORDER[24]);
        assert_eq!(q[2], ORDER[0]);
    }
}
