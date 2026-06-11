//! Per-letter Leitner SRS — a spaced-repetition system plus a drip-in gate
//! that controls which letters are active. Shared by phonics (over
//! `deck::INTRO_ORDER`, Jolly Phonics) and tracing (over `tracing::ORDER`,
//! motor-skill groups): the order-dependent functions take the introduction
//! order as a parameter.
//!
//! Persistent model (`LeitnerState`) is the ONLY thing saved/synced. JSON key
//! names + numeric constants are load-bearing: they must match the existing TS
//! clients byte-for-byte so save files and cross-device sync interoperate.
//!
//! Transcribed from `src/games/phonics/srs.ts` and docs/port-spec/phonics.md +
//! storage-sync.md §1.6.

use std::collections::HashMap;

use nanoserde::{DeJson, SerJson};

use crate::deck::LETTERS;
use crate::rng::Mulberry32;

// --- Constants (load-bearing) ----------------------------------------------

/// Persistent schema version; a blob with any other value is discarded.
pub const SCHEMA_VERSION: u32 = 1;
/// Top Leitner box (mastered).
pub const MAX_BOX: u8 = 4;
/// box >= this → "introduced" (graded correct at least once).
pub const INTRODUCED_BOX_MIN: u8 = 1;
/// box >= this (and < MASTERED_BOX) → "strong" bucket (parent view).
pub const STRONG_MIN_BOX: u8 = 3;
/// box >= this → "mastered" bucket (parent view).
pub const MASTERED_BOX: u8 = 4;
/// Max simultaneous not-yet-settled letters allowed active (drip-in gate).
pub const NEW_LETTER_BUFFER: usize = 3;
/// Cards between a miss / dup-swap and that letter re-appearing.
pub const REQUEUE_GAP: usize = 4;

const MIN: i64 = 60_000; // 1 minute in ms
const HOUR: i64 = 3_600_000; // 1 hour in ms

// --- Persistent state -------------------------------------------------------

/// Per-letter Leitner state. `box` is a Rust keyword, so the field is `box_`
/// renamed to the JSON key `"box"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SerJson, DeJson)]
pub struct LetterState {
    /// Leitner box, 0..=MAX_BOX. 0 = new/just-missed, 4 = mastered.
    #[nserde(rename = "box")]
    pub box_: u8,
    /// epoch ms; the letter is "ready" when `due <= now`.
    pub due: i64,
    /// epoch ms of the last grade; 0 = never graded.
    #[nserde(rename = "lastSeen")]
    pub last_seen: i64,
}

/// The full persisted per-game Leitner state. JSON keys: `schemaVersion`,
/// `version`, `letters` (a map keyed by the single-character lowercase letter
/// string).
#[derive(Clone, Debug, PartialEq, Eq, SerJson, DeJson)]
pub struct LeitnerState {
    #[nserde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// Monotonic counter, +1 on every mutating grade.
    pub version: u64,
    /// Per-letter state, keyed by the lowercase letter as a 1-char string.
    /// HashMap (not BTreeMap) because nanoserde only derives maps for HashMap;
    /// helpers sort keys explicitly wherever deterministic order is needed.
    pub letters: HashMap<String, LetterState>,
}

/// Fresh / empty state: `{ schemaVersion: 1, version: 0, letters: {} }`.
pub fn empty_state() -> LeitnerState {
    LeitnerState {
        schema_version: SCHEMA_VERSION,
        version: 0,
        letters: HashMap::new(),
    }
}

/// For every deck letter (`LETTERS`, a..z) missing from `state.letters`,
/// insert `{ box: 0, due: now, lastSeen: 0 }`. Mutates in place.
///
/// Called right after load and after every remote merge so a loaded state
/// always has all 26 letters with at least a box-0 default.
pub fn ensure_letters(state: &mut LeitnerState, now: i64) {
    for &c in LETTERS.iter() {
        let key = c.to_string();
        state.letters.entry(key).or_insert(LetterState {
            box_: 0,
            due: now,
            last_seen: 0,
        });
    }
}

