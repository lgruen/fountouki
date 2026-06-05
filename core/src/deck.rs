//! Phonics deck data: the 26 lowercase letters, their exemplar word/emoji
//! cards, and the two orderings used by the game.
//!
//! - `LETTERS` — alphabetical `a..z` (= deck order). This is the order
//!   `srs::ensure_letters` walks when initializing missing state.
//! - `INTRO_ORDER` — Jolly-Phonics introduction order. Distinct from
//!   `LETTERS`; used ONLY by the active-set drip-in gate in `srs`.
//!
//! Each letter has a CANONICAL exemplar (always used for the miss-hint —
//! a clean anchor) plus optional VARIANTS unlocked at box >= 2 for
//! generalization. Emoji include variation selectors / ZWJ sequences
//! (☀️, ☂️, 🗝️, 6️⃣, 0️⃣) — stored as exact UTF-8, never normalized.
//!
//! Transcribed from `src/games/phonics/deck.ts`; values are load-bearing.

use crate::rng::Mulberry32;

/// One exemplar: an emoji glyph paired with its spoken word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exemplar {
    pub emoji: &'static str,
    pub word: &'static str,
}

/// A full deck entry: the letter, its canonical exemplar, and its variants.
#[derive(Clone, Copy, Debug)]
pub struct LetterCard {
    pub letter: char,
    pub canonical: Exemplar,
    /// Extra exemplars unlocked at box >= 2 (may be empty).
    pub variants: &'static [Exemplar],
}

/// Convenience constructor for an `Exemplar` literal.
const fn ex(emoji: &'static str, word: &'static str) -> Exemplar {
    Exemplar { emoji, word }
}

/// Deck order = alphabetical a..z. (Note: `LETTERS` != `INTRO_ORDER`.)
pub const LETTERS: [char; 26] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];

/// Jolly-Phonics introduction / drip-in order. Used ONLY by the active-set
/// gate (`srs::active_letters`).
///
/// group 1: s a t i p n
/// group 2: c k e h r m d
/// group 3: g o u l f b
/// tail:    j z w v y x q
pub const INTRO_ORDER: [char; 26] = [
    's', 'a', 't', 'i', 'p', 'n', // group 1
    'c', 'k', 'e', 'h', 'r', 'm', 'd', // group 2
    'g', 'o', 'u', 'l', 'f', 'b', // group 3
    'j', 'z', 'w', 'v', 'y', 'x', 'q', // tail
];

