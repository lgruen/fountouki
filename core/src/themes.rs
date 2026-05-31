//! themes — the 9 pattern themes' data, transcribed from
//! `docs/port-spec/patterns.md` §7 and the original `src/games/patterns/themes.ts`.
//!
//! Load-bearing details:
//! - The serialized `ThemeChoice` strings ("mix", "emoji-animals", …) are used
//!   for save-compat + cross-device sync interop with the existing TS clients;
//!   keep them byte-identical.
//! - `ALL_THEME_IDS` insertion order is load-bearing: the "mix" theme picker
//!   indexes into it via the RNG (see §6 / §14), so reordering would change
//!   which theme a seeded run picks.
//! - The letter-set quirk (upper = 18 glyphs, lower = 17 — lower also omits
//!   `l`) and the vehicle/construction emoji (incl. the U+FE0F variation
//!   selectors on ✈️ / 🏗️) are reproduced exactly.

use nanoserde::{DeJson, DeJsonErr, DeJsonState, SerJson, SerJsonState};
use std::str::Chars;

use crate::rng::Mulberry32;

/// A drawable shape (the `shapes` theme). Colors / radius / clip mirror the CSS
/// `--shape-color` / `--shape-radius` / `--shape-clip` custom properties the TS
/// app set per item.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// CSS color as a packed `0xRRGGBB` value (the TS stored a hex string).
    pub color: u32,
    /// CSS border-radius. `None` = sharp corners (the triangles).
    pub radius: Option<&'static str>,
    /// CSS clip-path polygon. `None` = no clip (circles / squares).
    pub clip: Option<&'static str>,
}

/// A single pattern item. Equality + the answer key are by *identity*: for
/// glyphs the glyph string is the stable id, for shapes the `id` field is.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// An emoji / letter / number glyph (the literal char(s) to render).
    Glyph(&'static str),
    /// A drawn shape with a stable id (e.g. "red-circle").
    Shape { id: &'static str, shape: Shape },
}

impl Item {
    /// Stable id used for equality + as the answer key (matches the TS `item.id`).
    /// For glyphs the id equals the glyph; for shapes it's the explicit id.
    pub fn id(&self) -> &'static str {
        match self {
            Item::Glyph(g) => g,
            Item::Shape { id, .. } => id,
        }
    }
}

/// The nine theme choices. `Mix` is not a concrete theme — it picks one of the
/// nine concrete themes at round time via the RNG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    Mix,
    EmojiAnimals,
    EmojiFruit,
    EmojiVehicles,
    EmojiConstruction,
    EmojiDinosaurs,
    Shapes,
    LettersUpper,
    LettersLower,
    Numbers,
}

impl ThemeChoice {
    /// Exact serialized string (load-bearing for storage / sync interop).
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeChoice::Mix => "mix",
            ThemeChoice::EmojiAnimals => "emoji-animals",
            ThemeChoice::EmojiFruit => "emoji-fruit",
            ThemeChoice::EmojiVehicles => "emoji-vehicles",
            ThemeChoice::EmojiConstruction => "emoji-construction",
            ThemeChoice::EmojiDinosaurs => "emoji-dinosaurs",
            ThemeChoice::Shapes => "shapes",
            ThemeChoice::LettersUpper => "letters-upper",
            ThemeChoice::LettersLower => "letters-lower",
            ThemeChoice::Numbers => "numbers",
        }
    }

    /// Parse from the serialized string. `None` on unknown input.
    pub fn from_str(s: &str) -> Option<ThemeChoice> {
        Some(match s {
            "mix" => ThemeChoice::Mix,
            "emoji-animals" => ThemeChoice::EmojiAnimals,
            "emoji-fruit" => ThemeChoice::EmojiFruit,
            "emoji-vehicles" => ThemeChoice::EmojiVehicles,
            "emoji-construction" => ThemeChoice::EmojiConstruction,
            "emoji-dinosaurs" => ThemeChoice::EmojiDinosaurs,
            "shapes" => ThemeChoice::Shapes,
            "letters-upper" => ThemeChoice::LettersUpper,
            "letters-lower" => ThemeChoice::LettersLower,
            "numbers" => ThemeChoice::Numbers,
            _ => return None,
        })
    }
}

impl SerJson for ThemeChoice {
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        // Serialize as a plain JSON string of the exact id.
        self.as_str().ser_json(d, s);
    }
}

