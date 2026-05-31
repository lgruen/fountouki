//! Bundled emoji sprite set.
//!
//! The OS emoji font renders differently on every platform (Apple vs. Noto
//! vs. Segoe …), so we ship our own PNGs and draw them as textures. This
//! makes every glyph pixel-identical regardless of where the app runs.
//!
//! Art is **Twemoji** by the jdecked fork, licensed **CC-BY 4.0**. The PNGs
//! live in `app/assets/emoji/` and are `include_bytes!`-baked into the binary.
//! Attribution: <https://github.com/jdecked/twemoji> (see
//! `app/assets/emoji/ATTRIBUTION.md`).
//!
//! Keys are the EXACT emoji strings as they appear in `core`'s `themes.rs` /
//! `deck.rs` (including any U+FE0F variation selector and keycap ZWJ
//! sequences), so callers look a sprite up by the raw glyph. The set is the
//! union of every emoji those two modules use (110 distinct glyphs).

use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// A lookup table from emoji glyph string → bundled sprite texture.
pub struct EmojiSet {
    map: HashMap<&'static str, Texture2D>,
}

impl EmojiSet {
    /// Decode every bundled emoji PNG into a `Texture2D` (linear filtering for
    /// smooth scaling) and build the glyph → texture map.
    pub fn load() -> EmojiSet {
        let mut map: HashMap<&'static str, Texture2D> = HashMap::new();

        macro_rules! insert {
            ($glyph:literal, $file:literal) => {{
                let tex = Texture2D::from_file_with_format(
                    include_bytes!(concat!("../assets/emoji/", $file)),
                    Some(ImageFormat::Png),
                );
                tex.set_filter(FilterMode::Linear);
                map.insert($glyph, tex);
            }};
        }

        insert!("🐶", "1f436.png");
        insert!("🐱", "1f431.png");
        insert!("🐰", "1f430.png");
        insert!("🐻", "1f43b.png");
        insert!("🐼", "1f43c.png");
        insert!("🐯", "1f42f.png");
        insert!("🐸", "1f438.png");
        insert!("🐵", "1f435.png");
        insert!("🦁", "1f981.png");
        insert!("🦊", "1f98a.png");
        insert!("🐮", "1f42e.png");
        insert!("🐷", "1f437.png");
        insert!("🐭", "1f42d.png");
        insert!("🐹", "1f439.png");
        insert!("🐨", "1f428.png");
        insert!("🐘", "1f418.png");
        insert!("🦒", "1f992.png");
        insert!("🦓", "1f993.png");
        insert!("🐴", "1f434.png");
        insert!("🦄", "1f984.png");
        insert!("🐧", "1f427.png");
        insert!("🐤", "1f424.png");
        insert!("🦉", "1f989.png");
        insert!("🐳", "1f433.png");
        insert!("🐙", "1f419.png");
        insert!("🐠", "1f420.png");
        insert!("🐝", "1f41d.png");
        insert!("🦋", "1f98b.png");
        insert!("🍎", "1f34e.png");
        insert!("🍌", "1f34c.png");
        insert!("🍇", "1f347.png");
        insert!("🍓", "1f353.png");
        insert!("🍊", "1f34a.png");
        insert!("🥝", "1f95d.png");
        insert!("🍐", "1f350.png");
        insert!("🍉", "1f349.png");
        insert!("🚗", "1f697.png");
        insert!("🚌", "1f68c.png");
        insert!("🚂", "1f682.png");
        insert!("✈️", "2708.png");
        insert!("🚀", "1f680.png");
        insert!("🚲", "1f6b2.png");
        insert!("⛵", "26f5.png");
        insert!("🚜", "1f69c.png");
        insert!("🏗️", "1f3d7.png");
        insert!("🚛", "1f69b.png");
        insert!("🚧", "1f6a7.png");
        insert!("🔨", "1f528.png");
        insert!("🔧", "1f527.png");
        insert!("🪚", "1fa9a.png");
        insert!("🧰", "1f9f0.png");
        insert!("🦖", "1f996.png");
        insert!("🦕", "1f995.png");
        insert!("🐊", "1f40a.png");
        insert!("🐢", "1f422.png");
        insert!("🦎", "1f98e.png");
        insert!("🐉", "1f409.png");
        insert!("🥚", "1f95a.png");
        insert!("🦴", "1f9b4.png");
        insert!("🐜", "1f41c.png");
        insert!("🎈", "1f388.png");
        insert!("🥕", "1f955.png");
        insert!("🐄", "1f404.png");
        insert!("🐕", "1f415.png");
        insert!("🦆", "1f986.png");
        insert!("🐟", "1f41f.png");
        insert!("🌸", "1f338.png");
        insert!("🐐", "1f410.png");
        insert!("🎁", "1f381.png");
        insert!("🏠", "1f3e0.png");
        insert!("🎩", "1f3a9.png");
        insert!("🐛", "1f41b.png");
        insert!("🪻", "1fabb.png");
        insert!("🪼", "1fabc.png");
        insert!("🎷", "1f3b7.png");
        insert!("🃏", "1f0cf.png");
        insert!("🦘", "1f998.png");
        insert!("🗝️", "1f5dd.png");
        insert!("🪁", "1fa81.png");
        insert!("🍋", "1f34b.png");
        insert!("🐞", "1f41e.png");
        insert!("🌙", "1f319.png");
        insert!("🍄", "1f344.png");
        insert!("🪺", "1faba.png");
        insert!("👃", "1f443.png");
        insert!("🥜", "1f95c.png");
        insert!("🍍", "1f34d.png");
        insert!("👸", "1f478.png");
        insert!("🪶", "1fab6.png");
        insert!("❓", "2753.png");
        insert!("🌈", "1f308.png");
        insert!("🤖", "1f916.png");
        insert!("☀️", "2600.png");
        insert!("🐍", "1f40d.png");
        insert!("⭐", "2b50.png");
        insert!("🐅", "1f405.png");
        insert!("🌳", "1f333.png");
        insert!("☂️", "2602.png");
        insert!("🆙", "1f199.png");
        insert!("🚐", "1f690.png");
        insert!("🎻", "1f3bb.png");
        insert!("🌋", "1f30b.png");
        insert!("🌊", "1f30a.png");
        insert!("🩻", "1fa7b.png");
        insert!("📦", "1f4e6.png");
        insert!("6️⃣", "36-20e3.png");
        insert!("🪀", "1fa80.png");
        insert!("🟡", "1f7e1.png");
        insert!("0️⃣", "30-20e3.png");
        insert!("💤", "1f4a4.png");

        EmojiSet { map }
    }

    /// The bundled sprite for `e` (the exact glyph string), or `None` if this
    /// emoji isn't in the set.
    pub fn get(&self, e: &str) -> Option<&Texture2D> {
        self.map.get(e)
    }
}

thread_local! {
    static EMOJI: RefCell<Option<EmojiSet>> = const { RefCell::new(None) };
}

/// Load the sprite set into thread-local storage. Call once after the GL
/// context exists (inside the macroquad main). Idempotent.
pub fn init() {
    EMOJI.with(|e| {
        if e.borrow().is_none() {
            *e.borrow_mut() = Some(EmojiSet::load());
        }
    });
}

/// Cloneable handle to a glyph's sprite (`Texture2D` is a cheap GPU handle), or
/// `None` if the glyph isn't bundled / the set isn't initialized.
pub fn texture(glyph: &str) -> Option<Texture2D> {
    EMOJI.with(|e| e.borrow().as_ref().and_then(|s| s.get(glyph).cloned()))
}
