//! "Frog's Day" analog-clock game — persisted mastery + cross-device merge.
//!
//! Like [`crate::singback`] (and unlike phonics/tracing's per-letter Leitner
//! SRS), the clock game tracks a single monotonic number: the highest
//! DIFFICULTY LEVEL whose full day the kid has completed (`best_level`, 1..=4;
//! 0 = none yet). The in-session day progress + the per-difficulty level the
//! parent picked are NOT persisted — only this best.
//!
//! Sync model: GENERATION + MAX (identical to singback). `best_level` is
//! monotonic within a generation — both [`record_level`] (per-device) and
//! [`merge`] (cross-device) only ever RAISE it, so a sync can never lower the
//! kid's furthest. A parent "start over" ([`start_over`]) bumps `generation`
//! and resets `best_level` to 0; the higher generation wins the merge, so the
//! reset propagates to every device.
//!
//! [`merge`] is commutative + idempotent. JSON key names are load-bearing for
//! cross-device sync — do not rename.

use crate::storage::KeyValueStore;
use nanoserde::{DeJson, SerJson};

/// Persistent schema version; a blob with any other value is discarded.
pub const SCHEMA_VERSION: u32 = 1;

/// The highest difficulty level (1..=4) the game offers; `record_level` clamps
/// to it so a stray higher value can never be persisted.
pub const MAX_LEVEL: u32 = 4;

/// The full persisted clock-game state. JSON keys: `schemaVersion`,
/// `generation`, `bestLevel`, `lastSeen`.
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct ClockState {
    #[nserde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// Reset generation: +1 ONLY on a parent start-over (never on a level
    /// improvement). A higher generation = a more-recent reset, and wins the
    /// merge. An absent key (older blob) reads as generation 0.
    #[nserde(default)]
    pub generation: u32,
    /// Highest difficulty level (1..=`MAX_LEVEL`) whose full day the kid has
    /// completed; 0 = none yet.
    #[nserde(rename = "bestLevel")]
    pub best_level: u32,
    /// epoch ms of the last mutating change; 0 = never.
    #[nserde(rename = "lastSeen")]
    pub last_seen: i64,
}

/// Fresh / empty state: `{ schemaVersion: 1, generation: 0, bestLevel: 0, lastSeen: 0 }`.
pub fn empty_state() -> ClockState {
    ClockState { schema_version: SCHEMA_VERSION, generation: 0, best_level: 0, last_seen: 0 }
}

/// Validate a raw JSON blob into a `ClockState`. Returns `None` (→ caller falls
/// back to `empty_state`) unless it parses AND `schemaVersion == SCHEMA_VERSION`.
pub fn validate(json: &str) -> Option<ClockState> {
    let state = ClockState::deserialize_json(json).ok()?;
    if state.schema_version != SCHEMA_VERSION {
        return None;
    }
    Some(state)
}

/// Record a completed day at difficulty `level`. MONOTONIC: only raises
/// `best_level` (clamped to `MAX_LEVEL`) when `level` beats it, stamping
/// `last_seen`; `generation` is UNCHANGED. A non-improvement is a no-op.
pub fn record_level(state: &mut ClockState, level: u32, now: i64) {
    let level = level.min(MAX_LEVEL);
    if level > state.best_level {
        state.best_level = level;
        state.last_seen = now;
    }
}

/// Start-over state: `best_level = 0` with `generation = state.generation + 1`
/// and `last_seen = now`. The bumped generation lets a parent's reset out-rank
/// the pre-reset entries in the merge — the only path that lowers `best_level`.
pub fn start_over(state: &ClockState, now: i64) -> ClockState {
    ClockState {
        schema_version: SCHEMA_VERSION,
        generation: state.generation + 1,
        best_level: 0,
        last_seen: now,
    }
}

/// Merge a remote state into local (cross-device sync). GENERATION + MAX:
/// - `generation` = max of the two.
/// - `best_level`: within the SAME generation, `max(local, remote)` — so a sync
///   can never lower the kid's furthest. When generations differ, take the
///   `best_level` of the side with the HIGHER generation (its reset is more
///   recent and must propagate).
/// - `last_seen` = max of the two.
///
/// Commutative + idempotent (max is symmetric; the generation tie/strict cases
/// pick identically either order).
pub fn merge(local: &ClockState, remote: &ClockState, _now: i64) -> ClockState {
    let best_level = match local.generation.cmp(&remote.generation) {
        std::cmp::Ordering::Equal => local.best_level.max(remote.best_level),
        std::cmp::Ordering::Greater => local.best_level,
        std::cmp::Ordering::Less => remote.best_level,
    };
    ClockState {
        schema_version: SCHEMA_VERSION,
        generation: local.generation.max(remote.generation),
        best_level,
        last_seen: local.last_seen.max(remote.last_seen),
    }
}

/// Load the clock state from `fountouki.clock.state.v1`: validated current
/// schema, else fresh. (`now` is unused today but kept for signature parity.)
pub fn load<S: KeyValueStore + ?Sized>(store: &S, _now: i64) -> ClockState {
    store
        .get(&crate::storage::ns_key("clock", "state"))
        .and_then(|raw| validate(&raw))
        .unwrap_or_else(empty_state)
}