/// The full deck, in `LETTERS` (alphabetical) order. Canonical first, then
/// variants, per `deck.ts`.
pub const DECK: [LetterCard; 26] = [
    LetterCard {
        letter: 'a',
        canonical: ex("🐜", "ant"),
        variants: &[ex("🍎", "apple"), ex("🐊", "alligator")],
    },
    LetterCard {
        letter: 'b',
        canonical: ex("🐻", "bear"),
        variants: &[ex("🦋", "butterfly"), ex("🎈", "balloon")],
    },
    LetterCard {
        letter: 'c',
        canonical: ex("🐱", "cat"),
        variants: &[ex("🥕", "carrot"), ex("🐄", "cow")],
    },
    LetterCard {
        letter: 'd',
        canonical: ex("🐕", "dog"),
        variants: &[ex("🦆", "duck"), ex("🦖", "dinosaur")],
    },
    LetterCard {
        letter: 'e',
        canonical: ex("🐘", "elephant"),
        variants: &[ex("🥚", "egg")],
    },
    LetterCard {
        letter: 'f',
        canonical: ex("🐟", "fish"),
        variants: &[ex("🐸", "frog"), ex("🌸", "flower")],
    },
    LetterCard {
        letter: 'g',
        canonical: ex("🐐", "goat"),
        variants: &[ex("🍇", "grapes"), ex("🎁", "gift")],
    },
    LetterCard {
        letter: 'h',
        canonical: ex("🐴", "horse"),
        variants: &[ex("🏠", "house"), ex("🎩", "hat")],
    },
    LetterCard {
        letter: 'i',
        canonical: ex("🦎", "iguana"),
        variants: &[ex("🪻", "iris")],
    },
    LetterCard {
        letter: 'j',
        canonical: ex("🪼", "jellyfish"),
        variants: &[ex("🎷", "jazz"), ex("🃏", "joker")],
    },
    LetterCard {
        letter: 'k',
        canonical: ex("🦘", "kangaroo"),
        variants: &[ex("🗝️", "key"), ex("🪁", "kite")],
    },
    LetterCard {
        letter: 'l',
        canonical: ex("🦁", "lion"),
        variants: &[ex("🍋", "lemon"), ex("🐞", "ladybug")],
    },
    LetterCard {
        letter: 'm',
        canonical: ex("🐵", "monkey"),
        variants: &[ex("🌙", "moon"), ex("🍄", "mushroom")],
    },
    LetterCard {
        letter: 'n',
        canonical: ex("🪺", "nest"),
        variants: &[ex("👃", "nose"), ex("🥜", "nut")],
    },
    LetterCard {
        letter: 'o',
        canonical: ex("🐙", "octopus"),
        variants: &[ex("🦉", "owl"), ex("🍊", "orange")],
    },
    LetterCard {
        letter: 'p',
        canonical: ex("🐼", "panda"),
        variants: &[ex("🍍", "pineapple"), ex("🐧", "penguin")],
    },
    LetterCard {
        letter: 'q',
        canonical: ex("👸", "queen"),
        variants: &[ex("🪶", "quill"), ex("❓", "question")],
    },
    LetterCard {
        letter: 'r',
        canonical: ex("🌈", "rainbow"),
        variants: &[ex("🐰", "rabbit"), ex("🤖", "robot")],
    },
    LetterCard {
        letter: 's',
        canonical: ex("☀️", "sun"),
        variants: &[ex("🐍", "snake"), ex("⭐", "star")],
    },
    LetterCard {
        letter: 't',
        canonical: ex("🐢", "turtle"),
        variants: &[ex("🐅", "tiger"), ex("🌳", "tree")],
    },
    LetterCard {
        letter: 'u',
        canonical: ex("☂️", "umbrella"),
        variants: &[ex("🆙", "up")],
    },
    LetterCard {
        letter: 'v',
        canonical: ex("🚐", "van"),
        variants: &[ex("🎻", "violin"), ex("🌋", "volcano")],
    },
    LetterCard {
        letter: 'w',
        canonical: ex("🐳", "whale"),
        variants: &[ex("🌊", "wave"), ex("🍉", "watermelon")],
    },
    LetterCard {
        letter: 'x',
        canonical: ex("🩻", "x-ray"),
        variants: &[ex("📦", "box"), ex("6️⃣", "six")],
    },
    LetterCard {
        letter: 'y',
        canonical: ex("🪀", "yo-yo"),
        variants: &[ex("🟡", "yellow")],
    },
    LetterCard {
        letter: 'z',
        canonical: ex("🦓", "zebra"),
        variants: &[ex("0️⃣", "zero"), ex("💤", "zzz")],
    },
];

/// Look up a deck card by letter. `None` for any non-deck char.
pub fn card(letter: char) -> Option<&'static LetterCard> {
    DECK.iter().find(|c| c.letter == letter)
}

/// The canonical exemplar for `letter` (the clean anchor used by the
/// miss-hint). `None` for an unknown letter.
pub fn exemplar(letter: char) -> Option<Exemplar> {
    card(letter).map(|c| c.canonical)
}

