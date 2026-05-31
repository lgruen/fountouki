//! Parent settings panel — a modal overlay opened by long-pressing ←. Holds the
//! universal Sync controls (token + endpoint) and a per-game section (patterns:
//! cyclers + start-over; phonics: read-only mastery). Drawn on top of a dimmed
//! scene; closing persists changes. No native widgets — selects are tap-to-cycle
//! chips, the token/endpoint are tap-to-focus text fields.
use crate::{draw, input, palette, scene::Ctx, store::Db, text};
use fountouki_core::{
    rng::Mulberry32,
    settings::{self, PatternsSettings},
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
    /// Pixels the body content is scrolled up (0 = top). Clamped to the
    /// content's overflow each frame; always 0 when everything fits.
    scroll: f32,
    /// Last pointer-y while dragging the body, for frame-to-frame delta.
    drag_y: Option<f32>,
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
            scroll: 0.0,
            drag_y: None,
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
        let l = layout(&ctx.frame, &self.game, self.scroll);
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

        // Drag- and wheel-scroll the body. A drag (>16px) won't register as a
        // tap, so this never conflicts with the button hit-tests below.
        if pt.down {
            if let Some(prev) = self.drag_y {
                if l.max_scroll > 0.0 {
                    self.scroll -= pt.pos.y - prev;
                }
            }
            self.drag_y = Some(pt.pos.y);
        } else {
            self.drag_y = None;
        }
        self.scroll -= mouse_wheel().1 * 0.6;
        self.scroll = self.scroll.clamp(0.0, l.max_scroll);

        if !pt.tapped() {
            return PanelResult::Stay;
        }
        // Outside the card → close.
        if !hit(pt.pos, l.card) {
            self.apply();
            return PanelResult::Close { rebuild: self.ptn_dirty };
        }
        // Body controls are only tappable where they're actually visible (inside
        // the scroll viewport); the pinned `done` button is always live.
        let in_body = hit(pt.pos, l.view);
        self.focus = if in_body && hit(pt.pos, l.token) {
            Focus::Token
        } else if in_body && hit(pt.pos, l.endpoint) {
            Focus::Endpoint
        } else {
            Focus::None
        };
        if in_body && hit(pt.pos, l.gen) {
            let mut rng = Mulberry32::new(self.seed);
            self.token = settings::generate_token(&mut rng);
        }
        if in_body && hit(pt.pos, l.clear) {
            self.token.clear();
        }
        if self.game == "patterns" && in_body {
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
        let l = layout(&ctx.frame, &self.game, self.scroll);
        draw::card(l.card.x, l.card.y, l.card.w, l.card.h, palette::CARD);
        text::ui_centered("parent settings", l.card.x + l.card.w / 2.0, l.title_y, 26, palette::INK);

        // Scrollable body, clipped to the viewport so rows can't bleed into the
        // pinned title/done bands or off the card.
        draw::push_clip(l.view.x, l.view.y, l.view.w, l.view.h);
        if self.game == "patterns" {
            chip(l.theme, "pictures", theme_label(&self.ptn.theme_choice));
            chip(l.diff, "helpers", &self.ptn.difficulty);
            chip(l.mode, "game", &self.ptn.mode);
            chip(l.hint, "highlight piece", if self.ptn.show_hint { "on" } else { "off" });
            button(l.start_over, "start over", palette::ACCENT, palette::WHITE);
        }
        if let Some(m) = &self.mastery {
            draw_mastery(l.mastery, m);
        }
        // Sync section.
        label(l.token, "sync token");
        field(l.token, &self.token, self.focus == Focus::Token, "(empty = no sync)");
        button(l.gen, "generate", palette::OK, palette::OK_STRONG);
        button(l.clear, "clear", palette::CARD, palette::MUTED);
        label(l.endpoint, "endpoint");
        field(l.endpoint, &self.endpoint, self.focus == Focus::Endpoint, settings_default_endpoint());
        draw::pop_clip();

        scrollbar(&l, self.scroll);

        button(l.done, "done", palette::ACCENT, palette::WHITE);
    }
}

/// Thin scroll-position thumb on the card's right edge (only when overflowing).
fn scrollbar(l: &Layout, scroll: f32) {
    if l.max_scroll <= 0.5 {
        return;
    }
    let track = l.view.h;
    let thumb = (track * track / l.inner_h).clamp(28.0, track);
    let t = scroll / l.max_scroll;
    let y = l.view.y + t * (track - thumb);
    let x = l.card.x + l.card.w - 10.0;
    draw::rounded_rect(x, y, 4.0, thumb, 2.0, palette::MUTED);
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
    title_y: f32,
    /// Scroll viewport: body rows are clipped to and hit-tested against this.
    view: Rect,
    theme: Rect,
    diff: Rect,
    mode: Rect,
    hint: Rect,
    start_over: Rect,
    mastery: Rect,
    token: Rect,
    gen: Rect,
    clear: Rect,
    endpoint: Rect,
    done: Rect,
    /// Total body content height, and the overflow past the viewport.
    inner_h: f32,
    max_scroll: f32,
}

