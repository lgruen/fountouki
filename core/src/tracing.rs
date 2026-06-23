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
//! the teaching lives in the animated demo + start/end dots. Which letters
//! come up is driven by the shared Leitner SRS (`crate::srs`) over the
//! motor-skill `ORDER`; the parent grades each finished trace ✓/✗ (scheduling
//! only — the kid always gets the star).

use crate::srs;
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

/// Smooth a stroke polyline into a denser curve with `subdiv` interpolated
/// points per source segment, using a centripetal-ish Catmull-Rom spline
/// (uniform parameterisation). The baked stroke points sit ~16 font units
/// apart; drawn as raw line segments they read faceted at iPad sizes, so the
/// renderer resamples here with `subdiv` scaled to the on-screen segment length
/// — the curve then stays truly round however big the card.
///
/// Endpoints are preserved exactly (the start/end dots must still land on the
/// path). A degenerate input (0/1 points) is returned unchanged.
pub fn smooth(pts: &[(f32, f32)], subdiv: usize) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 2 || subdiv <= 1 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity((n - 1) * subdiv + 1);
    out.push(pts[0]);
    for i in 0..n - 1 {
        // Catmull-Rom control points: clamp at the ends (duplicate endpoints).
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(n - 1)];
        for j in 1..=subdiv {
            let t = j as f32 / subdiv as f32;
            out.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    out
}

/// Uniform Catmull-Rom interpolation between `p1` and `p2` (t in 0..1), using
/// `p0`/`p3` as the surrounding control points.
fn catmull_rom(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    t: f32,
) -> (f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    let f = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * ((2.0 * b)
            + (-a + c) * t
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
            + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    (f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1))
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

/// The finger must land within this of a stroke's first point (the green dot)
/// to start it — touching the corridor mid-path doesn't arm the stroke.
/// Slightly larger than the drawn dot (90 units) for small fingers.
pub const START_RADIUS: f32 = 110.0;

/// A stroke counts as finished within this many font units of arc length from
/// its end (a 4yo's flick rarely lands on the exact tip)...
pub const END_SLACK: f32 = 60.0;
/// ...but only if the finger itself ended near the red end dot — arc progress
/// alone fired too early (the wide corridor let the pen run ahead of a finger
/// that never reached the end).
pub const END_RADIUS: f32 = 90.0;

/// Finished = nearly the whole arc traced AND the finger within `end_r` of
/// the stroke's last point (`end_r` ≥ `END_RADIUS`; callers may widen it so
/// it never shrinks below a fingertip on small screens). The app evaluates
/// this on the finger's LIFT, never mid-drag — auto-snapping to done while
/// the hand is still drawing cut letters short.
pub fn stroke_done(pts: &[(f32, f32)], cur: f32, finger: (f32, f32), end_r: f32) -> bool {
    cur >= stroke_len(pts) - END_SLACK && dist(finger, *pts.last().unwrap()) <= end_r
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)).sqrt()
}

// --- teaching order + persisted Leitner progression --------------------------

/// Lowercase teaching order, grouped by stroke family (anticlockwise "magic c"
/// letters first, then straight-down letters, then down + hump, then the
/// trickier diagonals) — easiest motor patterns first, à la HWT, adapted to
/// VMC's lowercase-first curriculum. This is the SRS drip-in order (tracing's
/// counterpart of phonics' `deck::INTRO_ORDER`).
pub const ORDER: [char; 26] = [
    'c', 'a', 'd', 'o', 'g', 'q', 'e', 's', // magic-c family
    'l', 'i', 't', 'u', 'j', 'y', // big/little lines down
    'n', 'm', 'h', 'r', 'b', 'p', 'k', 'f', // down, back up, over
    'v', 'w', 'x', 'z', // diagonals
];

/// Letters traced per session (~5 minutes at a preschool pace). Matches the
/// build-a-house stage count: one ✓-graded letter per construction step
/// (foundation → walls → roof → chimney → windows → door).
pub const SESSION_GOAL: usize = 6;

/// Which letters the SRS allows right now (drip-in frontier over `ORDER`).
/// Fresh learner → `c, a, d`.
pub fn active_letters(state: &srs::LeitnerState) -> Vec<char> {
    srs::active_letters(state, &ORDER)
}

/// Due-preferred shuffled play queue over the active letters (see
/// `srs::build_queue`). The scene rebuilds it whenever it runs out.
pub fn build_queue(
    state: &srs::LeitnerState,
    now: i64,
    rng: &mut crate::rng::Mulberry32,
) -> Vec<char> {
    srs::build_queue(state, &ORDER, now, rng)
}

/// The legacy persisted progression (pre-Leitner): an index into `ORDER` of
/// the next letter to introduce. Only kept so `load` can migrate it.
#[derive(Clone, Debug, PartialEq, Eq, SerJson, DeJson)]
struct LegacyState {
    schema_version: u32,
    next: u32,
}

