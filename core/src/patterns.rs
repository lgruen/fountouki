//! patterns — round generation for the two pattern game modes (`next` /
//! `unit`), transcribed from `docs/port-spec/patterns.md` §3–§6 and the
//! original `src/games/patterns/patterns.ts`.
//!
//! The RNG is consumed in a load-bearing order so that a seeded
//! [`Mulberry32`] reproduces the TS golden runs bit-for-bit:
//!
//!   1. (caller, mix only) `resolve_theme` — 1 call (see `themes.rs`).
//!   2. `chooseTemplate`                    — 1 call.
//!   3. shuffle the *whole* pool             — `pool.len() - 1` calls.
//!   4. partial-length threshold             — 1 call (always).
//!   5. partial-length pick (iff threshold ≥ 0.2) — 1 more call.
//!   6. (`next` mode) `buildChoices`:
//!        - hard: shuffle filtered pool (`filtered.len() - 1`) then
//!          shuffle the final list (`final.len() - 1`).
//!        - easy: shuffle the final list (`final.len() - 1`).
//!
//! [`generate_round`] performs steps 2–6. The caller does step 1 and passes
//! the *resolved* concrete theme in.

use nanoserde::{DeJson, DeJsonErr, DeJsonState, SerJson, SerJsonState};
use std::str::Chars;

use crate::rng::Mulberry32;
use crate::themes::{self, Item, ThemeChoice};

/// `TEMPLATES_BY_LEVEL[level-1]` — placeholder strings whose period is the
/// string length and whose distinct-count is the number of unique letters.
/// Copied verbatim from the spec §3 table; do not reorder (the per-tier pick is
/// `pickRng(tier, rng)`, so order is load-bearing for seeded reproduction).
pub const TEMPLATES_BY_LEVEL: [&[&str]; 6] = [
    // Level 1: periods 2,3; distinct 2.
    &["AB", "AAB", "ABB"],
    // Level 2: periods 2,3; distinct 2,3.
    &["AB", "AAB", "ABB", "ABC"],
    // Level 3: periods 3,4; distinct 2,3.
    &["AAB", "ABB", "ABC", "AABB"],
    // Level 4: periods 3,4; distinct 2,3.
    &["ABC", "AABB", "AABC", "ABBC", "ABCB"],
    // Level 5: periods 2,3,4; distinct 2,3,4.
    &["AB", "AAB", "ABC", "AABB", "AABC", "ABBC", "ABCB", "ABCD"],
    // Level 6: periods 4,5; distinct 4,5.
    &["ABCD", "AABCD", "ABCBD", "ABCDE"],
];

/// Game level cap (1-based). Templates above level 6 reuse the level-6 tier.
pub const MAX_LEVEL: u32 = 6;

/// Difficulty knob. `Auto` derives easy/hard from the level (≥4 → hard).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Auto,
    Easy,
    Hard,
}

impl Difficulty {
    pub fn as_str(self) -> &'static str {
        match self {
            Difficulty::Auto => "auto",
            Difficulty::Easy => "easy",
            Difficulty::Hard => "hard",
        }
    }

    pub fn from_str(s: &str) -> Option<Difficulty> {
        Some(match s {
            "auto" => Difficulty::Auto,
            "easy" => Difficulty::Easy,
            "hard" => Difficulty::Hard,
            _ => return None,
        })
    }
}

impl SerJson for Difficulty {
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        self.as_str().ser_json(d, s);
    }
}

impl DeJson for Difficulty {
    fn de_json(s: &mut DeJsonState, i: &mut Chars) -> Result<Self, DeJsonErr> {
        let raw = String::de_json(s, i)?;
        Difficulty::from_str(&raw).ok_or_else(|| s.err_parse("Difficulty"))
    }
}

/// The two game modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMode {
    /// "What comes next?" — fill the `?` slot from multiple choices.
    Next,
    /// "Find the repeating piece" — select a `period`-length contiguous run.
    Unit,
}

impl GameMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GameMode::Next => "next",
            GameMode::Unit => "unit",
        }
    }

    pub fn from_str(s: &str) -> Option<GameMode> {
        Some(match s {
            "next" => GameMode::Next,
            "unit" => GameMode::Unit,
            _ => return None,
        })
    }
}

impl SerJson for GameMode {
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        self.as_str().ser_json(d, s);
    }
}

impl DeJson for GameMode {
    fn de_json(s: &mut DeJsonState, i: &mut Chars) -> Result<Self, DeJsonErr> {
        let raw = String::de_json(s, i)?;
        GameMode::from_str(&raw).ok_or_else(|| s.err_parse("GameMode"))
    }
}