/// Leitner interval in ms for a box: 0→0, 1→2min, 2→15min, 3→6h, else→24h.
///
/// Early ramp is short so a kid sees the same card 2–3× in one ~5-min session;
/// long park between sessions.
pub fn interval_for(box_: u8) -> i64 {
    match box_ {
        0 => 0,
        1 => 2 * MIN,
        2 => 15 * MIN,
        3 => 6 * HOUR,
        _ => 24 * HOUR,
    }
}

/// Promote on a correct grade: `box = min(MAX_BOX, box+1)`, `due = now +
/// interval(new box)`, `last_seen = now`.
///
/// NOTE: does NOT bump `state.version` — the caller owns the version counter
/// (`gotIt(state, letter, now)` in the TS does `state.version += 1`). This
/// mirrors that split so the grade math stays a pure transition on one letter.
pub fn got_it(ls: &mut LetterState, now: i64) {
    ls.box_ = (ls.box_ + 1).min(MAX_BOX);
    ls.due = now + interval_for(ls.box_);
    ls.last_seen = now;
}

/// Soft-decay on a miss: `box = max(0, box-1)` (drop ONE box, not to 0),
/// `due = now + interval(new box)`, `last_seen = now`.
///
/// One wobble must not blow away days of spacing. A box-1 letter missed
/// returns to box 0 and re-counts as "unintroduced" (see the active-set gate),
/// giving the kid breathing room. Caller owns the version bump.
pub fn missed(ls: &mut LetterState, now: i64) {
    ls.box_ = ls.box_.saturating_sub(1);
    ls.due = now + interval_for(ls.box_);
    ls.last_seen = now;
}

/// Grade a letter correct on the whole state, bumping `version`. No-op (no
/// version bump) if the letter isn't in the map. Convenience over `got_it`.
pub fn grade_got_it(state: &mut LeitnerState, letter: char, now: i64) {
    if let Some(ls) = state.letters.get_mut(&letter.to_string()) {
        got_it(ls, now);
        state.version += 1;
    }
}

/// Grade a letter missed on the whole state, bumping `version`. No-op (no
/// version bump) if the letter isn't in the map. Convenience over `missed`.
pub fn grade_missed(state: &mut LeitnerState, letter: char, now: i64) {
    if let Some(ls) = state.letters.get_mut(&letter.to_string()) {
        missed(ls, now);
        state.version += 1;
    }
}

/// Validate a raw JSON blob into a `LeitnerState`. Returns `None` (→ caller
/// falls back to `empty_state`) unless ALL hold:
/// - parses as an object,
/// - `schemaVersion == SCHEMA_VERSION` (1),
/// - `version` is a number,
/// - `letters` is an object.
///
/// On success keeps only `{ schemaVersion, version, letters }` (any extra
/// fields are dropped) and the per-letter fields verbatim (no coercion beyond
/// what the typed deserialize already enforces).
pub fn validate(json: &str) -> Option<LeitnerState> {
    // nanoserde's typed deserialize already enforces: object shape, that
    // `version` is numeric (u64), and that `letters` is an object of
    // `LetterState`. A missing/malformed field fails → None.
    let state = LeitnerState::deserialize_json(json).ok()?;
    if state.schema_version != SCHEMA_VERSION {
        return None;
    }
    Some(LeitnerState {
        schema_version: SCHEMA_VERSION,
        version: state.version,
        letters: state.letters,
    })
}

/// Merge a remote state into local (cross-device sync). Per-letter winner =
/// the entry with the larger `lastSeen`; ties keep `local` (strict `>` keeps
/// `a`). Union of keys. `version = max(local, remote)`, `schemaVersion = 1`,
/// then `ensure_letters` so the result is fully populated.
///
/// `now` is used by the trailing `ensure_letters` for any letter present in
/// neither side.
pub fn merge(local: &LeitnerState, remote: &LeitnerState, now: i64) -> LeitnerState {
    let mut letters: HashMap<String, LetterState> = local.letters.clone();
    for (key, r) in remote.letters.iter() {
        match letters.get(key) {
            // In both: remote wins only on strictly larger lastSeen.
            Some(l) => {
                if r.last_seen > l.last_seen {
                    letters.insert(key.clone(), *r);
                }
            }
            // Only in remote: take it.
            None => {
                letters.insert(key.clone(), *r);
            }
        }
    }
    let mut merged = LeitnerState {
        schema_version: SCHEMA_VERSION,
        version: local.version.max(remote.version),
        letters,
    };
    ensure_letters(&mut merged, now);
    merged
}

