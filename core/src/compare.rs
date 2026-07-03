//! "Number Scales" — compare-two-numbers game: persisted mastery + round gen.
//!
//! The skill is SYMBOLIC MAGNITUDE COMPARISON ("which is more, 3 or 7?"), the
//! foundation of number sense. Two halves live here (the pure, testable half —
//! no rendering):
//!
//! 1. MASTERY — the highest DIFFICULTY LEVEL (1..=[`MAX_LEVEL`]) whose full
//!    session the kid has completed (`best_level`; 0 = none yet). Identical
//!    model + sync to [`crate::clock`]: GENERATION + MAX, monotonic within a
//!    generation (a sync never lowers the furthest), a parent "start over"
//!    bumps `generation` so the reset propagates. JSON keys are load-bearing.
//!
//! 2. ROUND GENERATION ([`next_round`]) — the pedagogy ladder, encoded as pure
//!    number choice so it can be unit-tested. L1 far: 0..9, a big gap (≥3),
//!    quantity dots shown (learn more=bigger). L2 near: 0..9, gap down to 1,
//!    dots shown (harder discrimination). L3 read: 0..9, numerals ONLY (no dots)
//!    → must actually read. L4 teens: up to 20, two-digit numerals, no dots.
//!    L5 fewer: up to 20, mixes "which is SMALLER?" into the prompts.
//!    Difficulty is parent-set (like clock); a session plays a fixed number of
//!    rounds at one level, then records that level as the new best.
//!
//! Why the ladder: comparison is easier when the two numbers are far apart (the
//! "distance effect") and small (the "size effect"); "fewer/smaller" is harder
//! than "more/bigger" and comes last. Grounding the numeral in a shown quantity
//! first, then fading it, builds the symbol→magnitude link and forces genuine
//! reading at the top — where a coin-flip can't reach and grading is parent-led.

use crate::rng::Mulberry32;
use crate::storage::KeyValueStore;
use nanoserde::{DeJson, SerJson};

/// Persistent schema version; a blob with any other value is discarded.
pub const SCHEMA_VERSION: u32 = 1;

/// The highest difficulty level the game offers; `record_level` clamps to it so
/// a stray higher value can never be persisted.
pub const MAX_LEVEL: u32 = 5;

/// The full persisted compare-game state. JSON keys: `schemaVersion`,
/// `generation`, `bestLevel`, `lastSeen` — matching [`crate::clock::ClockState`]
/// so the sync wiring is identical.
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct CompareState {
    #[nserde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// Reset generation: +1 ONLY on a parent start-over. A higher generation is
    /// a more-recent reset and wins the merge. Absent (older blob) reads as 0.
    #[nserde(default)]
    pub generation: u32,
    /// Highest difficulty level (1..=`MAX_LEVEL`) whose full session the kid has
    /// completed; 0 = none yet.
    #[nserde(rename = "bestLevel")]
    pub best_level: u32,
    /// epoch ms of the last mutating change; 0 = never.
    #[nserde(rename = "lastSeen")]
    pub last_seen: i64,
}

/// Fresh / empty state: `{ schemaVersion: 1, generation: 0, bestLevel: 0, lastSeen: 0 }`.
pub fn empty_state() -> CompareState {
    CompareState { schema_version: SCHEMA_VERSION, generation: 0, best_level: 0, last_seen: 0 }
}

/// Validate a raw JSON blob into a `CompareState`. Returns `None` (→ caller
/// falls back to `empty_state`) unless it parses AND the schema matches.
pub fn validate(json: &str) -> Option<CompareState> {
    let state = CompareState::deserialize_json(json).ok()?;
    if state.schema_version != SCHEMA_VERSION {
        return None;
    }
    Some(state)
}

/// Record a completed session at difficulty `level`. MONOTONIC: only raises
/// `best_level` (clamped to `MAX_LEVEL`) when `level` beats it, stamping
/// `last_seen`; `generation` is UNCHANGED. A non-improvement is a no-op.
pub fn record_level(state: &mut CompareState, level: u32, now: i64) {
    let level = level.min(MAX_LEVEL);
    if level > state.best_level {
        state.best_level = level;
        state.last_seen = now;
    }
}