/// Pick an exemplar for display.
///
/// - box < 2 OR no variants → always the canonical (clean anchor).
/// - box >= 2 with variants → uniform over `[canonical] ++ variants`
///   (canonical included), index = `floor(rng() * pool.len())`.
///
/// The miss-hint always calls this with `box == 0` → canonical. Panics on
/// an unknown letter (mirrors the TS `lookup` throwing).
pub fn pick_exemplar(letter: char, box_: u8, rng: &mut Mulberry32) -> Exemplar {
    let c = card(letter).expect("pick_exemplar: unknown letter");
    if box_ < 2 || c.variants.is_empty() {
        return c.canonical;
    }
    // pool = [canonical] ++ variants; uniform pick, index = floor(rng()*len).
    let pool_len = 1 + c.variants.len();
    let idx = rng.below(pool_len);
    if idx == 0 {
        c.canonical
    } else {
        c.variants[idx - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_is_alphabetical() {
        assert_eq!(LETTERS.len(), 26);
        assert_eq!(LETTERS[0], 'a');
        assert_eq!(LETTERS[25], 'z');
        for w in LETTERS.windows(2) {
            assert!(w[0] < w[1], "LETTERS must be ascending");
        }
    }

    #[test]
    fn intro_order_is_jolly_and_a_permutation() {
        assert_eq!(INTRO_ORDER.len(), 26);
        let expected: [char; 26] = [
            's', 'a', 't', 'i', 'p', 'n', 'c', 'k', 'e', 'h', 'r', 'm', 'd', 'g', 'o', 'u', 'l',
            'f', 'b', 'j', 'z', 'w', 'v', 'y', 'x', 'q',
        ];
        assert_eq!(INTRO_ORDER, expected);
        // It is a permutation of LETTERS.
        let mut sorted = INTRO_ORDER;
        sorted.sort_unstable();
        assert_eq!(sorted, LETTERS);
    }

    #[test]
    fn deck_covers_every_letter_in_alpha_order() {
        assert_eq!(DECK.len(), 26);
        for (i, c) in DECK.iter().enumerate() {
            assert_eq!(c.letter, LETTERS[i]);
        }
    }

    #[test]
    fn exemplars_match_spec_samples() {
        assert_eq!(exemplar('a'), Some(ex("🐜", "ant")));
        assert_eq!(exemplar('s'), Some(ex("☀️", "sun")));
        assert_eq!(exemplar('r'), Some(ex("🌈", "rainbow")));
        assert_eq!(exemplar('z'), Some(ex("🦓", "zebra")));
        assert_eq!(exemplar('x'), Some(ex("🩻", "x-ray")));
        assert_eq!(exemplar('?'), None);
    }

    #[test]
    fn variants_preserved_with_selectors() {
        let s = card('s').unwrap();
        assert_eq!(s.variants, &[ex("🐍", "snake"), ex("⭐", "star")]);
        // The 'u' canonical retains its variation selector (☂️).
        assert_eq!(card('u').unwrap().canonical.emoji, "☂️");
        // x has a keycap six variant (6️⃣).
        assert_eq!(card('x').unwrap().variants[1], ex("6️⃣", "six"));
    }

    #[test]
    fn pick_exemplar_below_box2_is_canonical() {
        let mut rng = Mulberry32::new(1);
        for box_ in 0..2u8 {
            assert_eq!(pick_exemplar('s', box_, &mut rng), ex("☀️", "sun"));
        }
    }

    #[test]
    fn pick_exemplar_high_box_chooses_within_pool() {
        let mut rng = Mulberry32::new(7);
        let chosen = pick_exemplar('e', 3, &mut rng);
        let pool = [ex("🐘", "elephant"), ex("🥚", "egg")];
        assert!(pool.contains(&chosen));
    }

    #[test]
    fn pick_exemplar_box2_can_include_variant() {
        // Over many draws on a letter with 2 variants we should see more than
        // just the canonical.
        let mut rng = Mulberry32::new(42);
        let mut seen_variant = false;
        let mut seen_canonical = false;
        for _ in 0..50 {
            let e = pick_exemplar('a', 2, &mut rng);
            if e == ex("🐜", "ant") {
                seen_canonical = true;
            } else {
                seen_variant = true;
            }
        }
        assert!(seen_canonical, "canonical should appear in the pool");
        assert!(seen_variant, "variants should appear at box>=2");
    }
}