// --- Active set (drip-in gate) ---------------------------------------------

/// Box for `letter` in `state`, defaulting to 0 if absent.
fn box_of(state: &LeitnerState, letter: char) -> u8 {
    state
        .letters
        .get(&letter.to_string())
        .map(|ls| ls.box_)
        .unwrap_or(0)
}

/// The letters eligible to be queued right now (the drip-in frontier).
///
/// Walk `order` from the start, gathering letters until the frontier is
/// hit, then STOP. A letter counts as "unsettled" (a consumed buffer slot)
/// while its box < `INTRODUCED_BOX_MIN`. Once `NEW_LETTER_BUFFER` unsettled
/// letters are active, the frontier is reached and the loop breaks.
///
/// CRITICAL: the `break` gates BOTH branches (introduced AND new). Letters
/// *beyond* the frontier — even ones already at box >= 1 from a legacy /
/// out-of-order state — are PARKED: their box is retained but they are not
/// queued until the kid drips far enough down `order` to reach them.
/// (If the gate only counted box-0 letters, a tail polluted to box >= 1 would
/// leak the entire tail, since no box-0 letter would remain to stop on.)
///
/// Fresh learner (all box 0) → the first 3 letters of `order` (phonics:
/// `s, a, t`; tracing: `c, a, d`).
pub fn active_letters(state: &LeitnerState, order: &[char]) -> Vec<char> {
    let mut active = Vec::new();
    let mut unsettled = 0usize;
    for &letter in order.iter() {
        if unsettled >= NEW_LETTER_BUFFER {
            break; // frontier reached — stop (gates both branches)
        }
        active.push(letter);
        if box_of(state, letter) < INTRODUCED_BOX_MIN {
            unsettled += 1;
        }
    }
    active
}

// --- Queue building ---------------------------------------------------------

/// Build the play queue: a permutation of currently-active letters.
///
/// ```text
/// active = activeLetters(state, order).filter(present in state.letters)
/// due    = active.filter(due <= now)
/// if !due.is_empty():  return shuffle(due)             // due-preferred
/// else:                shuffle(active); stable-sort by box asc; return
/// ```
///
/// - Due letters preferred → genuine SRS spacing across a day. The queue is a
///   permutation: each active letter appears once before any repeat.
/// - Shuffled (not due-sorted) so consecutive sessions don't replay the same
///   recency-ordered sequence.
/// - Fallback (nothing due): shuffle all active, then STABLE-sort by box
///   ascending → weaker letters first, within-box order stays shuffled.
/// - Avoiding the same letter twice in a row across rebuilds is the caller's
///   job (`avoid_repeat`), not this function's.
pub fn build_queue(
    state: &LeitnerState,
    order: &[char],
    now: i64,
    rng: &mut Mulberry32,
) -> Vec<char> {
    let active: Vec<char> = active_letters(state, order)
        .into_iter()
        .filter(|l| state.letters.contains_key(&l.to_string()))
        .collect();

    let mut due: Vec<char> = active
        .iter()
        .copied()
        .filter(|l| {
            state
                .letters
                .get(&l.to_string())
                .map(|ls| ls.due <= now)
                .unwrap_or(false)
        })
        .collect();

    if !due.is_empty() {
        rng.shuffle(&mut due);
        return due;
    }

    // Fallback: shuffle all active, then STABLE sort by box ascending.
    let mut all = active;
    rng.shuffle(&mut all);
    all.sort_by_key(|l| box_of(state, *l)); // Rust's sort_by_key is stable
    all
}