/// The effective easy/hard answer mode after resolving `Auto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerMode {
    Easy,
    Hard,
}

/// `effectiveAnswerMode`: map difficulty + level → easy/hard (spec §6).
pub fn effective_answer_mode(difficulty: Difficulty, level: u32) -> AnswerMode {
    match difficulty {
        Difficulty::Easy => AnswerMode::Easy,
        Difficulty::Hard => AnswerMode::Hard,
        Difficulty::Auto => {
            if level >= 4 {
                AnswerMode::Hard
            } else {
                AnswerMode::Easy
            }
        }
    }
}

/// `letterIndex(ch)` = `ch as u32 - 'A' as u32` (A→0, B→1, …, E→4).
fn letter_index(ch: char) -> usize {
    (ch as u32 - 'A' as u32) as usize
}

/// `distinctCount(template)` = number of unique chars.
fn distinct_count(template: &str) -> usize {
    let mut seen = [false; 26];
    let mut n = 0;
    for ch in template.chars() {
        let idx = letter_index(ch);
        if idx < 26 && !seen[idx] {
            seen[idx] = true;
            n += 1;
        }
    }
    n
}

/// `chooseTemplate(level, rng)` — clamp to the last tier for level > 6, then
/// `pickRng(tier, rng)`. Consumes exactly **1** RNG call.
fn choose_template(level: u32, rng: &mut Mulberry32) -> &'static str {
    // idx = min(level-1, len-1); guard level==0 (treated as level 1).
    let zero_based = level.saturating_sub(1) as usize;
    let idx = zero_based.min(TEMPLATES_BY_LEVEL.len() - 1);
    let tier = TEMPLATES_BY_LEVEL[idx];
    // pickRng: floor(rng()*len), clamped. below() == floor for the [0,1) stream.
    tier[rng.below(tier.len())]
}

/// A generated pattern round. Carries everything both UIs need.
///
/// - `next` mode shows `visible` followed by a `?` slot; the player chooses the
///   item that fills `slot_index` (== `visible.len()`), comparing by id to
///   `answer`. `choices` is pre-built.
/// - `unit` mode shows `visible` with no slot; the player selects any
///   contiguous run of `unit_len` (== `period`) cells. `answer` / `choices` are
///   still populated (harmless; the UI ignores them in unit mode).
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    /// Placeholder string, e.g. "AAB".
    pub template: String,
    /// Distinct items, index 0 == placeholder 'A', 1 == 'B', … (length =
    /// distinct count of the template).
    pub unit_items: Vec<Item>,
    /// The full visible sequence (no `?` slot). `len = full_reps*period + partial_len`.
    pub visible: Vec<Item>,
    /// The correct next item (the one that fills the `?` slot in `next` mode).
    pub answer: Item,
    /// Number of full repetitions visible at the start.
    pub full_reps: usize,
    /// Tail length beyond the last full rep (0..period-1).
    pub partial_len: usize,
    /// The pattern period (== template length == `unit_len`).
    pub period: usize,
    /// `next` mode: the index of the `?` slot (== `visible.len()`).
    pub slot_index: usize,
    /// `unit` mode: the required selection length (== `period`).
    pub unit_len: usize,
    /// Pre-built multiple-choice items for `next` mode (correct + distractors,
    /// shuffled). Empty-safe for `unit` mode callers that ignore it.
    pub choices: Vec<Item>,
}