/// Start-over state: `best_level = 0`, `generation + 1`, `last_seen = now`. The
/// bumped generation lets a parent's reset out-rank pre-reset entries in the
/// merge — the only path that lowers `best_level`.
pub fn start_over(state: &CompareState, now: i64) -> CompareState {
    CompareState {
        schema_version: SCHEMA_VERSION,
        generation: state.generation + 1,
        best_level: 0,
        last_seen: now,
    }
}

/// Merge a remote state into local (cross-device sync). GENERATION + MAX —
/// same rules as [`crate::clock::merge`]. Commutative + idempotent.
pub fn merge(local: &CompareState, remote: &CompareState, _now: i64) -> CompareState {
    let best_level = match local.generation.cmp(&remote.generation) {
        std::cmp::Ordering::Equal => local.best_level.max(remote.best_level),
        std::cmp::Ordering::Greater => local.best_level,
        std::cmp::Ordering::Less => remote.best_level,
    };
    CompareState {
        schema_version: SCHEMA_VERSION,
        generation: local.generation.max(remote.generation),
        best_level,
        last_seen: local.last_seen.max(remote.last_seen),
    }
}

/// Load the compare state from `fountouki.compare.state.v1`: validated current
/// schema, else fresh.
pub fn load<S: KeyValueStore + ?Sized>(store: &S, _now: i64) -> CompareState {
    store
        .get(&crate::storage::ns_key("compare", "state"))
        .and_then(|raw| validate(&raw))
        .unwrap_or_else(empty_state)
}

/// Persist the whole state to `fountouki.compare.state.v1`.
pub fn save<S: KeyValueStore + ?Sized>(store: &mut S, state: &CompareState) {
    store.set(&crate::storage::ns_key("compare", "state"), &state.serialize_json());
}

// ---------------------------------------------------------------------------
// Round generation — the pedagogy ladder (pure, seeded, testable).
// ---------------------------------------------------------------------------

/// One comparison to present: two distinct numbers and which one the child is
/// asked to find. `left`/`right` are already in display order (independently
/// drawn, so either can be the larger).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Round {
    pub left: u8,
    pub right: u8,
    /// `true` → the prompt asks for the SMALLER number (level 5 mixes these in);
    /// `false` → the BIGGER number (levels 1–4 and half of level 5).
    pub want_smaller: bool,
}

impl Round {
    /// The numeric value that is the correct answer for this round's direction.
    pub fn target_value(&self) -> u8 {
        if self.want_smaller {
            self.left.min(self.right)
        } else {
            self.left.max(self.right)
        }
    }
    /// Which card is correct: `0` = left, `1` = right.
    pub fn correct_side(&self) -> u8 {
        let left_wins = if self.want_smaller {
            self.left < self.right
        } else {
            self.left > self.right
        };
        if left_wins {
            0
        } else {
            1
        }
    }
}

/// Per-level generation constraints.
struct Spec {
    /// Largest value that can appear (inclusive). Values are drawn from `0..=max`.
    max: u8,
    /// Minimum absolute distance between the two numbers (the "distance effect"
    /// scaffold — far-apart pairs are easier).
    min_dist: u8,
    /// At least one operand must be ≥ 10 (exercise two-digit numerals).
    require_teen: bool,
    /// Whether "which is SMALLER?" prompts may appear.
    allow_smaller: bool,
}

fn spec(level: u32) -> Spec {
    match level {
        1 => Spec { max: 9, min_dist: 3, require_teen: false, allow_smaller: false },
        2 => Spec { max: 9, min_dist: 1, require_teen: false, allow_smaller: false },
        3 => Spec { max: 9, min_dist: 1, require_teen: false, allow_smaller: false },
        4 => Spec { max: 20, min_dist: 1, require_teen: true, allow_smaller: false },
        _ => Spec { max: 20, min_dist: 1, require_teen: true, allow_smaller: true },
    }
}

/// Whether this level shows quantity dots beside each numeral (the scaffold that
/// grounds "bigger number = more"). Only the two teaching levels; from L3 on the
/// numeral stands alone so the child must actually read it.
pub fn show_quantity(level: u32) -> bool {
    level <= 2
}