/// Caller-side no-back-to-back constraint (`REQUEUE_GAP = 4`). Given a freshly
/// rebuilt `queue` and the `last_letter` just shown, if the queue's front is
/// that same letter and there's an alternative, move the duplicate deeper to
/// index `min(REQUEUE_GAP, queue.len()-1)` so a different letter leads.
///
/// Mirrors `showNextCard`'s dup-swap: never the same letter twice in a row
/// when there's an alternative. With a 1-element queue (no alternative) it is
/// a no-op — the caller will simply re-show the only active letter.
pub fn avoid_repeat(queue: &mut Vec<char>, last_letter: Option<char>) {
    let last = match last_letter {
        Some(l) => l,
        None => return,
    };
    if queue.len() < 2 {
        return; // no alternative
    }
    if queue[0] != last {
        return;
    }
    let dup = queue.remove(0);
    // Re-insert deeper; clamp to the now-shortened queue length.
    let idx = REQUEUE_GAP.min(queue.len());
    queue.insert(idx, dup);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::INTRO_ORDER;

    fn ls(box_: u8, due: i64, last_seen: i64) -> LetterState {
        LetterState {
            box_,
            due,
            last_seen,
        }
    }

    fn fresh(now: i64) -> LeitnerState {
        let mut s = empty_state();
        ensure_letters(&mut s, now);
        s
    }

    // --- empty / ensure ---

    #[test]
    fn empty_state_shape() {
        let s = empty_state();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.version, 0);
        assert!(s.letters.is_empty());
    }

    #[test]
    fn ensure_letters_adds_all_26_box0() {
        let mut s = empty_state();
        ensure_letters(&mut s, 1000);
        assert_eq!(s.letters.len(), 26);
        for &c in LETTERS.iter() {
            let e = s.letters.get(&c.to_string()).unwrap();
            assert_eq!(*e, ls(0, 1000, 0));
        }
    }

    #[test]
    fn ensure_letters_does_not_clobber_existing() {
        let mut s = empty_state();
        s.letters.insert("s".into(), ls(3, 999, 42));
        ensure_letters(&mut s, 1000);
        assert_eq!(*s.letters.get("s").unwrap(), ls(3, 999, 42));
        assert_eq!(s.letters.len(), 26);
    }

    // --- interval table ---

    #[test]
    fn interval_table_matches_spec() {
        assert_eq!(interval_for(0), 0);
        assert_eq!(interval_for(1), 2 * 60_000);
        assert_eq!(interval_for(2), 15 * 60_000);
        assert_eq!(interval_for(3), 6 * 60 * 60_000);
        assert_eq!(interval_for(4), 24 * 60 * 60_000);
        assert_eq!(interval_for(9), 24 * 60 * 60_000); // >=4 clamps
    }

    // --- grade transitions ---

    #[test]
    fn got_it_promotes_and_sets_due_from_new_box() {
        let now = 1_000_000;
        let mut e = ls(0, now, 0);
        got_it(&mut e, now);
        assert_eq!(e.box_, 1);
        assert_eq!(e.due, now + 2 * 60_000); // interval for NEW box (1)
        assert_eq!(e.last_seen, now);
    }

    #[test]
    fn got_it_caps_at_max_box() {
        let now = 5;
        let mut e = ls(4, 0, 0);
        got_it(&mut e, now);
        assert_eq!(e.box_, 4);
        assert_eq!(e.due, now + interval_for(4));
    }

    #[test]
    fn missed_soft_decays_one_box() {
        let now = 2_000_000;
        let mut e = ls(2, 0, 0);
        missed(&mut e, now);
        assert_eq!(e.box_, 1);
        assert_eq!(e.due, now + 2 * 60_000); // interval for NEW box (1)
        assert_eq!(e.last_seen, now);
    }

    #[test]
    fn missed_floors_at_zero() {
        let now = 7;
        let mut e = ls(0, 0, 0);
        missed(&mut e, now);
        assert_eq!(e.box_, 0);
        assert_eq!(e.due, now); // interval 0
    }

    #[test]
    fn box1_missed_returns_to_box0_unintroduced() {
        let now = 100;
        let mut e = ls(1, 0, 50);
        missed(&mut e, now);
        assert_eq!(e.box_, 0); // re-counts as unintroduced
    }

    #[test]
    fn grade_helpers_bump_version_and_noop_on_unknown() {
        let now = 1234;
        let mut s = fresh(now);
        assert_eq!(s.version, 0);
        grade_got_it(&mut s, 's', now);
        assert_eq!(s.version, 1);
        assert_eq!(s.letters.get("s").unwrap().box_, 1);
        grade_missed(&mut s, 's', now);
        assert_eq!(s.version, 2);
        assert_eq!(s.letters.get("s").unwrap().box_, 0);
        // Unknown letter: no-op, no version bump.
        grade_got_it(&mut s, 'A', now); // not in map (uppercase)
        assert_eq!(s.version, 2);
    }

    // --- active set / drip-in gate ---

    #[test]
    fn fresh_learner_active_is_s_a_t() {
        let s = fresh(0);
        assert_eq!(active_letters(&s, &INTRO_ORDER), vec!['s', 'a', 't']);
    }

    #[test]
    fn promotion_unlocks_next_intro_letter() {
        let now = 1000;
        let mut s = fresh(now);
        // Grade 's' correct → box 1 → frees a buffer slot.
        grade_got_it(&mut s, 's', now);
        // Now s is settled; the gate should reach the 4th INTRO letter 'i'.
        // active walks: s(box1,settled) a(0) t(0) i(0) -> after pushing i,
        // unsettled hits 3 and the next iteration breaks.
        assert_eq!(active_letters(&s, &INTRO_ORDER), vec!['s', 'a', 't', 'i']);
    }

    #[test]
    fn unlock_chains_as_more_settle() {
        let now = 1000;
        let mut s = fresh(now);
        for l in ['s', 'a', 't'] {
            grade_got_it(&mut s, l, now);
        }
        // s,a,t all box1 (settled). Unsettled budget refills with i,p,n.
        assert_eq!(active_letters(&s, &INTRO_ORDER), vec!['s', 'a', 't', 'i', 'p', 'n']);
    }

    #[test]
    fn legacy_polluted_state_stays_within_frontier() {
        // i,h,m at box 0; everything else at box 2. Frontier = up to & incl m.
        let now = 0;
        let mut s = fresh(now);
        for &c in LETTERS.iter() {
            s.letters.get_mut(&c.to_string()).unwrap().box_ = 2;
        }
        for c in ['i', 'h', 'm'] {
            s.letters.get_mut(&c.to_string()).unwrap().box_ = 0;
        }
        let active = active_letters(&s, &INTRO_ORDER);
        // INTRO_ORDER[0..=index_of('m')] = s,a,t,i,p,n,c,k,e,h,r,m
        assert_eq!(
            active,
            vec!['s', 'a', 't', 'i', 'p', 'n', 'c', 'k', 'e', 'h', 'r', 'm']
        );
        // The tail must NEVER surface.
        for parked in ['x', 'v', 'q', 'd', 'g', 'b'] {
            assert!(
                !active.contains(&parked),
                "{parked} must be parked beyond the frontier"
            );
        }
    }

    // --- build_queue ---

    #[test]
    fn build_queue_fresh_is_permutation_of_s_a_t() {
        let now = 1000;
        let s = fresh(now); // all due == now <= now => all due
        let mut rng = Mulberry32::new(1);
        let mut q = build_queue(&s, &INTRO_ORDER, now, &mut rng);
        q.sort_unstable();
        assert_eq!(q, vec!['a', 's', 't']);
    }

    #[test]
    fn build_queue_due_preferred() {
        let now = 1_000_000;
        let mut s = fresh(now);
        // Make 's' and 'a' not due (future), 't' due.
        s.letters.get_mut("s").unwrap().due = now + 10_000;
        s.letters.get_mut("a").unwrap().due = now + 10_000;
        s.letters.get_mut("t").unwrap().due = now - 1;
        let mut rng = Mulberry32::new(3);
        let q = build_queue(&s, &INTRO_ORDER, now, &mut rng);
        assert_eq!(q, vec!['t']); // only the due one
    }

    #[test]
    fn build_queue_fallback_stable_sorts_by_box_asc() {
        let now = 1_000_000;
        let mut s = fresh(now);
        // Nothing due: push EVERY letter's due into the future (otherwise the
        // box-0 frontier letters i/p stay due and the due-preferred branch fires).
        for st in s.letters.values_mut() {
            st.due = now + 10_000;
        }
        // Distinct boxes across the frontier.
        s.letters.get_mut("s").unwrap().box_ = 2;
        s.letters.get_mut("a").unwrap().box_ = 0;
        s.letters.get_mut("t").unwrap().box_ = 1;
        let mut rng = Mulberry32::new(9);
        let q = build_queue(&s, &INTRO_ORDER, now, &mut rng);
        // Fallback returns all active letters, stable-sorted by box ascending.
        let mut active = active_letters(&s, &INTRO_ORDER);
        active.sort_unstable();
        let mut got = q.clone();
        got.sort_unstable();
        assert_eq!(got, active, "queue must be a permutation of active letters");
        // Weaker (lower box) first → boxes non-decreasing along the queue.
        for w in q.windows(2) {
            assert!(box_of(&s, w[0]) <= box_of(&s, w[1]), "not box-asc: {q:?}");
        }
    }

    #[test]
    fn build_queue_is_shuffled_across_seeds() {
        // Multiple due letters; >=2 distinct first cards across seeds.
        let now = 1000;
        let mut s = fresh(now);
        for l in ['s', 'a', 't'] {
            grade_got_it(&mut s, l, now); // box1; due in future, but...
            s.letters.get_mut(&l.to_string()).unwrap().due = now - 1; // force due
        }
        // Also unlock i,p,n as due so the queue has 6 due letters.
        for l in ['i', 'p', 'n'] {
            s.letters.get_mut(&l.to_string()).unwrap().due = now - 1;
        }
        let mut firsts = std::collections::HashSet::new();
        for seed in 0..8u32 {
            let mut rng = Mulberry32::new(seed.wrapping_mul(2654435761).wrapping_add(1));
            let q = build_queue(&s, &INTRO_ORDER, now, &mut rng);
            firsts.insert(q[0]);
        }
        assert!(
            firsts.len() >= 2,
            "8 mounts opened on the same letter every time: {firsts:?}"
        );
    }

    // --- avoid_repeat ---

    #[test]
    fn avoid_repeat_moves_dup_deeper() {
        let mut q = vec!['s', 'a', 't', 'i', 'p', 'n'];
        avoid_repeat(&mut q, Some('s'));
        // 's' removed from front, reinserted at min(REQUEUE_GAP=4, len=5)=4.
        assert_ne!(q[0], 's');
        assert_eq!(q[0], 'a');
        assert_eq!(q[4], 's');
        assert_eq!(q.len(), 6);
    }

    #[test]
    fn avoid_repeat_clamps_to_short_queue() {
        let mut q = vec!['s', 'a'];
        avoid_repeat(&mut q, Some('s'));
        // len after remove = 1; insert at min(4,1)=1 → ['a','s'].
        assert_eq!(q, vec!['a', 's']);
    }

    #[test]
    fn avoid_repeat_noop_when_front_differs_or_singleton() {
        let mut q = vec!['a', 's'];
        avoid_repeat(&mut q, Some('s'));
        assert_eq!(q, vec!['a', 's']); // front already differs
        let mut single = vec!['s'];
        avoid_repeat(&mut single, Some('s'));
        assert_eq!(single, vec!['s']); // no alternative
        let mut q2 = vec!['s', 'a'];
        avoid_repeat(&mut q2, None);
        assert_eq!(q2, vec!['s', 'a']); // no last letter
    }

    #[test]
    fn queue_never_starts_with_just_shown_letter() {
        // Rebuild + avoid_repeat must not lead with the last shown letter
        // when an alternative exists, across many seeds.
        let now = 1000;
        let s = fresh(now); // s,a,t all due
        for seed in 0..40u32 {
            let mut rng = Mulberry32::new(seed.wrapping_add(7));
            let mut q = build_queue(&s, &INTRO_ORDER, now, &mut rng);
            let last = Some('s');
            avoid_repeat(&mut q, last);
            assert_ne!(q[0], 's', "seed {seed} led with the just-shown letter");
        }
    }

    // --- validate ---

    #[test]
    fn validate_accepts_good_blob() {
        let json = r#"{"schemaVersion":1,"version":3,"letters":{"s":{"box":2,"due":1748600900000,"lastSeen":1748600100000}}}"#;
        let s = validate(json).expect("should validate");
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.version, 3);
        assert_eq!(
            *s.letters.get("s").unwrap(),
            ls(2, 1748600900000, 1748600100000)
        );
    }

    #[test]
    fn validate_rejects_wrong_schema() {
        let json = r#"{"schemaVersion":2,"version":0,"letters":{}}"#;
        assert!(validate(json).is_none());
    }

    #[test]
    fn validate_rejects_missing_letters() {
        let json = r#"{"schemaVersion":1,"version":0}"#;
        assert!(validate(json).is_none());
    }

    #[test]
    fn validate_rejects_garbage() {
        assert!(validate("not json").is_none());
        assert!(validate("null").is_none());
        assert!(validate("[]").is_none());
    }

    #[test]
    fn validate_drops_extra_fields_roundtrip_keys() {
        // Extra unknown top-level fields are ignored by nanoserde; the kept
        // shape serializes with exactly the 3 canonical keys.
        let json = r#"{"schemaVersion":1,"version":5,"letters":{},"junk":true}"#;
        let s = validate(json).expect("validate");
        let out = s.serialize_json();
        assert!(out.contains("\"schemaVersion\":1"));
        assert!(out.contains("\"version\":5"));
        assert!(out.contains("\"letters\""));
        assert!(!out.contains("junk"));
    }

    #[test]
    fn json_keys_are_exact() {
        // Per-letter keys must be box / due / lastSeen.
        let mut s = empty_state();
        s.letters.insert("s".into(), ls(2, 1000, 900));
        let out = s.serialize_json();
        assert!(out.contains("\"box\":2"), "got: {out}");
        assert!(out.contains("\"due\":1000"), "got: {out}");
        assert!(out.contains("\"lastSeen\":900"), "got: {out}");
        assert!(out.contains("\"schemaVersion\":1"));
        // Round-trips.
        let back = LeitnerState::deserialize_json(&out).unwrap();
        assert_eq!(back, s);
    }

    // --- merge ---

    #[test]
    fn merge_picks_higher_last_seen() {
        let now = 0;
        let mut local = empty_state();
        let mut remote = empty_state();
        local.letters.insert("s".into(), ls(1, 100, 50));
        remote.letters.insert("s".into(), ls(3, 200, 80)); // newer
        local.letters.insert("a".into(), ls(2, 100, 90)); // newer locally
        remote.letters.insert("a".into(), ls(0, 200, 40));
        let m = merge(&local, &remote, now);
        assert_eq!(*m.letters.get("s").unwrap(), ls(3, 200, 80)); // remote won
        assert_eq!(*m.letters.get("a").unwrap(), ls(2, 100, 90)); // local won
    }

    #[test]
    fn merge_tie_keeps_local() {
        let now = 0;
        let mut local = empty_state();
        let mut remote = empty_state();
        local.letters.insert("s".into(), ls(1, 111, 50));
        remote.letters.insert("s".into(), ls(4, 999, 50)); // equal lastSeen
        let m = merge(&local, &remote, now);
        assert_eq!(*m.letters.get("s").unwrap(), ls(1, 111, 50)); // local kept
    }

    #[test]
    fn merge_unions_keys_and_takes_max_version() {
        let now = 7;
        let mut local = empty_state();
        let mut remote = empty_state();
        local.version = 5;
        remote.version = 9;
        local.letters.insert("s".into(), ls(1, 0, 10));
        remote.letters.insert("z".into(), ls(2, 0, 20));
        let m = merge(&local, &remote, now);
        assert_eq!(m.version, 9);
        assert_eq!(m.schema_version, 1);
        assert_eq!(*m.letters.get("s").unwrap(), ls(1, 0, 10));
        assert_eq!(*m.letters.get("z").unwrap(), ls(2, 0, 20));
        // ensure_letters ran → all 26 present.
        assert_eq!(m.letters.len(), 26);
        // A letter present on neither side defaulted to box 0 due=now.
        assert_eq!(*m.letters.get("q").unwrap(), ls(0, now, 0));
    }
}
