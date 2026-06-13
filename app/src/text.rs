//! Font loading + centered text helpers. VicModernCursive is baked into the
//! binary via include_bytes! so there is no asset-path / web-fetch dependency
//! on any platform, and the glyph atlas is identical everywhere.
use macroquad::prelude::*;
use std::cell::RefCell;

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

/// Like [`draw_centered`], but the glyphs are rotated `rot` radians (clockwise,
/// screen y-down) about their visual center `(cx, cy)` — so the centered text
/// rides a tilted surface (e.g. a bunting flag) instead of staying upright.
pub fn draw_centered_rot(text: &str, cx: f32, cy: f32, size: u16, font: &Font, color: Color, rot: f32) {
    let dim = measure_text(text, Some(font), size, 1.0);
    // macroquad rotates a text run about its draw anchor (the baseline-left
    // pen point). The unrotated center sits (width/2) right and (offset_y/2) up
    // from there; place the anchor so that vector, once rotated, lands on the
    // requested center.
    let (s, c) = rot.sin_cos();
    let (ox, oy) = (dim.width / 2.0, -dim.offset_y / 2.0);
    let x = cx - (ox * c - oy * s);
    let y = cy - (ox * s + oy * c);
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(font),
            font_size: size,
            rotation: rot,
            color,
            ..Default::default()
        },
    );
}

// --- UI font (Varela Round) ------------------------------------------------
// Clean rounded sans for chrome, labels, parent menu, HUD. Cursive is reserved
// for letter/number learning stimuli. Baked in + held thread-local so the free
// `ui_*` helpers can reach it without threading a font through every call.
thread_local! {
    static UI_FONT: RefCell<Option<Font>> = const { RefCell::new(None) };
}

/// Load the UI font into thread-local storage. Call once after the GL context
/// exists (inside the macroquad main). Idempotent.
pub fn init_ui() {
    UI_FONT.with(|f| {
        if f.borrow().is_none() {
            if let Ok(font) = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/ui.ttf")) {
                *f.borrow_mut() = Some(font);
            }
        }
    });
}

/// UI text centered on (cx,cy), in the rounded sans.
pub fn ui_centered(text: &str, cx: f32, cy: f32, size: u16, color: Color) {
    UI_FONT.with(|uf| {
        let b = uf.borrow();
        let font = b.as_ref();
        let dim = measure_text(text, font, size, 1.0);
        draw_text_ex(
            text,
            cx - dim.width / 2.0,
            cy + dim.offset_y / 2.0,
            TextParams { font, font_size: size, color, ..Default::default() },
        );
    });
}
/// UI text left-aligned at `x`, vertically centered on `cy`.
pub fn ui_left(text: &str, x: f32, cy: f32, size: u16, color: Color) {
    UI_FONT.with(|uf| {
        let b = uf.borrow();
        let font = b.as_ref();
        let dim = measure_text(text, font, size, 1.0);
        draw_text_ex(
            text,
            x,
            cy + dim.offset_y / 2.0,
            TextParams { font, font_size: size, color, ..Default::default() },
        );
    });
}

/// Draw text left-aligned at `x`, vertically centered on `cy`.
pub fn draw_centered_left(text: &str, x: f32, cy: f32, size: u16, font: &Font, color: Color) {
    let dim = measure_text(text, Some(font), size, 1.0);
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
