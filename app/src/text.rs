//! Font loading + centered text helpers. VicModernCursive is baked into the
//! binary via include_bytes! so there is no asset-path / web-fetch dependency
//! on any platform, and the glyph atlas is identical everywhere.
use macroquad::prelude::*;

pub struct Fonts {
    /// VicModernCursive — the canonical learn-to-write letterform (single-story
    /// a/g). Used ONLY for letter/number learning stimuli.
    pub cursive: Font,
    pub cursive_bold: Font,
}

impl Fonts {
    pub fn load() -> Fonts {
        let cursive = load_ttf_font_from_bytes(include_bytes!(
            "../assets/fonts/VicModernCursive-Regular.ttf"
        ))
        .expect("VicModernCursive-Regular");
        let cursive_bold = load_ttf_font_from_bytes(include_bytes!(
            "../assets/fonts/VicModernCursive-Bold.ttf"
        ))
        .expect("VicModernCursive-Bold");
        Fonts {
            cursive,
            cursive_bold,
        }
    }
}

/// Draw text centered horizontally on `cx`, vertically centered on `cy`
/// (using the measured cap box so big glyphs sit visually centered).
pub fn draw_centered(text: &str, cx: f32, cy: f32, size: u16, font: &Font, color: Color) {
    let dim = measure_text(text, Some(font), size, 1.0);
    let x = cx - dim.width / 2.0;
    // offset_y is the distance from the draw baseline to the top of the glyphs;
    // centering the cap box means baseline = cy + offset_y/2.
    let y = cy + dim.offset_y / 2.0;
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(font),
            font_size: size,
            color,
            ..Default::default()
        },
    );
}