/// Generate one round. Consumes the RNG in the spec's exact order (steps 2–6;
/// the caller must have already resolved `Mix` via
/// [`themes::resolve_theme`], which is step 1). `theme` must be a *concrete*
/// theme — passing `Mix` panics.
///
/// `level` is 1-based (game caps at 6; level > 6 reuses the level-6 tier).
pub fn generate_round(
    level: u32,
    theme: ThemeChoice,
    mode: GameMode,
    difficulty: Difficulty,
    rng: &mut Mulberry32,
) -> Round {
    assert!(
        theme != ThemeChoice::Mix,
        "generate_round needs a concrete theme; resolve Mix first"
    );
    let pool = themes::items(theme);

    // 2. template (1 rng call).
    let template = choose_template(level, rng);

    // 3. needed = distinct count.
    let needed = distinct_count(template);
    assert!(
        pool.len() >= needed,
        "theme pool has {} items but template '{}' needs {}",
        pool.len(),
        template,
        needed
    );

    // 4. shuffle the WHOLE pool (pool.len()-1 calls), then take the first
    //    `needed` as the unit items.
    let mut shuffled = pool.clone();
    rng.shuffle(&mut shuffled);
    let unit_items: Vec<Item> = shuffled.into_iter().take(needed).collect();

    // 5. period + 6. full reps.
    let period = template.chars().count();
    let full_reps = if period == 2 { 3 } else { 2 };

    // 7. partial length. period >= 2 always, so tail_max >= 1 and the JS
    //    short-circuit never skips the threshold rng() call: always 1 call for
    //    the < 0.2 test; a 2nd call only when that test fails.
    let tail_max = period - 1;
    let threshold = rng.next_f64(); // always consumed
    let partial_len = if tail_max == 0 || threshold < 0.2 {
        0
    } else {
        // 1 + floor(rng() * tail_max)
        1 + rng.below(tail_max)
    };

    // 8. build the visible sequence.
    let template_chars: Vec<char> = template.chars().collect();
    let mut visible: Vec<Item> = Vec::with_capacity(full_reps * period + partial_len);
    for _ in 0..full_reps {
        for &ch in &template_chars {
            visible.push(unit_items[letter_index(ch)].clone());
        }
    }
    for i in 0..partial_len {
        let ch = template_chars[i];
        visible.push(unit_items[letter_index(ch)].clone());
    }

    // 9. answer = next item in the infinite repetition.
    let next_ch = template_chars[visible.len() % period];
    let answer = unit_items[letter_index(next_ch)].clone();

    let slot_index = visible.len();

    // 6 (choices) — `next` mode only. Build using the effective answer mode.
    let choices = if mode == GameMode::Next {
        build_choices(&unit_items, &answer, effective_answer_mode(difficulty, level), &pool, rng)
    } else {
        Vec::new()
    };

    Round {
        template: template.to_string(),
        unit_items,
        visible,
        answer,
        full_reps,
        partial_len,
        period,
        slot_index,
        unit_len: period,
        choices,
    }
}

