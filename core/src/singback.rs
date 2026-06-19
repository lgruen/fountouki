//! "Sing Back" memory game — persisted best-span mastery + cross-device merge.
//!
//! Unlike phonics/tracing (per-letter Leitner SRS), Sing Back tracks a single
//! number: the longest sequence the kid has correctly reproduced (`best_span`).
//!
//! Sync model: GENERATION + MAX. `best_span` is monotonic within a generation —
//! both [`record_span`] (per-device) and [`merge`] (cross-device) only ever
//! RAISE it (a `max`), so a sync can NEVER lower the kid's best. The single
//! exception is a parent "start over" ([`start_over`]), which bumps `generation`
//! and resets `best_span` to 0; a higher generation wins the merge, so the reset
//! propagates to every device — the only non-monotonic path, and it wins by
//! generation rather than by span. (`generation` is bumped ONLY by start_over,
//! never by record_span, so ordinary play never churns it.)
//!
//! [`merge`] is commutative and idempotent. The rare case of two devices each
//! doing an independent start_over offline is resolved best-effort by max
//! generation (one reset's `best_span` may be carried, the other dropped) —
//! acceptable for a single-family app, since both intended best=0 anyway.
//!
//! Persistent model (`SingBackState`) is the only thing saved/synced. JSON key
//! names are load-bearing for cross-device sync — do not rename.

use crate::storage::KeyValueStore;
use nanoserde::{DeJson, SerJson};

/// Persistent schema version; a blob with any other value is discarded.
pub const SCHEMA_VERSION: u32 = 1;

/// The full persisted Sing Back state. JSON keys: `schemaVersion`,
/// `generation`, `bestSpan`, `lastSeen`.
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct SingBackState {
    #[nserde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// Reset generation: +1 ONLY on a parent start-over (never on a span
    /// improvement). A higher generation = a more-recent reset, and wins the
    /// merge. `#[nserde(default)]` → an absent key (e.g. an older blob) reads
    /// as generation 0.
    #[nserde(default)]
    pub generation: u32,
    /// Longest sequence length the kid has correctly reproduced.
    #[nserde(rename = "bestSpan")]
    pub best_span: u32,
    /// epoch ms of the last mutating change; 0 = never.
    #[nserde(rename = "lastSeen")]
    pub last_seen: i64,
}

/// Fresh / empty state: `{ schemaVersion: 1, generation: 0, bestSpan: 0, lastSeen: 0 }`.
pub fn empty_state() -> SingBackState {
    SingBackState {
        schema_version: SCHEMA_VERSION,
        generation: 0,
        best_span: 0,
        last_seen: 0,
    }
}

/// Validate a raw JSON blob into a `SingBackState`. Returns `None` (→ caller
/// falls back to `empty_state`) unless it parses AND `schemaVersion ==
/// SCHEMA_VERSION` (1). nanoserde's typed deserialize enforces the field shape;
/// an absent `generation` (e.g. an older blob) defaults to 0.
pub fn validate(json: &str) -> Option<SingBackState> {
    let state = SingBackState::deserialize_json(json).ok()?;
    if state.schema_version != SCHEMA_VERSION {
        return None;
    }
    Some(state)
}

/// Record a finished round of length `span`. MONOTONIC: only raises `best_span`
/// when `span` beats it, stamping `last_seen`; `generation` is UNCHANGED. A
/// non-improvement is a no-op so ordinary play doesn't churn sync state.
pub fn record_span(state: &mut SingBackState, span: u32, now: i64) {
    if span > state.best_span {
        state.best_span = span;
        state.last_seen = now;
    }
}

/// Start-over state: `best_span = 0` with `generation = state.generation + 1`
/// and `last_seen = now`. The bumped generation lets a parent's reset out-rank
/// the pre-reset entries in the merge (below) instead of being resurrected by
/// the first pull — this is the only path that lowers `best_span`.
pub fn start_over(state: &SingBackState, now: i64) -> SingBackState {
    SingBackState {
        schema_version: SCHEMA_VERSION,
        generation: state.generation + 1,
        best_span: 0,
        last_seen: now,
    }
}

/// Merge a remote state into local (cross-device sync). GENERATION + MAX:
/// - `generation` = max of the two.
/// - `best_span`: within the SAME generation, `max(local, remote)` — so a sync
///   can never lower the kid's best (monotonic). When generations differ, take
///   the `best_span` of the side with the HIGHER generation — its reset is more
///   recent and must propagate (a plain `max(best_span)` would silently undo it).
/// - `last_seen` = max of the two.
///
/// Commutative + idempotent: max is symmetric, and the generations either tie
/// (→ symmetric max best_span) or one strictly exceeds the other (→ that side's
/// best_span, picked identically either order). Merging a state with itself is a
/// no-op. The rare two-independent-resets case (each side bumped generation
/// offline) is resolved best-effort by max generation — acceptable here.
pub fn merge(local: &SingBackState, remote: &SingBackState, _now: i64) -> SingBackState {
    let best_span = match local.generation.cmp(&remote.generation) {
        std::cmp::Ordering::Equal => local.best_span.max(remote.best_span),
        std::cmp::Ordering::Greater => local.best_span,
        std::cmp::Ordering::Less => remote.best_span,
    };
    SingBackState {
        schema_version: SCHEMA_VERSION,
        generation: local.generation.max(remote.generation),
        best_span,
        last_seen: local.last_seen.max(remote.last_seen),
    }
}