/// Layout = pinned title band, a scrollable body, and a pinned `done` band. The
/// body rows are laid out in local coords (y from 0) then shifted by `-scroll`;
/// the card fits its content when it can, else fills the safe viewport and the
/// body scrolls (phones in landscape are too short for the full form).
fn layout(f: &crate::layout::Frame, game: &str, scroll: f32) -> Layout {
    const PAD: f32 = 26.0;
    const HEADER: f32 = 54.0; // pinned title band
    const FOOTER: f32 = 66.0; // pinned done band
    const RH: f32 = 46.0; // labeled-control height
    const LBL: f32 = 22.0; // space reserved for a label above a control
    const GAP: f32 = 16.0; // gap below each row
    const BTN_H: f32 = 44.0;

    let cw = (f.w * 0.5).clamp(420.0, 660.0);
    let rw = cw - 2.0 * PAD;

    // --- body rows in LOCAL coords (y grows down from 0) ---
    // A little headroom so the first row's label doesn't kiss the clip edge.
    let mut ly = 14.0;
    let labeled = |ly: &mut f32| {
        let top = *ly + LBL;
        *ly = top + RH + GAP;
        (top, RH)
    };
    let block = |ly: &mut f32, h: f32| {
        let top = *ly;
        *ly = top + h + GAP;
        (top, h)
    };

    let z2 = (0.0_f32, 0.0_f32);
    let (mut theme_l, mut diff_l, mut mode_l, mut hint_l, mut start_l) = (z2, z2, z2, z2, z2);
    let mut mastery_l = z2;
    if game == "patterns" {
        theme_l = labeled(&mut ly);
        diff_l = labeled(&mut ly);
        mode_l = labeled(&mut ly);
        hint_l = labeled(&mut ly);
        start_l = block(&mut ly, BTN_H);
    } else if game == "phonics" {
        mastery_l = block(&mut ly, 78.0);
    }
    let token_l = labeled(&mut ly);
    let gen_l = block(&mut ly, BTN_H);
    let endpoint_l = labeled(&mut ly);
    let inner_h = ly;

    // --- card sizing: fit content, else fill the safe viewport and scroll ---
    let v_margin = f.safe.top.max(14.0) + f.safe.bottom.max(14.0);
    let avail = (f.h - v_margin).max(220.0);
    let ch = (HEADER + inner_h + FOOTER).min(avail);
    let cx = f.w / 2.0 - cw / 2.0;
    let cy = (f.h - ch) / 2.0;

    let view = Rect::new(cx, cy + HEADER, cw, ch - HEADER - FOOTER);
    let max_scroll = (inner_h - view.h).max(0.0);
    let s = scroll.clamp(0.0, max_scroll);

    // local row → on-screen rect (left-aligned in the padded column).
    let lx = cx + PAD;
    let half = (rw - 14.0) / 2.0;
    let row = |loc: (f32, f32), w: f32| Rect::new(lx, view.y - s + loc.0, w, loc.1);

    let gen = row(gen_l, half);
    let clear = Rect::new(lx + half + 14.0, gen.y, half, BTN_H);
    let off = Rect::new(-1000.0, -1000.0, 0.0, 0.0);
    let (theme, diff, mode, hint, start_over) = if game == "patterns" {
        (row(theme_l, rw), row(diff_l, rw), row(mode_l, rw), row(hint_l, rw), row(start_l, rw))
    } else {
        (off, off, off, off, off)
    };

    let done = Rect::new(cx + cw / 2.0 - 80.0, cy + ch - FOOTER + (FOOTER - 46.0) / 2.0, 160.0, 46.0);

    Layout {
        card: Rect::new(cx, cy, cw, ch),
        title_y: cy + 30.0,
        view,
        theme,
        diff,
        mode,
        hint,
        start_over,
        mastery: row(mastery_l, rw),
        token: row(token_l, rw),
        gen,
        clear,
        endpoint: row(endpoint_l, rw),
        done,
        inner_h,
        max_scroll,
    }
}

fn hit(p: Vec2, r: Rect) -> bool {
    input::hit_rect(p, r.x, r.y, r.w, r.h)
}

fn label(r: Rect, t: &str) {
    text::ui_left(t, r.x, r.y - 15.0, 15, palette::MUTED);
}