impl DeJson for ThemeChoice {
    fn de_json(s: &mut DeJsonState, i: &mut Chars) -> Result<Self, DeJsonErr> {
        let raw = String::de_json(s, i)?;
        ThemeChoice::from_str(&raw).ok_or_else(|| s.err_parse("ThemeChoice"))
    }
}

/// The nine concrete theme ids, in the **insertion order** the TS `THEMES` map
/// used. The `mix` picker indexes into this with `floor(rng() * len)`, so the
/// order is load-bearing for seeded reproduction.
pub const ALL_THEME_IDS: [ThemeChoice; 9] = [
    ThemeChoice::EmojiAnimals,
    ThemeChoice::EmojiFruit,
    ThemeChoice::EmojiVehicles,
    ThemeChoice::EmojiConstruction,
    ThemeChoice::EmojiDinosaurs,
    ThemeChoice::Shapes,
    ThemeChoice::LettersUpper,
    ThemeChoice::LettersLower,
    ThemeChoice::Numbers,
];

// --- glyph pools (pool order matters only as input to the shuffle) ---------

const ANIMALS: &[&str] = &[
    "🐶", "🐱", "🐰", "🐻", "🐼", "🐯", "🐸", "🐵", "🦁", "🦊", "🐮", "🐷", "🐭", "🐹",
    "🐨", "🐘", "🦒", "🦓", "🐴", "🦄", "🐧", "🐤", "🦉", "🐳", "🐙", "🐠", "🐝", "🦋",
];

const FRUIT: &[&str] = &["🍎", "🍌", "🍇", "🍓", "🍊", "🥝", "🍐", "🍉"];

// plane "✈️" includes U+FE0F; boat is "⛵".
const VEHICLES: &[&str] = &["🚗", "🚌", "🚂", "✈️", "🚀", "🚲", "⛵", "🚜"];

// crane "🏗️" includes U+FE0F; digger reuses 🚜 (same glyph as the vehicles
// tractor but a distinct item).
const CONSTRUCTION: &[&str] = &["🏗️", "🚛", "🚜", "🚧", "🔨", "🔧", "🪚", "🧰"];

const DINOSAURS: &[&str] = &["🦖", "🦕", "🐊", "🐢", "🦎", "🐉", "🥚", "🦴"];

// letters-upper: 18 glyphs. Omits I, O, Q, U, V, W, X, Z.
const LETTERS_UPPER: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "J", "K", "L", "M", "N", "P", "R", "S", "T", "Y",
];

// letters-lower: 17 glyphs. Omits i, l, o, q, u, v, w, x, z (note: also omits
// `l`, unlike upper which keeps `L`).
const LETTERS_LOWER: &[&str] = &[
    "a", "b", "c", "d", "e", "f", "g", "h", "j", "k", "m", "n", "p", "r", "s", "t", "y",
];

const NUMBERS: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// The six `shapes` items, in pool order. Colors are packed `0xRRGGBB`.
fn shapes_pool() -> Vec<Item> {
    vec![
        Item::Shape {
            id: "red-circle",
            shape: Shape { color: 0xef476f, radius: Some("50%"), clip: None },
        },
        Item::Shape {
            id: "blue-square",
            shape: Shape { color: 0x118ab2, radius: Some("6px"), clip: None },
        },
        Item::Shape {
            id: "yellow-triangle",
            shape: Shape {
                color: 0xffd166,
                radius: None,
                clip: Some("polygon(50% 0, 100% 100%, 0 100%)"),
            },
        },
        Item::Shape {
            id: "green-circle",
            shape: Shape { color: 0x06d6a0, radius: Some("50%"), clip: None },
        },
        Item::Shape {
            id: "purple-square",
            shape: Shape { color: 0x9b5de5, radius: Some("6px"), clip: None },
        },
        Item::Shape {
            id: "orange-triangle",
            shape: Shape {
                color: 0xff8c42,
                radius: None,
                clip: Some("polygon(50% 0, 100% 100%, 0 100%)"),
            },
        },
    ]
}

