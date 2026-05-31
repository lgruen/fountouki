//! Parent settings panel — a modal overlay opened by long-pressing ←. Holds the
//! universal Sync controls (token + endpoint) and a per-game section (patterns:
//! cyclers + start-over; phonics: read-only mastery). Drawn on top of a dimmed
//! scene; closing persists changes. No native widgets — selects are tap-to-cycle
//! chips, the token/endpoint are tap-to-focus text fields.
use crate::{draw, input, palette, scene::Ctx, store::Db, text};
use fountouki_core::{
    rng::Mulberry32,
    settings::{self, PatternsSettings, SharedSettings},
    srs,
    themes::ThemeChoice,
};
use macroquad::prelude::*;

pub enum PanelResult {
    Stay,
    /// Close, applying changes. `rebuild` = the current game scene should be
    /// recreated to pick up new settings (patterns only).
    Close { rebuild: bool },
}

#[derive(PartialEq, Clone, Copy)]
enum Focus {
    None,
    Token,
    Endpoint,
}

/// Theme cycle order (Mix first, then the nine concrete themes).
const THEME_CYCLE: [ThemeChoice; 10] = [
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
];
const DIFFS: [&str; 3] = ["auto", "easy", "hard"];
const MODES: [&str; 2] = ["next", "unit"];

pub struct ParentPanel {
    db: Db,
    game: String,
    token: String,
    endpoint: String,
    focus: Focus,
    ptn: PatternsSettings,
    ptn_dirty: bool,
    start_over: bool,
    mastery: Option<Mastery>,
    seed: u32,
}

struct Mastery {
    mastered: u32,
    strong: u32,
    learning: u32,
    new: u32,
    boxes: Vec<(char, u8)>, // a..z, box
}

impl ParentPanel {
    pub fn open(db: Db, game: &str, now: i64, seed: u32) -> ParentPanel {
        let shared = {
            let kv = db.borrow_kv();
            settings::load_shared(&**kv)
        };
        let ptn = {
            let kv = db.borrow_kv();
            settings::load_patterns(&**kv)
        };
        let mastery = if game == "phonics" {
            Some(compute_mastery(&db, now))
        } else {
            None
        };
        ParentPanel {
            db,
            game: game.to_string(),
            token: shared.sync_token.unwrap_or_default(),
            endpoint: shared.sync_endpoint.unwrap_or_default(),
            focus: Focus::None,
            ptn,
            ptn_dirty: false,
            start_over: false,
            mastery,
            seed,
        }
    }

    fn apply(&self) {
        let mut s = {
            let kv = self.db.borrow_kv();
            settings::load_shared(&**kv)
        };
        s.sync_token = none_if_empty(&self.token);
        s.sync_endpoint = none_if_empty(&self.endpoint);
        {
            let mut kv = self.db.borrow_kv_mut();
            settings::save_shared(&mut **kv, &s);
            if self.game == "patterns" {
                settings::save_patterns(&mut **kv, &self.ptn);
            }
        }
    }

    pub fn took_start_over(&self) -> bool {
        self.start_over
    }