/// Generate the next round for `level` from `rng`. Deterministic for a given
/// seed (goldens/playtests reproduce). Always returns two distinct numbers
/// honouring the level's range / distance / teen constraints.
pub fn next_round(level: u32, rng: &mut Mulberry32) -> Round {
    let s = spec(level);
    let n = s.max as usize + 1;
    let (mut a, mut b) = (0u8, s.max);
    // Rejection-sample a valid pair. Every spec is satisfiable, so this
    // terminates quickly; the guard only bounds a pathological RNG run, falling
    // back to (0, max) which trivially meets every constraint.
    for _ in 0..64 {
        let x = rng.below(n) as u8;
        let y = rng.below(n) as u8;
        if x == y {
            continue;
        }
        let dist = x.abs_diff(y);
        if dist < s.min_dist {
            continue;
        }
        if s.require_teen && x < 10 && y < 10 {
            continue;
        }
        a = x;
        b = y;
        break;
    }
    let want_smaller = s.allow_smaller && rng.below(2) == 1;
    Round { left: a, right: b, want_smaller }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemStore;

    #[test]
    fn empty_state_shape() {
        let s = empty_state();
        assert_eq!((s.schema_version, s.generation, s.best_level, s.last_seen), (1, 0, 0, 0));
    }

    #[test]
    fn record_raises_level_only_and_never_bumps_generation() {
        let mut s = empty_state();
        record_level(&mut s, 2, 100);
        assert_eq!((s.best_level, s.generation, s.last_seen), (2, 0, 100));
        record_level(&mut s, 1, 200); // lower → no change
        assert_eq!((s.best_level, s.generation, s.last_seen), (2, 0, 100));
        record_level(&mut s, 2, 300); // equal → no change
        assert_eq!((s.best_level, s.generation, s.last_seen), (2, 0, 100));
        record_level(&mut s, 5, 400); // better → raises + stamps
        assert_eq!((s.best_level, s.generation, s.last_seen), (5, 0, 400));
    }

    #[test]
    fn record_clamps_to_max_level() {
        let mut s = empty_state();
        record_level(&mut s, 99, 10);
        assert_eq!(s.best_level, MAX_LEVEL);
    }

    #[test]
    fn record_never_decrements() {
        let mut s = empty_state();
        for (lvl, now) in [(3, 10), (1, 20), (5, 30), (2, 40)] {
            let before = s.best_level;
            record_level(&mut s, lvl, now);
            assert!(s.best_level >= before, "best_level went backwards");
        }
        assert_eq!(s.best_level, 5);
    }

    #[test]
    fn start_over_resets_and_bumps_generation() {
        let mut s = empty_state();
        record_level(&mut s, 3, 100);
        let reset = start_over(&s, 9_000);
        assert_eq!((reset.best_level, reset.generation, reset.last_seen), (0, s.generation + 1, 9_000));
    }

    #[test]
    fn merge_is_monotonic_within_a_generation() {
        let mut a = empty_state();
        record_level(&mut a, 5, 200);
        let mut b = empty_state();
        record_level(&mut b, 2, 150);
        assert_eq!(merge(&a, &b, 999).best_level, 5);
        assert_eq!(merge(&b, &a, 999).best_level, 5); // order-independent
    }

    #[test]
    fn merge_higher_generation_reset_propagates() {
        let mut never_reset = empty_state();
        record_level(&mut never_reset, 5, 100);
        let reset = start_over(&empty_state(), 500);
        let m1 = merge(&never_reset, &reset, 999);
        let m2 = merge(&reset, &never_reset, 999);
        assert_eq!((m1.generation, m1.best_level), (1, 0), "reset did not propagate");
        assert_eq!(m2.best_level, 0, "reset did not propagate (other order)");
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        let cases = [
            (empty_state(), empty_state()),
            (
                CompareState { schema_version: 1, generation: 0, best_level: 2, last_seen: 30 },
                CompareState { schema_version: 1, generation: 0, best_level: 4, last_seen: 25 },
            ),
            (
                CompareState { schema_version: 1, generation: 0, best_level: 3, last_seen: 100 },
                CompareState { schema_version: 1, generation: 1, best_level: 0, last_seen: 500 },
            ),
        ];
        for (a, b) in cases {
            assert_eq!(merge(&a, &b, 0), merge(&b, &a, 0), "merge not commutative");
            assert_eq!(merge(&a, &a, 0), a, "merge(a, a) != a");
            let m = merge(&a, &b, 0);
            assert_eq!(merge(&m, &a, 0), m, "re-merge with a changed it");
        }
    }

    #[test]
    fn validate_and_json_keys_roundtrip() {
        let s = CompareState { schema_version: 1, generation: 7, best_level: 2, last_seen: 1234 };
        let json = s.serialize_json();
        assert!(json.contains("\"schemaVersion\":1"), "json: {json}");
        assert!(json.contains("\"generation\":7"), "json: {json}");
        assert!(json.contains("\"bestLevel\":2"), "json: {json}");
        assert!(json.contains("\"lastSeen\":1234"), "json: {json}");
        assert!(!json.contains("best_level"), "json: {json}");
        assert_eq!(validate(&json).unwrap(), s);
        assert!(validate(r#"{"schemaVersion":2,"bestLevel":0,"lastSeen":0}"#).is_none());
        assert!(validate("not json").is_none());
    }

    #[test]
    fn load_falls_back_and_save_roundtrips() {
        let mut store = MemStore::new();
        assert_eq!(load(&store, 0), empty_state());
        let mut s = empty_state();
        record_level(&mut s, 3, 555);
        save(&mut store, &s);
        assert_eq!(load(&store, 0), s);
    }

    // --- round generation --------------------------------------------------

    fn draws(level: u32, seed: u32, n: usize) -> Vec<Round> {
        let mut rng = Mulberry32::new(seed);
        (0..n).map(|_| next_round(level, &mut rng)).collect()
    }

    #[test]
    fn rounds_always_have_two_distinct_numbers_in_range() {
        for level in 1..=MAX_LEVEL {
            let s = spec(level);
            for r in draws(level, 42 + level, 400) {
                assert_ne!(r.left, r.right, "L{level}: equal numbers");
                assert!(r.left <= s.max && r.right <= s.max, "L{level}: out of range {r:?}");
            }
        }
    }

    #[test]
    fn level1_pairs_are_far_apart() {
        for r in draws(1, 7, 400) {
            assert!(r.left.abs_diff(r.right) >= 3, "L1 pair too close: {r:?}");
            assert!(r.left <= 9 && r.right <= 9, "L1 out of 0..9: {r:?}");
        }
    }

    #[test]
    fn low_levels_stay_single_digit_and_bigger_only() {
        for level in 1..=3 {
            for r in draws(level, 3 + level, 300) {
                assert!(r.left <= 9 && r.right <= 9, "L{level} not single-digit: {r:?}");
                assert!(!r.want_smaller, "L{level} should never ask for smaller");
            }
        }
    }

    #[test]
    fn teen_levels_include_a_two_digit_number() {
        for level in [4, 5] {
            for r in draws(level, 100 + level, 300) {
                assert!(r.left <= 20 && r.right <= 20, "L{level} over 20: {r:?}");
                assert!(r.left >= 10 || r.right >= 10, "L{level} lacks a teen: {r:?}");
            }
        }
    }

    #[test]
    fn only_top_level_ever_asks_for_smaller() {
        let any_smaller = draws(5, 9, 400).iter().any(|r| r.want_smaller);
        let any_bigger = draws(5, 9, 400).iter().any(|r| !r.want_smaller);
        assert!(any_smaller && any_bigger, "L5 should mix both directions");
    }

    #[test]
    fn correct_side_matches_target_value() {
        for level in 1..=MAX_LEVEL {
            for r in draws(level, level * 13, 200) {
                let chosen = if r.correct_side() == 0 { r.left } else { r.right };
                assert_eq!(chosen, r.target_value(), "L{level} side/value mismatch: {r:?}");
            }
        }
    }

    #[test]
    fn show_quantity_only_on_teaching_levels() {
        assert!(show_quantity(1) && show_quantity(2));
        assert!(!show_quantity(3) && !show_quantity(4) && !show_quantity(5));
    }
}