/// Seed a Leitner state from the legacy linear progression: every letter the
/// kid had already reached (`ORDER[0..next]`) starts introduced (box 1) and
/// immediately due, so the first Leitner session reviews exactly what they
/// knew. `lastSeen = now` marks them seen (parent view + merge priority).
fn migrate_legacy(json: &str, now: i64) -> Option<srs::LeitnerState> {
    let legacy: LegacyState = DeJson::deserialize_json(json).ok()?;
    if legacy.schema_version != 1 {
        return None;
    }
    let mut st = srs::empty_state();
    srs::ensure_letters(&mut st, now);
    let n = (legacy.next as usize).min(ORDER.len());
    for &c in &ORDER[..n] {
        if let Some(ls) = st.letters.get_mut(&c.to_string()) {
            ls.box_ = 1;
            ls.due = now;
            ls.last_seen = now;
        }
    }
    st.version = n as u64;
    Some(st)
}

/// Load the tracing Leitner state: current schema first, then the legacy
/// `{schema_version, next}` blob (migrated in place on the next save), else
/// fresh. Always returns a fully-populated 26-letter state.
pub fn load<S: KeyValueStore + ?Sized>(store: &S, now: i64) -> srs::LeitnerState {
    let mut st = store
        .get(&crate::storage::ns_key("tracing", "state"))
        .and_then(|raw| srs::validate(&raw).or_else(|| migrate_legacy(&raw, now)))
        .unwrap_or_else(srs::empty_state);
    srs::ensure_letters(&mut st, now);
    st
}

pub fn save<S: KeyValueStore + ?Sized>(store: &mut S, st: &srs::LeitnerState) {
    store.set(&crate::storage::ns_key("tracing", "state"), &st.serialize_json());
}

