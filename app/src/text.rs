//! Font loading + centered text helpers. The handwriting font is baked into the
//! binary via include_bytes! so there is no asset-path / web-fetch dependency
//! on any platform, and the glyph atlas is identical everywhere.
//!
//! **Size classes matter at every call site.** In the handwriting font (UPEM
//! 1000) the ink heights per em are:
//! - lowercase x-height (a c e m n o …) ≈ 0.40 em
//! - caps, ascenders (b d h k l t) and **digits** ≈ 0.78 em
//!
//! So a digit drawn at `font_size` is ~1.94× taller than a lowercase letter at
//! the same `font_size`. A call site that sizes a numeral must pick its ratio
//! against the ~0.78 em cap box, not the ~0.40 em x-height. Digits also carry
//! real side bearings, so multi-digit numbers pack correctly with plain
//! [`draw_centered`] — no app-side tracking hack.
use macroquad::prelude::*;
use std::cell::RefCell;

pub struct Fonts {
    /// Fountouki Handwriting — the self-authored Tasmanian-style print
    /// letterform the kids are taught to write (single-story a/g, unjoined).
    /// Used ONLY for letter/number learning stimuli; chrome uses the UI font.
    pub handwriting: Font,
}

impl Fonts {
    pub fn load() -> Fonts {
        let handwriting =
            load_ttf_font_from_bytes(include_bytes!("../assets/fonts/handwriting.ttf"))
                .expect("handwriting.ttf");
        Fonts { handwriting }
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
// Clean rounded sans for chrome, labels, parent menu, HUD. The handwriting font
// is reserved for letter/number learning stimuli. Baked in + held thread-local
// so the free `ui_*` helpers can reach it without threading a font through
// every call.
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