fn glyph_pool(glyphs: &[&'static str]) -> Vec<Item> {
    glyphs.iter().map(|g| Item::Glyph(g)).collect()
}

/// The item pool for a **concrete** theme. Panics for `Mix` (resolve it via
/// [`resolve_theme`] first).
pub fn items(theme: ThemeChoice) -> Vec<Item> {
    match theme {
        ThemeChoice::Mix => {
            panic!("themes::items called with Mix; resolve a concrete theme first")
        }
        ThemeChoice::EmojiAnimals => glyph_pool(ANIMALS),
        ThemeChoice::EmojiFruit => glyph_pool(FRUIT),
        ThemeChoice::EmojiVehicles => glyph_pool(VEHICLES),
        ThemeChoice::EmojiConstruction => glyph_pool(CONSTRUCTION),
        ThemeChoice::EmojiDinosaurs => glyph_pool(DINOSAURS),
        ThemeChoice::Shapes => shapes_pool(),
        ThemeChoice::LettersUpper => glyph_pool(LETTERS_UPPER),
        ThemeChoice::LettersLower => glyph_pool(LETTERS_LOWER),
        ThemeChoice::Numbers => glyph_pool(NUMBERS),
    }
}

/// The human-readable label for a theme (matches the TS `Theme.label`).
pub fn label(theme: ThemeChoice) -> &'static str {
    match theme {
        ThemeChoice::Mix => "Mix",
        ThemeChoice::EmojiAnimals => "Animals",
        ThemeChoice::EmojiFruit => "Fruit",
        ThemeChoice::EmojiVehicles => "Vehicles",
        ThemeChoice::EmojiConstruction => "Construction",
        ThemeChoice::EmojiDinosaurs => "Dinosaurs",
        ThemeChoice::Shapes => "Shapes",
        ThemeChoice::LettersUpper => "Letters (ABC)",
        ThemeChoice::LettersLower => "letters (abc)",
        ThemeChoice::Numbers => "Numbers",
    }
}