/// `buildChoices` (spec §6) — `next` mode only.
///
/// - easy: `shuffle([correct, ...fromUnit])` — exactly the distinct unit items.
/// - hard: pad with theme-pool distractors not in the unit to `max(4, distinct)`
///   choices, then shuffle. RNG order: shuffle filtered pool first, then shuffle
///   the final list.
fn build_choices(
    unit_items: &[Item],
    answer: &Item,
    mode: AnswerMode,
    pool: &[Item],
    rng: &mut Mulberry32,
) -> Vec<Item> {
    let answer_id = answer.id();
    // fromUnit = unit items minus the answer (preserving unit order).
    let from_unit: Vec<Item> = unit_items
        .iter()
        .filter(|it| it.id() != answer_id)
        .cloned()
        .collect();

    match mode {
        AnswerMode::Easy => {
            let mut list: Vec<Item> = Vec::with_capacity(1 + from_unit.len());
            list.push(answer.clone());
            list.extend(from_unit);
            rng.shuffle(&mut list);
            list
        }
        AnswerMode::Hard => {
            let target_count = 4.max(unit_items.len());
            // needed = targetCount - 1 - fromUnit.len(); may be 0 or negative.
            let needed = (target_count as isize) - 1 - (from_unit.len() as isize);
            let needed = needed.max(0) as usize;

            // extras = shuffle(pool not in unit)[0..needed].
            let mut filtered: Vec<Item> = pool
                .iter()
                .filter(|it| !unit_items.iter().any(|u| u.id() == it.id()))
                .cloned()
                .collect();
            rng.shuffle(&mut filtered);
            let extras: Vec<Item> = filtered.into_iter().take(needed).collect();

            let mut list: Vec<Item> = Vec::with_capacity(1 + from_unit.len() + extras.len());
            list.push(answer.clone());
            list.extend(from_unit);
            list.extend(extras);
            rng.shuffle(&mut list);
            list
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::ThemeChoice;
    use nanoserde::{DeJson, SerJson};

    #[test]
    fn difficulty_strings() {
        assert_eq!(Difficulty::Auto.as_str(), "auto");
        assert_eq!(Difficulty::Easy.as_str(), "easy");
        assert_eq!(Difficulty::Hard.as_str(), "hard");
        assert_eq!(Difficulty::from_str("hard"), Some(Difficulty::Hard));
        assert!(Difficulty::from_str("x").is_none());
        assert_eq!(Difficulty::Hard.serialize_json(), "\"hard\"");
        let d: Difficulty = DeJson::deserialize_json("\"auto\"").unwrap();
        assert_eq!(d, Difficulty::Auto);
    }

    #[test]
    fn game_mode_strings() {
        assert_eq!(GameMode::Next.as_str(), "next");
        assert_eq!(GameMode::Unit.as_str(), "unit");
        assert_eq!(GameMode::from_str("unit"), Some(GameMode::Unit));
        assert!(GameMode::from_str("x").is_none());
        assert_eq!(GameMode::Unit.serialize_json(), "\"unit\"");
        let m: GameMode = DeJson::deserialize_json("\"next\"").unwrap();
        assert_eq!(m, GameMode::Next);
    }

    #[test]
    fn effective_answer_mode_rules() {
        assert_eq!(effective_answer_mode(Difficulty::Easy, 6), AnswerMode::Easy);
        assert_eq!(effective_answer_mode(Difficulty::Hard, 1), AnswerMode::Hard);
        for lvl in 1..=3 {
            assert_eq!(effective_answer_mode(Difficulty::Auto, lvl), AnswerMode::Easy);
        }
        for lvl in 4..=6 {
            assert_eq!(effective_answer_mode(Difficulty::Auto, lvl), AnswerMode::Hard);
        }
    }

    #[test]
    fn distinct_count_matches() {
        assert_eq!(distinct_count("AB"), 2);
        assert_eq!(distinct_count("AAB"), 2);
        assert_eq!(distinct_count("ABC"), 3);
        assert_eq!(distinct_count("AABB"), 2);
        assert_eq!(distinct_count("AABC"), 3);
        assert_eq!(distinct_count("ABCD"), 4);
        assert_eq!(distinct_count("AABCD"), 4);
        assert_eq!(distinct_count("ABCBD"), 4);
        assert_eq!(distinct_count("ABCDE"), 5);
    }

    #[test]
    fn templates_by_level_table_is_exact() {
        let expected: [&[&str]; 6] = [
            &["AB", "AAB", "ABB"],
            &["AB", "AAB", "ABB", "ABC"],
            &["AAB", "ABB", "ABC", "AABB"],
            &["ABC", "AABB", "AABC", "ABBC", "ABCB"],
            &["AB", "AAB", "ABC", "AABB", "AABC", "ABBC", "ABCB", "ABCD"],
            &["ABCD", "AABCD", "ABCBD", "ABCDE"],
        ];
        assert_eq!(TEMPLATES_BY_LEVEL, expected);
    }

    #[test]
    fn choose_template_clamps_above_level_6() {
        // Level 99 must draw from the level-6 tier only.
        let tier6 = TEMPLATES_BY_LEVEL[5];
        for seed in 0..50u32 {
            let mut rng = Mulberry32::new(seed);
            let t = choose_template(99, &mut rng);
            assert!(tier6.contains(&t), "{t} not in level-6 tier");
        }
    }

    #[test]
    fn choose_template_level_zero_uses_level_1_tier() {
        let tier1 = TEMPLATES_BY_LEVEL[0];
        let mut rng = Mulberry32::new(7);
        let t = choose_template(0, &mut rng);
        assert!(tier1.contains(&t));
    }

    // ---- core determinism / structural invariants ----

    #[test]
    fn same_seed_same_round() {
        for level in 1..=6 {
            for &mode in &[GameMode::Next, GameMode::Unit] {
                let mut a = Mulberry32::new(0xC0FFEE);
                let mut b = Mulberry32::new(0xC0FFEE);
                let ra = generate_round(level, ThemeChoice::Numbers, mode, Difficulty::Auto, &mut a);
                let rb = generate_round(level, ThemeChoice::Numbers, mode, Difficulty::Auto, &mut b);
                assert_eq!(ra, rb, "level {level} mode {:?}", mode);
            }
        }
    }

    #[test]
    fn different_theme_does_not_panic_and_uses_theme_pool() {
        for &theme in &[
            ThemeChoice::EmojiAnimals,
            ThemeChoice::EmojiFruit,
            ThemeChoice::EmojiVehicles,
            ThemeChoice::EmojiConstruction,
            ThemeChoice::EmojiDinosaurs,
            ThemeChoice::Shapes,
            ThemeChoice::LettersUpper,
            ThemeChoice::LettersLower,
            ThemeChoice::Numbers,
        ] {
            let mut rng = Mulberry32::new(1234);
            let r = generate_round(6, theme, GameMode::Next, Difficulty::Hard, &mut rng);
            let pool = themes::items(theme);
            for it in &r.unit_items {
                assert!(pool.iter().any(|p| p.id() == it.id()));
            }
        }
    }

    #[test]
    #[should_panic]
    fn mix_theme_panics() {
        let mut rng = Mulberry32::new(1);
        let _ = generate_round(1, ThemeChoice::Mix, GameMode::Next, Difficulty::Auto, &mut rng);
    }

    #[test]
    fn visible_length_formula_holds() {
        // full_reps = 3 for period 2, else 2; partial_len in 0..period-1.
        for level in 1..=6 {
            for seed in 0..200u32 {
                let mut rng = Mulberry32::new(seed);
                let r =
                    generate_round(level, ThemeChoice::EmojiAnimals, GameMode::Unit, Difficulty::Auto, &mut rng);
                let expected_full = if r.period == 2 { 3 } else { 2 };
                assert_eq!(r.full_reps, expected_full);
                assert!(r.partial_len <= r.period - 1);
                assert_eq!(r.visible.len(), r.full_reps * r.period + r.partial_len);
                assert_eq!(r.slot_index, r.visible.len());
                assert_eq!(r.unit_len, r.period);
            }
        }
    }

    #[test]
    fn period_range_scales_with_level() {
        // Collect the set of periods reachable at each level over many seeds.
        let periods_for = |level: u32| {
            let mut set = std::collections::BTreeSet::new();
            for seed in 0..2000u32 {
                let mut rng = Mulberry32::new(seed);
                let r =
                    generate_round(level, ThemeChoice::Numbers, GameMode::Unit, Difficulty::Auto, &mut rng);
                set.insert(r.period);
            }
            set
        };
        // Per spec §3: periods present per level.
        assert_eq!(periods_for(1), [2usize, 3].into_iter().collect());
        assert_eq!(periods_for(2), [2usize, 3].into_iter().collect());
        assert_eq!(periods_for(3), [3usize, 4].into_iter().collect());
        assert_eq!(periods_for(4), [3usize, 4].into_iter().collect());
        assert_eq!(periods_for(5), [2usize, 3, 4].into_iter().collect());
        assert_eq!(periods_for(6), [4usize, 5].into_iter().collect());
    }

    #[test]
    fn answer_is_the_correct_next_item() {
        for seed in 0..500u32 {
            for level in 1..=6 {
                let mut rng = Mulberry32::new(seed);
                let r =
                    generate_round(level, ThemeChoice::Numbers, GameMode::Next, Difficulty::Easy, &mut rng);
                // The answer must equal the item the infinite pattern dictates
                // at the slot position.
                let chars: Vec<char> = r.template.chars().collect();
                let next_ch = chars[r.visible.len() % r.period];
                let expected = &r.unit_items[(next_ch as u32 - 'A' as u32) as usize];
                assert_eq!(&r.answer, expected);
            }
        }
    }

    // ---- choice rules ----

    #[test]
    fn easy_choice_count_equals_distinct_count() {
        for seed in 0..500u32 {
            for level in 1..=6 {
                let mut rng = Mulberry32::new(seed);
                let r =
                    generate_round(level, ThemeChoice::EmojiAnimals, GameMode::Next, Difficulty::Easy, &mut rng);
                assert_eq!(r.choices.len(), r.unit_items.len());
                // Every choice is a distinct unit item; answer is present.
                assert!(r.choices.iter().any(|c| c.id() == r.answer.id()));
            }
        }
    }

    #[test]
    fn hard_choice_count_is_max4_distinct_when_pool_big() {
        // animals pool (28) always supplies enough distractors.
        for seed in 0..500u32 {
            for level in 1..=6 {
                let mut rng = Mulberry32::new(seed);
                let r =
                    generate_round(level, ThemeChoice::EmojiAnimals, GameMode::Next, Difficulty::Hard, &mut rng);
                let expected = 4.max(r.unit_items.len());
                assert_eq!(r.choices.len(), expected, "level {level} seed {seed}");
                assert!(r.choices.iter().any(|c| c.id() == r.answer.id()));
                // No duplicate ids among choices.
                let mut ids: Vec<&str> = r.choices.iter().map(|c| c.id()).collect();
                ids.sort();
                ids.dedup();
                assert_eq!(ids.len(), r.choices.len());
            }
        }
    }

    #[test]
    fn hard_choices_include_all_unit_mates() {
        // The correct + every unit-mate must always be present in hard mode.
        let mut rng = Mulberry32::new(99);
        let r = generate_round(6, ThemeChoice::Numbers, GameMode::Next, Difficulty::Hard, &mut rng);
        for u in &r.unit_items {
            assert!(r.choices.iter().any(|c| c.id() == u.id()));
        }
    }

    #[test]
    fn unit_mode_has_no_choices_and_valid_unit() {
        for seed in 0..200u32 {
            for level in 1..=6 {
                let mut rng = Mulberry32::new(seed);
                let r =
                    generate_round(level, ThemeChoice::Numbers, GameMode::Unit, Difficulty::Auto, &mut rng);
                assert!(r.choices.is_empty());
                // The visible sequence is a genuine repetition of unit_items by
                // the template: any window of length `unit_len` starting at a
                // valid offset is a rotation of the unit.
                assert_eq!(r.unit_len, r.period);
                assert!(r.visible.len() >= r.period);
                // Check the first full period matches template -> unit_items.
                let chars: Vec<char> = r.template.chars().collect();
                for (i, &ch) in chars.iter().enumerate() {
                    let expect = &r.unit_items[(ch as u32 - 'A' as u32) as usize];
                    assert_eq!(&r.visible[i], expect);
                }
                // The sequence is periodic with period `period`.
                for i in r.period..r.visible.len() {
                    assert_eq!(r.visible[i], r.visible[i - r.period]);
                }
            }
        }
    }

    #[test]
    fn unit_mode_any_period_window_is_valid_rotation() {
        // The §9 correctness criterion: any contiguous run of exactly `period`
        // cells (any offset) is a valid rotation of the unit. Verify each such
        // window contains exactly the distinct unit items.
        let mut rng = Mulberry32::new(2024);
        let r = generate_round(6, ThemeChoice::Numbers, GameMode::Unit, Difficulty::Auto, &mut rng);
        let last_start = r.visible.len() - r.period;
        for start in 0..=last_start {
            let window = &r.visible[start..start + r.period];
            let mut got: Vec<&str> = window.iter().map(|i| i.id()).collect();
            got.sort();
            got.dedup();
            let mut want: Vec<&str> = r.unit_items.iter().map(|i| i.id()).collect();
            want.sort();
            want.dedup();
            assert_eq!(got, want, "window at offset {start} is not a unit rotation");
        }
    }

    #[test]
    fn shapes_hard_mode_may_fall_short_of_4_without_panicking() {
        // shapes has 6 items; at distinct 5 (level-6 ABCDE) only 1 distractor
        // exists, so hard count can be < or == max(4,5)=5 depending on draw,
        // but it must never panic and never exceed the pool.
        for seed in 0..300u32 {
            let mut rng = Mulberry32::new(seed);
            let r = generate_round(6, ThemeChoice::Shapes, GameMode::Next, Difficulty::Hard, &mut rng);
            assert!(r.choices.len() <= 6);
            assert!(r.choices.iter().any(|c| c.id() == r.answer.id()));
        }
    }

    #[test]
    fn rng_consumption_order_matches_spec_for_easy_next() {
        // Reproduce generate_round's rng draws by hand for a fixed theme + easy
        // mode and assert the streams line up step-by-step. Numbers = 9 items.
        let theme = ThemeChoice::Numbers;
        let pool_len = themes::items(theme).len();
        let level = 1;

        let mut auto = Mulberry32::new(0xABCDEF);
        let r = generate_round(level, theme, GameMode::Next, Difficulty::Easy, &mut auto);

        // Now replay the consumption count manually on a fresh stream and make
        // sure the post-round state matches (i.e. total draws are identical).
        let mut manual = Mulberry32::new(0xABCDEF);
        // 1 call: choose_template.
        let _ = manual.below(TEMPLATES_BY_LEVEL[0].len());
        // pool_len-1 calls: full shuffle of the pool.
        let mut dummy: Vec<usize> = (0..pool_len).collect();
        manual.shuffle(&mut dummy);
        // 1 call: partial threshold (always consumed; period >= 2 => tail_max >= 1).
        let threshold = manual.next_f64();
        if threshold >= 0.2 {
            // 1 more call for the partial pick.
            let _ = manual.below(r.period - 1);
        }
        // easy buildChoices: shuffle the final list (distinct count items).
        let mut dummy2: Vec<usize> = (0..r.unit_items.len()).collect();
        manual.shuffle(&mut dummy2);

        // Both streams should now be at the same position.
        assert_eq!(auto.next_f64(), manual.next_f64());
    }
}