    pub fn update(&mut self, ctx: &Ctx) -> PanelResult {
        let l = layout(&ctx.frame, &self.game);
        // Text entry into the focused field.
        if self.focus != Focus::None {
            while let Some(c) = get_char_pressed() {
                let buf = if self.focus == Focus::Token { &mut self.token } else { &mut self.endpoint };
                if self.focus == Focus::Token {
                    if c.is_ascii_alphanumeric() && buf.len() < 64 {
                        buf.push(c.to_ascii_lowercase());
                    }
                } else if !c.is_control() && buf.len() < 120 {
                    buf.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                if self.focus == Focus::Token {
                    self.token.pop();
                } else {
                    self.endpoint.pop();
                }
            }
        }

        let pt = ctx.pointer;
        if !pt.tapped() {
            return PanelResult::Stay;
        }
        // Outside the card → close.
        if !input::hit_rect(pt.pos, l.card.x, l.card.y, l.card.w, l.card.h) {
            self.apply();
            return PanelResult::Close { rebuild: self.ptn_dirty };
        }
        // Focus fields.
        self.focus = if input::hit_rect(pt.pos, l.token.x, l.token.y, l.token.w, l.token.h) {
            Focus::Token
        } else if input::hit_rect(pt.pos, l.endpoint.x, l.endpoint.y, l.endpoint.w, l.endpoint.h) {
            Focus::Endpoint
        } else {
            Focus::None
        };
        if hit(pt.pos, l.gen) {
            let mut rng = Mulberry32::new(self.seed);
            self.token = settings::generate_token(&mut rng);
        }
        if hit(pt.pos, l.clear) {
            self.token.clear();
        }
        if self.game == "patterns" {
            if hit(pt.pos, l.theme) {
                self.ptn.theme_choice = cycle_theme(&self.ptn.theme_choice);
                self.ptn_dirty = true;
            }
            if hit(pt.pos, l.diff) {
                self.ptn.difficulty = cycle(&DIFFS, &self.ptn.difficulty);
                self.ptn_dirty = true;
            }
            if hit(pt.pos, l.mode) {
                self.ptn.mode = cycle(&MODES, &self.ptn.mode);
                self.ptn_dirty = true;
            }
            if hit(pt.pos, l.hint) {
                self.ptn.show_hint = !self.ptn.show_hint;
                self.ptn_dirty = true;
            }
            if hit(pt.pos, l.start_over) {
                self.start_over = true;
                self.apply();
                return PanelResult::Close { rebuild: true };
            }
        }
        if hit(pt.pos, l.done) {
            self.apply();
            return PanelResult::Close { rebuild: self.ptn_dirty };
        }
        PanelResult::Stay
    }

    pub fn draw(&mut self, ctx: &Ctx) {
        // Dim scrim over the (already-drawn) scene.
        draw_rectangle(0.0, 0.0, ctx.frame.w, ctx.frame.h, palette::SCRIM);
        let l = layout(&ctx.frame, &self.game);
        draw::card(l.card.x, l.card.y, l.card.w, l.card.h, palette::CARD);
        let fnt = &ctx.fonts.cursive;

        text::draw_centered("parent settings", l.card.x + l.card.w / 2.0, l.card.y + 30.0, 26, fnt, palette::INK);

        if self.game == "patterns" {
            chip(l.theme, "pictures", theme_label(&self.ptn.theme_choice), fnt, ctx);
            chip(l.diff, "helpers", &self.ptn.difficulty, fnt, ctx);
            chip(l.mode, "game", &self.ptn.mode, fnt, ctx);
            chip(l.hint, "highlight piece", if self.ptn.show_hint { "on" } else { "off" }, fnt, ctx);
            button(l.start_over, "start over", palette::ACCENT, palette::WHITE, fnt, ctx);
        }
        if let Some(m) = &self.mastery {
            draw_mastery(l.card, m, fnt, ctx);
        }

        // Sync section.
        label(l.token, "sync token", fnt, ctx);
        field(l.token, &self.token, self.focus == Focus::Token, "(empty = no sync)", fnt, ctx);
        button(l.gen, "generate", palette::OK, palette::OK_STRONG, fnt, ctx);
        button(l.clear, "clear", palette::CARD, palette::MUTED, fnt, ctx);
        label(l.endpoint, "endpoint", fnt, ctx);
        field(l.endpoint, &self.endpoint, self.focus == Focus::Endpoint, settings_default_endpoint(), fnt, ctx);

        button(l.done, "done", palette::ACCENT, palette::WHITE, fnt, ctx);
    }
}

fn settings_default_endpoint() -> &'static str {
    fountouki_core::sync::DEFAULT_ENDPOINT
}

fn none_if_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn compute_mastery(db: &Db, now: i64) -> Mastery {
    let key = fountouki_core::storage::ns_key("phonics", "state");
    let mut st = db
        .get(&key)
        .and_then(|raw| srs::validate(&raw))
        .unwrap_or_else(srs::empty_state);
    srs::ensure_letters(&mut st, now);
    let (mut mastered, mut strong, mut learning, mut new) = (0, 0, 0, 0);
    let mut boxes: Vec<(char, u8)> = Vec::new();
    for c in 'a'..='z' {
        let ls = st.letters.get(&c.to_string());
        let (b, seen) = ls.map(|l| (l.box_, l.last_seen)).unwrap_or((0, 0));
        boxes.push((c, b));
        if seen == 0 {
            new += 1;
        } else if b >= srs::MASTERED_BOX {
            mastered += 1;
        } else if b >= srs::STRONG_MIN_BOX {
            strong += 1;
        } else {
            learning += 1;
        }
    }
    Mastery { mastered, strong, learning, new, boxes }
}

fn cycle(opts: &[&str], cur: &str) -> String {
    let i = opts.iter().position(|o| *o == cur).unwrap_or(0);
    opts[(i + 1) % opts.len()].to_string()
}
fn cycle_theme(cur: &str) -> String {
    let i = THEME_CYCLE
        .iter()
        .position(|t| t.as_str() == cur)
        .unwrap_or(0);
    THEME_CYCLE[(i + 1) % THEME_CYCLE.len()].as_str().to_string()
}
fn theme_label(cur: &str) -> &'static str {
    ThemeChoice::from_str(cur)
        .map(fountouki_core::themes::label)
        .unwrap_or("mix")
}

// --- layout + control drawing ----------------------------------------------