/// Persist the whole state to `fountouki.clock.state.v1`.
pub fn save<S: KeyValueStore + ?Sized>(store: &mut S, state: &ClockState) {
    store.set(&crate::storage::ns_key("clock", "state"), &state.serialize_json());
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
        // A lower level: no change at all (no churn).
        record_level(&mut s, 1, 200);
        assert_eq!((s.best_level, s.generation, s.last_seen), (2, 0, 100));
        // Equal level: still a no-op (strict improvement only).
        record_level(&mut s, 2, 300);
        assert_eq!((s.best_level, s.generation, s.last_seen), (2, 0, 100));
        // A better level: raises + stamps last_seen, generation UNCHANGED.
        record_level(&mut s, 4, 400);
        assert_eq!((s.best_level, s.generation, s.last_seen), (4, 0, 400));
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
        for (lvl, now) in [(3, 10), (1, 20), (4, 30), (2, 40)] {
            let before = s.best_level;
            record_level(&mut s, lvl, now);
            assert!(s.best_level >= before, "best_level went backwards");
        }
        assert_eq!(s.best_level, 4);
    }

    #[test]
    fn start_over_resets_to_zero_and_bumps_generation() {
        let mut s = empty_state();
        record_level(&mut s, 3, 100);
        let reset = start_over(&s, 9_000);
        assert_eq!(reset.best_level, 0);
        assert_eq!(reset.generation, s.generation + 1);
        assert_eq!(reset.last_seen, 9_000);
    }

    #[test]
    fn merge_is_monotonic_within_a_generation() {
        let mut a = empty_state();
        record_level(&mut a, 4, 200);
        let mut b = empty_state();
        record_level(&mut b, 2, 150);
        let m1 = merge(&a, &b, 999);
        let m2 = merge(&b, &a, 999);
        assert_eq!(m1.best_level, 4);
        assert_eq!(m2.best_level, 4); // order-independent
        assert_eq!(m1.generation, 0);
    }

    #[test]
    fn merge_higher_generation_reset_propagates() {
        let mut never_reset = empty_state();
        record_level(&mut never_reset, 4, 100); // gen 0, best 4
        let reset = start_over(&empty_state(), 500); // gen 1, best 0
        let m1 = merge(&never_reset, &reset, 999);
        let m2 = merge(&reset, &never_reset, 999);
        assert_eq!(m1.generation, 1);
        assert_eq!(m1.best_level, 0, "reset did not propagate");
        assert_eq!(m2.best_level, 0, "reset did not propagate (other order)");
    }

    #[test]
    fn merge_after_reset_resumes_max_within_generation() {
        let reset = start_over(&empty_state(), 500); // gen 1, best 0
        let mut played = reset.clone();
        record_level(&mut played, 3, 600); // gen 1, best 3
        let m = merge(&reset, &played, 999);
        assert_eq!((m.generation, m.best_level), (1, 3));
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        let cases = [
            (empty_state(), empty_state()),
            (
                ClockState { schema_version: 1, generation: 0, best_level: 2, last_seen: 30 },
                ClockState { schema_version: 1, generation: 0, best_level: 4, last_seen: 25 },
            ),
            (
                ClockState { schema_version: 1, generation: 0, best_level: 3, last_seen: 100 },
                ClockState { schema_version: 1, generation: 1, best_level: 0, last_seen: 500 },
            ),
        ];
        for (a, b) in cases {
            assert_eq!(merge(&a, &b, 0), merge(&b, &a, 0), "merge not commutative");
            assert_eq!(merge(&a, &a, 0), a, "merge(a, a) != a");
            let m = merge(&a, &b, 0);
            assert_eq!(merge(&m, &a, 0), m, "re-merge with a changed it");
            assert_eq!(merge(&m, &b, 0), m, "re-merge with b changed it");
        }
    }

    #[test]
    fn validate_accepts_good_blob_and_rejects_wrong_schema() {
        let good = r#"{"schemaVersion":1,"generation":2,"bestLevel":3,"lastSeen":1748600100000}"#;
        let s = validate(good).expect("should validate");
        assert_eq!((s.generation, s.best_level, s.last_seen), (2, 3, 1748600100000));
        assert!(validate(r#"{"schemaVersion":2,"bestLevel":0,"lastSeen":0}"#).is_none());
        assert!(validate("not json").is_none());
    }

    #[test]
    fn validate_tolerates_absent_generation() {
        let no_gen = r#"{"schemaVersion":1,"bestLevel":3,"lastSeen":1234}"#;
        let s = validate(no_gen).expect("should validate without generation");
        assert_eq!((s.generation, s.best_level, s.last_seen), (0, 3, 1234));
    }

    #[test]
    fn json_keys_are_exact_camel_case_and_roundtrip() {
        let s = ClockState { schema_version: 1, generation: 7, best_level: 2, last_seen: 1234 };
        let json = s.serialize_json();
        assert!(json.contains("\"schemaVersion\":1"), "json: {json}");
        assert!(json.contains("\"generation\":7"), "json: {json}");
        assert!(json.contains("\"bestLevel\":2"), "json: {json}");
        assert!(json.contains("\"lastSeen\":1234"), "json: {json}");
        assert!(!json.contains("best_level"), "json: {json}");
        assert!(!json.contains("last_seen"), "json: {json}");
        assert_eq!(ClockState::deserialize_json(&json).unwrap(), s);
    }

    #[test]
    fn load_falls_back_to_fresh_and_save_roundtrips() {
        let mut store = MemStore::new();
        assert_eq!(load(&store, 0), empty_state());
        store.set(&crate::storage::ns_key("clock", "state"), "not json");
        assert_eq!(load(&store, 0).generation, 0);
        let mut s = empty_state();
        record_level(&mut s, 3, 555);
        save(&mut store, &s);
        assert_eq!(load(&store, 0), s);
    }
}