fn chip(r: Rect, name: &str, value: &str) {
    text::ui_left(name, r.x, r.y - 15.0, 14, palette::MUTED);
    draw::rounded_rect(r.x, r.y, r.w, r.h, 12.0, palette::CARD);
    draw::rounded_rect(r.x, r.y, r.w, r.h, 12.0, Color::new(0.0, 0.0, 0.0, 0.03));
    text::ui_centered(value, r.x + r.w / 2.0, r.y + r.h / 2.0, 20, palette::INK);
}

fn field(r: Rect, value: &str, focused: bool, placeholder: &str) {
    let ring = if focused { palette::ACCENT } else { palette::FIELD_BORDER };
    draw::rounded_rect(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, 12.0, ring);
    draw::rounded_rect(r.x, r.y, r.w, r.h, 12.0, palette::WHITE);
    let (txt, col) = if value.is_empty() {
        (placeholder, palette::MUTED)
    } else {
        (value, palette::INK)
    };
    text::ui_left(txt, r.x + 14.0, r.y + r.h / 2.0, 16, col);
}

fn button(r: Rect, t: &str, fill: Color, fg: Color) {
    draw::rounded_rect(r.x, r.y, r.w, r.h, 14.0, fill);
    text::ui_centered(t, r.x + r.w / 2.0, r.y + r.h / 2.0, 18, fg);
}

fn draw_mastery(r: Rect, m: &Mastery) {
    let x = r.x;
    text::ui_left(
        &format!("mastered {}  strong {}  learning {}  new {}", m.mastered, m.strong, m.learning, m.new),
        x,
        r.y + 11.0,
        15,
        palette::MUTED,
    );
    let dot = 14.0;
    let gap = 6.0;
    let per_row = 13;
    let y0 = r.y + 36.0;
    for (i, (_c, b)) in m.boxes.iter().enumerate() {
        let col = i % per_row;
        let rrow = i / per_row;
        let cx = x + col as f32 * (dot + gap) + dot / 2.0;
        let cy = y0 + rrow as f32 * (dot + gap) + dot / 2.0;
        draw_circle(cx, cy, dot / 2.0, palette::MASTERY[(*b as usize).min(4)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Frame, Insets};

    fn frame(w: f32, h: f32) -> Frame {
        Frame::new(w, h, Insets::default())
    }

    // label width height — the golden matrix, incl. the short phone-landscape.
    const SIZES: [(f32, f32); 3] = [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)];

    /// The pinned `done` button sits below the scroll viewport and inside the
    /// card on every device — the regression that overlapped it with the
    /// generate/clear/start-over controls on short phones.
    #[test]
    fn done_is_pinned_below_the_body() {
        for game in ["patterns", "phonics"] {
            for (w, h) in SIZES {
                let l = layout(&frame(w, h), game, 0.0);
                let view_bottom = l.view.y + l.view.h;
                assert!(
                    l.done.y >= view_bottom - 0.5,
                    "{game} {w}x{h}: done.y {} overlaps body (view bottom {view_bottom})",
                    l.done.y
                );
                assert!(
                    l.done.y + l.done.h <= l.card.y + l.card.h + 0.5,
                    "{game} {w}x{h}: done spills past the card bottom"
                );
                assert!(l.view.y > l.card.y, "{game} {w}x{h}: title band has no room");
            }
        }
    }

    /// A short phone-landscape card can't show the whole form, so it scrolls;
    /// at full scroll the last field (endpoint) is fully within the viewport.
    #[test]
    fn short_phone_scrolls_to_reach_the_end() {
        let small = frame(844.0, 390.0);
        let l0 = layout(&small, "patterns", 0.0);
        assert!(l0.max_scroll > 0.0, "patterns should scroll on a short phone");

        let l = layout(&small, "patterns", l0.max_scroll);
        let view_bottom = l.view.y + l.view.h;
        assert!(
            l.endpoint.y + l.endpoint.h <= view_bottom + 0.5,
            "endpoint unreachable at max scroll: {} vs {view_bottom}",
            l.endpoint.y + l.endpoint.h
        );
        assert!(l.endpoint.y >= l.view.y - 0.5, "endpoint above viewport at max scroll");
        // Scroll is clamped: asking for more than max doesn't push past the end.
        let over = layout(&small, "patterns", l0.max_scroll + 999.0);
        assert!((over.endpoint.y - l.endpoint.y).abs() < 0.5, "scroll not clamped to max");
    }

    /// Tablets are tall enough for the full form, so the card fits its content
    /// and never scrolls.
    #[test]
    fn tablets_fit_without_scrolling() {
        for game in ["patterns", "phonics"] {
            for (w, h) in [(1194.0, 834.0), (834.0, 1194.0)] {
                let l = layout(&frame(w, h), game, 0.0);
                assert_eq!(l.max_scroll, 0.0, "{game} {w}x{h} should fit without scroll");
            }
        }
    }
}