struct Layout {
    card: Rect,
    theme: Rect,
    diff: Rect,
    mode: Rect,
    hint: Rect,
    start_over: Rect,
    token: Rect,
    gen: Rect,
    clear: Rect,
    endpoint: Rect,
    done: Rect,
}

fn layout(f: &crate::layout::Frame, game: &str) -> Layout {
    let cw = (f.w * 0.5).clamp(420.0, 660.0);
    let ch = (f.h * 0.86).clamp(360.0, 760.0);
    let cx = f.w / 2.0 - cw / 2.0;
    let cy = f.h / 2.0 - ch / 2.0;
    let pad = 28.0;
    let rw = cw - 2.0 * pad;
    let rh = 52.0;
    let mut y = cy + 64.0;
    // `mk` takes `&mut y` (rather than capturing it) so we can still read/adjust
    // y directly between rows.
    let mk = |y: &mut f32| {
        let r = Rect::new(cx + pad, *y, rw, rh);
        *y += rh + 14.0;
        r
    };
    let (theme, diff, mode, hint, start_over) = if game == "patterns" {
        (mk(&mut y), mk(&mut y), mk(&mut y), mk(&mut y), mk(&mut y))
    } else {
        let z = Rect::new(-100.0, -100.0, 0.0, 0.0);
        // leave room for the phonics mastery block
        if game == "phonics" {
            y += 150.0;
        }
        (z, z, z, z, z)
    };
    let token = mk(&mut y);
    let half = (rw - 14.0) / 2.0;
    let gen = Rect::new(cx + pad, y, half, 44.0);
    let clear = Rect::new(cx + pad + half + 14.0, y, half, 44.0);
    y += 44.0 + 14.0;
    let endpoint = mk(&mut y);
    let done = Rect::new(cx + cw / 2.0 - 80.0, cy + ch - 60.0, 160.0, 46.0);
    Layout { card: Rect::new(cx, cy, cw, ch), theme, diff, mode, hint, start_over, token, gen, clear, endpoint, done }
}

fn hit(p: Vec2, r: Rect) -> bool {
    input::hit_rect(p, r.x, r.y, r.w, r.h)
}

fn label(r: Rect, t: &str, fnt: &Font, ctx: &Ctx) {
    text::draw_centered_left(t, r.x, r.y - 6.0, 16, fnt, palette::MUTED);
}

fn chip(r: Rect, name: &str, value: &str, fnt: &Font, ctx: &Ctx) {
    text::draw_centered_left(name, r.x, r.y - 6.0, 15, fnt, palette::MUTED);
    draw::rounded_rect(r.x, r.y, r.w, r.h, 12.0, palette::CARD);
    draw::rounded_rect(r.x, r.y, r.w, r.h, 12.0, Color::new(0.0, 0.0, 0.0, 0.03));
    text::draw_centered(value, r.x + r.w / 2.0, r.y + r.h / 2.0, 22, fnt, palette::INK);
}

fn field(r: Rect, value: &str, focused: bool, placeholder: &str, fnt: &Font, ctx: &Ctx) {
    let ring = if focused { palette::ACCENT } else { palette::FIELD_BORDER };
    draw::rounded_rect(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, 12.0, ring);
    draw::rounded_rect(r.x, r.y, r.w, r.h, 12.0, palette::WHITE);
    let (txt, col) = if value.is_empty() {
        (placeholder, palette::MUTED)
    } else {
        (value, palette::INK)
    };
    text::draw_centered_left(txt, r.x + 14.0, r.y + r.h / 2.0, 18, fnt, col);
}

fn button(r: Rect, t: &str, fill: Color, fg: Color, fnt: &Font, ctx: &Ctx) {
    draw::rounded_rect(r.x, r.y, r.w, r.h, 14.0, fill);
    text::draw_centered(t, r.x + r.w / 2.0, r.y + r.h / 2.0, 20, fnt, fg);
}

fn draw_mastery(card: Rect, m: &Mastery, fnt: &Font, ctx: &Ctx) {
    let x = card.x + 28.0;
    let mut y = card.y + 70.0;
    text::draw_centered_left(
        &format!("mastered {}  strong {}  learning {}  new {}", m.mastered, m.strong, m.learning, m.new),
        x,
        y,
        16,
        fnt,
        palette::MUTED,
    );
    y += 26.0;
    let dot = 14.0;
    let gap = 6.0;
    let per_row = 13;
    for (i, (_c, b)) in m.boxes.iter().enumerate() {
        let col = i % per_row;
        let rrow = i / per_row;
        let cx = x + col as f32 * (dot + gap) + dot / 2.0;
        let cy = y + rrow as f32 * (dot + gap) + dot / 2.0;
        draw_circle(cx, cy, dot / 2.0, palette::MASTERY[(*b as usize).min(4)]);
    }
}