/// Load the Sing Back state from `fountouki.singback.state.v1`: validated
/// current schema, else fresh. (`now` is unused today but kept for parity with
/// `tracing::load`'s signature and any future migration.)
pub fn load<S: KeyValueStore + ?Sized>(store: &S, _now: i64) -> SingBackState {
    store
        .get(&crate::storage::ns_key("singback", "state"))
        .and_then(|raw| validate(&raw))
        .unwrap_or_else(empty_state)
}

/// Persist the whole state to `fountouki.singback.state.v1`.
pub fn save<S: KeyValueStore + ?Sized>(store: &mut S, state: &SingBackState) {
    store.set(
        &crate::storage::ns_key("singback", "state"),
        &state.serialize_json(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemStore;

    #[test]
    fn empty_state_shape() {
        let s = empty_state();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.generation, 0);
        assert_eq!(s.best_span, 0);
        assert_eq!(s.last_seen, 0);
    }

    #[test]
    fn record_raises_span_only_and_never_bumps_generation() {
        let mut s = empty_state();
        record_span(&mut s, 3, 100);
        assert_eq!((s.best_span, s.generation, s.last_seen), (3, 0, 100));
        // A worse round: no change at all (no churn).
        record_span(&mut s, 2, 200);
        assert_eq!((s.best_span, s.generation, s.last_seen), (3, 0, 100));
        // Equal round: still a no-op (strict improvement only).
        record_span(&mut s, 3, 300);
        assert_eq!((s.best_span, s.generation, s.last_seen), (3, 0, 100));
        // A better round: raises span + stamps last_seen, generation UNCHANGED.
        record_span(&mut s, 5, 400);
        assert_eq!((s.best_span, s.generation, s.last_seen), (5, 0, 400));
    }

    #[test]
    fn record_never_decrements() {
        let mut s = empty_state();
        for (span, now) in [(4, 10), (2, 20), (7, 30), (1, 40), (5, 50)] {
            let before = s.best_span;
            record_span(&mut s, span, now);
            assert!(s.best_span >= before, "best_span went backwards");
        }
        assert_eq!(s.best_span, 7); // the max ever seen
    }

    #[test]
    fn start_over_resets_to_zero_and_bumps_generation() {
        let mut s = empty_state();
        record_span(&mut s, 6, 100); // gen still 0
        let reset = start_over(&s, 9_000);
        assert_eq!(reset.best_span, 0);
        assert_eq!(reset.generation, s.generation + 1);
        assert_eq!(reset.last_seen, 9_000);
    }

    #[test]
    fn merge_is_monotonic_under_record_on_both_sides() {
        // Two devices that played independently (same gen 0); the larger span
        // wins, never regressing the kid's best.
        let mut a = empty_state();
        record_span(&mut a, 4, 100);
        record_span(&mut a, 6, 200); // span 6
        let mut b = empty_state();
        record_span(&mut b, 3, 150); // span 3
        let m1 = merge(&a, &b, 999);
        let m2 = merge(&b, &a, 999);
        assert_eq!(m1.best_span, 6);
        assert_eq!(m2.best_span, 6); // order-independent
        assert_eq!(m1.generation, 0);
    }

    /// The EXACT review repro: a device with MORE improvements but a SMALLER
    /// best must NOT win and drag best_span down. With the generation+max model
    /// (record_span never bumps generation), both sides share gen 0 → best_span
    /// is the max, never the "more improvements" side's smaller value.
    #[test]
    fn merge_more_improvements_smaller_best_never_lowers() {
        // Device A: 3 improvements ending at best 10 (was the version-3 side).
        let mut a = empty_state();
        record_span(&mut a, 4, 10);
        record_span(&mut a, 7, 20);
        record_span(&mut a, 10, 30); // best 10, gen 0
        // Device B: 2 improvements ending at best 20 (was the version-2 side).
        let mut b = empty_state();
        record_span(&mut b, 12, 15);
        record_span(&mut b, 20, 25); // best 20, gen 0
        let m1 = merge(&a, &b, 999);
        let m2 = merge(&b, &a, 999);
        assert_eq!(m1.best_span, 20, "best_span dropped (the inversion bug)");
        assert_eq!(m2.best_span, 20, "order-dependent / dropped");
        assert_eq!(m1.generation, 0);
    }

    /// A device that never reset (gen0, best30) merged with a peer that did one
    /// start_over (gen1, best0): the reset propagates → best_span becomes 0,
    /// regardless of merge order.
    #[test]
    fn merge_higher_generation_reset_propagates() {
        let mut never_reset = empty_state();
        record_span(&mut never_reset, 30, 100); // gen 0, best 30
        let reset = start_over(&empty_state(), 500); // gen 1, best 0
        let m1 = merge(&never_reset, &reset, 999);
        let m2 = merge(&reset, &never_reset, 999);
        assert_eq!(m1.generation, 1);
        assert_eq!(m1.best_span, 0, "reset did not propagate");
        assert_eq!(m2.best_span, 0, "reset did not propagate (other order)");
    }

    /// After a reset propagates, ordinary play on the higher generation merges
    /// normally (max within the new generation).
    #[test]
    fn merge_after_reset_resumes_max_within_generation() {
        let reset = start_over(&empty_state(), 500); // gen 1, best 0
        let mut played = reset.clone();
        record_span(&mut played, 4, 600); // gen 1, best 4
        let m = merge(&reset, &played, 999);
        assert_eq!(m.generation, 1);
        assert_eq!(m.best_span, 4);
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        // A spread of generations / spans / last_seen.
        let cases = [
            (empty_state(), empty_state()),
            (
                SingBackState { schema_version: 1, generation: 0, best_span: 10, last_seen: 30 },
                SingBackState { schema_version: 1, generation: 0, best_span: 20, last_seen: 25 },
            ),
            (
                SingBackState { schema_version: 1, generation: 0, best_span: 30, last_seen: 100 },
                SingBackState { schema_version: 1, generation: 1, best_span: 0, last_seen: 500 },
            ),
            (
                SingBackState { schema_version: 1, generation: 2, best_span: 5, last_seen: 7 },
                SingBackState { schema_version: 1, generation: 1, best_span: 99, last_seen: 9 },
            ),
        ];
        for (a, b) in cases {
            // Commutative.
            assert_eq!(merge(&a, &b, 0), merge(&b, &a, 0), "merge not commutative");
            // Idempotent: merging a state with itself is a no-op.
            assert_eq!(merge(&a, &a, 0), a, "merge(a, a) != a");
            assert_eq!(merge(&b, &b, 0), b, "merge(b, b) != b");
            // Idempotent: re-merging the merged result changes nothing.
            let m = merge(&a, &b, 0);
            assert_eq!(merge(&m, &a, 0), m, "re-merge with a changed it");
            assert_eq!(merge(&m, &b, 0), m, "re-merge with b changed it");
        }
    }

    #[test]
    fn validate_accepts_good_blob_and_rejects_wrong_schema() {
        let good = r#"{"schemaVersion":1,"generation":3,"bestSpan":5,"lastSeen":1748600100000}"#;
        let s = validate(good).expect("should validate");
        assert_eq!((s.generation, s.best_span, s.last_seen), (3, 5, 1748600100000));
        let wrong = r#"{"schemaVersion":2,"generation":0,"bestSpan":0,"lastSeen":0}"#;
        assert!(validate(wrong).is_none());
        assert!(validate("not json").is_none());
    }

    /// A blob missing `generation` (e.g. an older shape) still validates and
    /// defaults generation to 0 — tolerated, per the spec.
    #[test]
    fn validate_tolerates_absent_generation() {
        let no_gen = r#"{"schemaVersion":1,"bestSpan":5,"lastSeen":1234}"#;
        let s = validate(no_gen).expect("should validate without generation");
        assert_eq!((s.generation, s.best_span, s.last_seen), (0, 5, 1234));
    }

    #[test]
    fn json_keys_are_exact_camel_case_and_roundtrip() {
        let s = SingBackState {
            schema_version: 1,
            generation: 7,
            best_span: 9,
            last_seen: 1234,
        };
        let json = s.serialize_json();
        assert!(json.contains("\"schemaVersion\":1"), "json: {json}");
        assert!(json.contains("\"generation\":7"), "json: {json}");
        assert!(json.contains("\"bestSpan\":9"), "json: {json}");
        assert!(json.contains("\"lastSeen\":1234"), "json: {json}");
        // No snake_case leakage.
        assert!(!json.contains("schema_version"), "json: {json}");
        assert!(!json.contains("best_span"), "json: {json}");
        assert!(!json.contains("last_seen"), "json: {json}");
        // The old field name is gone.
        assert!(!json.contains("\"version\""), "json: {json}");
        // Round-trips.
        assert_eq!(SingBackState::deserialize_json(&json).unwrap(), s);
    }

    #[test]
    fn load_falls_back_to_fresh_and_save_roundtrips() {
        let mut store = MemStore::new();
        // Nothing stored → fresh.
        assert_eq!(load(&store, 0), empty_state());
        // Garbage → fresh.
        store.set(&crate::storage::ns_key("singback", "state"), "not json");
        assert_eq!(load(&store, 0).generation, 0);
        // Save → load roundtrips.
        let mut s = empty_state();
        record_span(&mut s, 4, 555);
        save(&mut store, &s);
        assert_eq!(load(&store, 0), s);
    }
}