/// Resolve a [`ThemeChoice`] to a concrete theme id. For `Mix` this consumes
/// **exactly one** RNG call (`floor(rng() * ALL_THEME_IDS.len())`) — matching
/// the TS `pickTheme` — and indexes into [`ALL_THEME_IDS`]. For any fixed theme
/// it consumes **zero** RNG calls and returns the choice unchanged.
pub fn resolve_theme(choice: ThemeChoice, rng: &mut Mulberry32) -> ThemeChoice {
    match choice {
        ThemeChoice::Mix => {
            // `floor(rng() * len)`; below() truncates which == floor for the
            // strictly-[0,1) stream. Falls back to EmojiAnimals (the TS `??`).
            let idx = rng.below(ALL_THEME_IDS.len());
            *ALL_THEME_IDS.get(idx).unwrap_or(&ThemeChoice::EmojiAnimals)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nanoserde::{DeJson, SerJson};

    #[test]
    fn theme_choice_roundtrips_through_strings() {
        for &t in &[
            ThemeChoice::Mix,
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
            assert_eq!(ThemeChoice::from_str(t.as_str()), Some(t));
        }
    }

    #[test]
    fn theme_choice_exact_strings() {
        assert_eq!(ThemeChoice::Mix.as_str(), "mix");
        assert_eq!(ThemeChoice::EmojiAnimals.as_str(), "emoji-animals");
        assert_eq!(ThemeChoice::EmojiFruit.as_str(), "emoji-fruit");
        assert_eq!(ThemeChoice::EmojiVehicles.as_str(), "emoji-vehicles");
        assert_eq!(ThemeChoice::EmojiConstruction.as_str(), "emoji-construction");
        assert_eq!(ThemeChoice::EmojiDinosaurs.as_str(), "emoji-dinosaurs");
        assert_eq!(ThemeChoice::Shapes.as_str(), "shapes");
        assert_eq!(ThemeChoice::LettersUpper.as_str(), "letters-upper");
        assert_eq!(ThemeChoice::LettersLower.as_str(), "letters-lower");
        assert_eq!(ThemeChoice::Numbers.as_str(), "numbers");
    }

    #[test]
    fn theme_choice_json_is_a_bare_string() {
        assert_eq!(ThemeChoice::EmojiVehicles.serialize_json(), "\"emoji-vehicles\"");
        let parsed: ThemeChoice = DeJson::deserialize_json("\"shapes\"").unwrap();
        assert_eq!(parsed, ThemeChoice::Shapes);
    }

    #[test]
    fn theme_choice_rejects_unknown_string() {
        assert!(ThemeChoice::from_str("nope").is_none());
        let res: Result<ThemeChoice, _> = DeJson::deserialize_json("\"nope\"");
        assert!(res.is_err());
    }

    #[test]
    fn pool_sizes_match_spec() {
        assert_eq!(items(ThemeChoice::EmojiAnimals).len(), 28);
        assert_eq!(items(ThemeChoice::EmojiFruit).len(), 8);
        assert_eq!(items(ThemeChoice::EmojiVehicles).len(), 8);
        assert_eq!(items(ThemeChoice::EmojiConstruction).len(), 8);
        assert_eq!(items(ThemeChoice::EmojiDinosaurs).len(), 8);
        assert_eq!(items(ThemeChoice::Shapes).len(), 6);
        assert_eq!(items(ThemeChoice::LettersUpper).len(), 18);
        assert_eq!(items(ThemeChoice::LettersLower).len(), 17);
        assert_eq!(items(ThemeChoice::Numbers).len(), 9);
    }

    #[test]
    fn every_pool_satisfies_max_distinct_count() {
        // Largest template needs 5 distinct items (level-6 AABCD / ABCDE).
        for &t in &ALL_THEME_IDS {
            assert!(items(t).len() >= 5, "{:?} pool too small", t);
        }
    }

    #[test]
    fn letter_set_quirks() {
        let upper = items(ThemeChoice::LettersUpper);
        let upper_ids: Vec<&str> = upper.iter().map(|i| i.id()).collect();
        assert!(upper_ids.contains(&"L"));
        for omitted in ["I", "O", "Q", "U", "V", "W", "X", "Z"] {
            assert!(!upper_ids.contains(&omitted), "upper should omit {omitted}");
        }
        let lower = items(ThemeChoice::LettersLower);
        let lower_ids: Vec<&str> = lower.iter().map(|i| i.id()).collect();
        // lower omits `l` (unlike upper) -> 17 vs 18.
        assert!(!lower_ids.contains(&"l"));
        for omitted in ["i", "o", "q", "u", "v", "w", "x", "z"] {
            assert!(!lower_ids.contains(&omitted), "lower should omit {omitted}");
        }
    }

    #[test]
    fn vehicle_and_construction_glyphs_have_variation_selectors() {
        let v = items(ThemeChoice::EmojiVehicles);
        assert!(v.contains(&Item::Glyph("✈️"))); // U+2708 U+FE0F
        assert!(v.contains(&Item::Glyph("⛵")));
        let c = items(ThemeChoice::EmojiConstruction);
        assert!(c.contains(&Item::Glyph("🏗️"))); // U+1F3D7 U+FE0F
        // digger reuses the tractor glyph (same Glyph value here).
        assert!(c.contains(&Item::Glyph("🚜")));
    }

    #[test]
    fn shape_data_matches_spec() {
        let s = items(ThemeChoice::Shapes);
        assert_eq!(s[0].id(), "red-circle");
        match &s[0] {
            Item::Shape { shape, .. } => {
                assert_eq!(shape.color, 0xef476f);
                assert_eq!(shape.radius, Some("50%"));
                assert_eq!(shape.clip, None);
            }
            _ => panic!("expected shape"),
        }
        match &s[2] {
            Item::Shape { id, shape } => {
                assert_eq!(*id, "yellow-triangle");
                assert_eq!(shape.color, 0xffd166);
                assert_eq!(shape.radius, None);
                assert_eq!(shape.clip, Some("polygon(50% 0, 100% 100%, 0 100%)"));
            }
            _ => panic!("expected shape"),
        }
    }

    #[test]
    fn item_id_is_glyph_for_glyphs() {
        assert_eq!(Item::Glyph("🐶").id(), "🐶");
        assert_eq!(Item::Glyph("A").id(), "A");
    }

    #[test]
    fn resolve_fixed_theme_consumes_no_rng() {
        let mut a = Mulberry32::new(123);
        let mut b = Mulberry32::new(123);
        let resolved = resolve_theme(ThemeChoice::Numbers, &mut a);
        assert_eq!(resolved, ThemeChoice::Numbers);
        // a's stream must be untouched.
        assert_eq!(a.next_f64(), b.next_f64());
    }

    #[test]
    fn resolve_mix_consumes_exactly_one_rng_and_indexes_all_theme_ids() {
        let mut a = Mulberry32::new(0xC0FFEE);
        let mut b = Mulberry32::new(0xC0FFEE);
        let idx = b.below(ALL_THEME_IDS.len());
        let expected = ALL_THEME_IDS[idx];
        let resolved = resolve_theme(ThemeChoice::Mix, &mut a);
        assert_eq!(resolved, expected);
        // Both streams advanced by exactly one call.
        assert_eq!(a.next_f64(), b.next_f64());
    }

    #[test]
    fn resolve_mix_deterministic_for_same_seed() {
        let mut a = Mulberry32::new(42);
        let mut b = Mulberry32::new(42);
        assert_eq!(
            resolve_theme(ThemeChoice::Mix, &mut a),
            resolve_theme(ThemeChoice::Mix, &mut b)
        );
    }
}
