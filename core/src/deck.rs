//! Phonics deck data: the 26 lowercase letters, each paired with a single
//! canonical exemplar (the picture shown on a miss), plus the two letter
//! orderings the game uses. One picture per letter — consistency aids learning
//! for small working memory (no per-letter variety to track).
//!
//! - `LETTERS` — alphabetical `a..z` (= deck order). This is the order
//!   `srs::ensure_letters` walks when initializing missing state.
//! - `INTRO_ORDER` — Jolly-Phonics introduction order. Distinct from
//!   `LETTERS`; used ONLY by the active-set drip-in gate in `srs`.
//!
//! Emoji are stored as exact UTF-8 — variation selectors / ZWJ sequences (☀️,
//! ☂️, 🗝️ …) are kept verbatim, never normalized. 'igloo' has no Unicode glyph
//! and is drawn as a vector by the app (its emoji string is a sentinel).
//!
//! Transcribed from `src/games/phonics/deck.ts`; values are load-bearing.

/// One exemplar: an emoji glyph paired with its spoken word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exemplar {
    pub emoji: &'static str,
    pub word: &'static str,
}

/// A full deck entry: the letter and its canonical exemplar.
#[derive(Clone, Copy, Debug)]
pub struct LetterCard {
    pub letter: char,
    pub canonical: Exemplar,
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

/// The full deck, in `LETTERS` (alphabetical) order — one exemplar per letter.
/// Every consonant uses its hard/primary sound (cat/goat, not city/giraffe) and
/// every vowel its short sound (ant, egg, igloo, octopus, umbrella). x uses a
/// word-final /ks/ (fox), the only position the x sound occurs.
pub const DECK: [LetterCard; 26] = [
    LetterCard { letter: 'a', canonical: ex("🐜", "ant") },
    LetterCard { letter: 'b', canonical: ex("🐻", "bear") },
    LetterCard { letter: 'c', canonical: ex("🐱", "cat") },
    LetterCard { letter: 'd', canonical: ex("🐕", "dog") },
    LetterCard { letter: 'e', canonical: ex("🐘", "elephant") },
    LetterCard { letter: 'f', canonical: ex("🐟", "fish") },
    LetterCard { letter: 'g', canonical: ex("🐐", "goat") },
    LetterCard { letter: 'h', canonical: ex("🐴", "horse") },
    // 'igloo' has no Unicode/Twemoji glyph — the app draws it as a vector (keyed
    // off the word); 🛖 ('hut') is a never-rendered sentinel codepoint.
    LetterCard { letter: 'i', canonical: ex("🛖", "igloo") },
    LetterCard { letter: 'j', canonical: ex("🪼", "jellyfish") },
    LetterCard { letter: 'k', canonical: ex("🦘", "kangaroo") },
    LetterCard { letter: 'l', canonical: ex("🦁", "lion") },
    LetterCard { letter: 'm', canonical: ex("🐵", "monkey") },
    LetterCard { letter: 'n', canonical: ex("🪺", "nest") },
    LetterCard { letter: 'o', canonical: ex("🐙", "octopus") },
    LetterCard { letter: 'p', canonical: ex("🐼", "panda") },
    LetterCard { letter: 'q', canonical: ex("👸", "queen") },
    LetterCard { letter: 'r', canonical: ex("🌈", "rainbow") },
    LetterCard { letter: 's', canonical: ex("☀️", "sun") },
    LetterCard { letter: 't', canonical: ex("🐢", "turtle") },
    LetterCard { letter: 'u', canonical: ex("☂️", "umbrella") },
    LetterCard { letter: 'v', canonical: ex("🚐", "van") },
    LetterCard { letter: 'w', canonical: ex("🐳", "whale") },
    // x's sound /ks/ only occurs word-finally, so the picture word ends in x;
    // 'x-ray' would teach the letter *name* (/ɛks/), not the sound.
    LetterCard { letter: 'x', canonical: ex("🦊", "fox") },
    LetterCard { letter: 'y', canonical: ex("🪀", "yo-yo") },
    LetterCard { letter: 'z', canonical: ex("🦓", "zebra") },
];

/// Look up a deck card by letter. `None` for any non-deck char.
pub fn card(letter: char) -> Option<&'static LetterCard> {
    DECK.iter().find(|c| c.letter == letter)
}

/// The canonical exemplar for `letter` (the picture shown on a miss-hint).
/// `None` for an unknown letter.
pub fn exemplar(letter: char) -> Option<Exemplar> {
    card(letter).map(|c| c.canonical)
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
        assert_eq!(exemplar('x'), Some(ex("🦊", "fox")));
        assert_eq!(exemplar('?'), None);
    }

    #[test]
    fn canonical_emoji_keep_variation_selectors() {
        // The sun + umbrella canonicals carry a U+FE0F variation selector that
        // must survive verbatim (no normalization) to match the sprite keys.
        assert_eq!(card('s').unwrap().canonical.emoji, "☀️");
        assert_eq!(card('u').unwrap().canonical.emoji, "☂️");
    }
}