/// Start-over state: every letter back to box 0 and due now, but with
/// `lastSeen = now` (not 0) and a bumped version — so a parent's reset beats
/// the pre-reset entries in the last-seen-wins sync merge instead of being
/// resurrected by the first pull.
pub fn start_over(st: &srs::LeitnerState, now: i64) -> srs::LeitnerState {
    let mut fresh = srs::empty_state();
    srs::ensure_letters(&mut fresh, now);
    for ls in fresh.letters.values_mut() {
        ls.last_seen = now;
    }
    fresh.version = st.version + 1;
    fresh
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

    /// 'g' (like a/d/q) is a magic-c bowl: the pen starts at the bowl's true
    /// top-right corner, not below it. Regression for a baked start dot that
    /// sat ~30 units under the corner (an end-extension hook).
    #[test]
    fn g_starts_at_the_bowl_top_right() {
        let g = glyph('g').unwrap();
        let start = g.strokes[0][0];
        let bb = ink_bbox(g);
        // Within a stroke-width of both the right edge and the top of the ink.
        assert!(bb.2 - start.0 <= 12.0, "g start not at right edge: {start:?} bbox {bb:?}");
        assert!(bb.3 - start.1 <= 12.0, "g start not at top: {start:?} bbox {bb:?}");
    }

    #[test]
    fn smooth_preserves_endpoints_and_densifies() {
        let line = [(0.0, 0.0), (100.0, 0.0), (200.0, 50.0), (300.0, 50.0)];
        let s = smooth(&line, 8);
        assert_eq!(s.first().copied().unwrap(), line[0], "start moved");
        assert_eq!(s.last().copied().unwrap(), *line.last().unwrap(), "end moved");
        assert_eq!(s.len(), (line.len() - 1) * 8 + 1, "wrong density");
        // A straight run stays on the line (no overshoot wiggle).
        let mid = point_at(&s, stroke_len(&s) * 0.0); // start
        assert_eq!(mid, line[0]);
        // Degenerate inputs pass through untouched.
        assert_eq!(smooth(&[(1.0, 2.0)], 8), vec![(1.0, 2.0)]);
        assert_eq!(smooth(&line, 1), line.to_vec());
    }

    #[test]
    fn smooth_stays_near_the_original_path() {
        // The smoothed curve must hug the source polyline (a guide the kid
        // traces — it can't bulge away from the letter).
        for g in GLYPHS.iter() {
            for st in g.strokes {
                if is_dot(st) {
                    continue;
                }
                let s = smooth(st, 6);
                for &p in &s {
                    // Nearest distance from the smoothed point to the source path.
                    let d = (0..st.len() - 1)
                        .map(|i| seg_dist(p, st[i], st[i + 1]))
                        .fold(f32::MAX, f32::min);
                    assert!(d <= 24.0, "{}: smoothed point {p:?} strays {d} from path", g.ch);
                }
            }
        }
    }

    /// Distance from point `p` to segment `a`-`b`.
    fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let l2 = vx * vx + vy * vy;
        if l2 <= 0.0 {
            return dist(p, a);
        }
        let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0);
        dist(p, (a.0 + t * vx, a.1 + t * vy))
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
    fn stroke_done_needs_the_finger_at_the_end() {
        let total = stroke_len(&LINE); // 200
        // Full progress + finger on the tip: done.
        assert!(stroke_done(&LINE, total, (200.0, 0.0), END_RADIUS));
        // Full arc progress but the finger never reached the end: not done.
        assert!(!stroke_done(&LINE, total, (60.0, 0.0), END_RADIUS));
        // Finger at the end but progress stopped short of the slack: not done.
        assert!(!stroke_done(&LINE, total - END_SLACK - 10.0, (200.0, 0.0), END_RADIUS));
        // Inside both gates (a flick that stops just short): done.
        assert!(stroke_done(&LINE, total - END_SLACK + 1.0, (160.0, 30.0), END_RADIUS));
    }

    #[test]
    fn real_glyph_traces_to_completion() {
        // Walk a finger down the first stroke of every letter at a coarse step;
        // it must reach stroke_done with a generous-but-finite tolerance.
        for g in GLYPHS.iter() {
            let pts = g.strokes[0];
            let total = stroke_len(pts);
            let mut cur = 0.0;
            let mut target = pts[0];
            let steps = (total / 60.0).ceil() as usize + 2;
            for i in 0..=steps {
                target = point_at(pts, total * i as f32 / steps as f32);
                cur = advance_progress(pts, cur, target, 130.0);
            }
            assert!(stroke_done(pts, cur, target, END_RADIUS), "{}: stuck at {cur}/{total}", g.ch);
        }
    }

    use crate::storage::MemStore;

    #[test]
    fn fresh_active_letters_are_the_magic_c_start() {
        let now = 1000;
        let mut st = srs::empty_state();
        srs::ensure_letters(&mut st, now);
        assert_eq!(active_letters(&st), vec!['c', 'a', 'd']);
        // Fresh learner: everything due → the queue is a permutation of them.
        let mut rng = crate::rng::Mulberry32::new(7);
        let mut q = build_queue(&st, now, &mut rng);
        q.sort_unstable();
        assert_eq!(q, vec!['a', 'c', 'd']);
    }

    #[test]
    fn promotion_unlocks_down_the_motor_order() {
        let now = 1000;
        let mut st = srs::empty_state();
        srs::ensure_letters(&mut st, now);
        for c in ['c', 'a', 'd'] {
            srs::grade_got_it(&mut st, c, now);
        }
        // c,a,d settled → the buffer refills with o,g,q.
        assert_eq!(active_letters(&st), vec!['c', 'a', 'd', 'o', 'g', 'q']);
    }

    #[test]
    fn load_migrates_the_legacy_progression() {
        let now = 5_000;
        let mut store = MemStore::new();
        store.set(
            &crate::storage::ns_key("tracing", "state"),
            "{\"schema_version\":1,\"next\":5}",
        );
        let st = load(&store, now);
        // The 5 reached letters start introduced (box 1, due now, seen now)...
        for &c in &ORDER[..5] {
            let ls = st.letters.get(&c.to_string()).unwrap();
            assert_eq!((ls.box_, ls.due, ls.last_seen), (1, now, now), "{c}");
        }
        // ...the rest are fresh box-0.
        for &c in &ORDER[5..] {
            let ls = st.letters.get(&c.to_string()).unwrap();
            assert_eq!((ls.box_, ls.last_seen), (0, 0), "{c}");
        }
        assert_eq!(st.version, 5);
        // Migrated progress resumes mid-order: q is the next new letter.
        assert_eq!(active_letters(&st), ORDER[..8].to_vec());
    }

    #[test]
    fn load_falls_back_to_fresh_and_roundtrips() {
        let now = 77;
        let mut store = MemStore::new();
        // Nothing stored / garbage → fresh fully-populated state.
        let st = load(&store, now);
        assert_eq!(st.letters.len(), 26);
        assert_eq!(st.version, 0);
        store.set(&crate::storage::ns_key("tracing", "state"), "not json");
        assert_eq!(load(&store, now).version, 0);
        // Save → load roundtrips the current schema.
        let mut st = st;
        srs::grade_got_it(&mut st, 'c', now);
        save(&mut store, &st);
        assert_eq!(load(&store, now), st);
    }

    #[test]
    fn start_over_zeroes_boxes_but_wins_merges() {
        let now = 9_000;
        let mut st = srs::empty_state();
        srs::ensure_letters(&mut st, 100);
        for c in ['c', 'a', 'd'] {
            srs::grade_got_it(&mut st, c, 100);
        }
        let reset = start_over(&st, now);
        assert_eq!(reset.version, st.version + 1);
        for ls in reset.letters.values() {
            assert_eq!((ls.box_, ls.due, ls.last_seen), (0, now, now));
        }
        // A pre-reset remote (older lastSeen) must NOT resurrect progress.
        let merged = srs::merge(&reset, &st, now);
        assert!(merged.letters.values().all(|ls| ls.box_ == 0));
    }
}
