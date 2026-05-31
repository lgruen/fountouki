//! Patterns: "what comes next?" (next mode) over a repeating sequence. Pick the
//! item that fills the pink `?` slot. Errorless (wrong answers shake + let you
//! retry); monotonic stars + level pips. Round generation lives in
//! `fountouki_core::patterns`; this is the rendering + interaction shell.
//!
//! Unit mode (select the repeating piece) is tracked separately — this builds
//! `next` mode first; unit mode falls back to next for now.
use crate::{
    draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
    text,
};
use fountouki_core::{
    patterns::{generate_round, Difficulty, GameMode, Round, MAX_LEVEL},
    rng::Mulberry32,
    settings::load_patterns,
    themes::{self, Item, Shape, ThemeChoice},
};
use macroquad::prelude::*;

const CORRECT_PER_LEVEL: u32 = 4;
const ADVANCE_DELAY: f32 = 0.85;
const RETRY_DELAY: f32 = 0.55;

pub struct PatternsScene {
    db: Db,
    rng: Mulberry32,
    theme_choice: ThemeChoice,
    difficulty: Difficulty,
    mode: GameMode,
    show_hint: bool,
    pub level: u32,
    pub stars: u32,
    streak: u32,
    correct_count: u32,
    round: Round,
    selected: Option<usize>,
    result: Option<bool>, // Some(true)=correct, Some(false)=wrong
    fb_time: f32,
    advance_in: Option<f32>,
    /// Unit mode: the currently-selected contiguous cell range [start, end).
    sel: Option<(usize, usize)>,
    confetti: crate::confetti::Confetti,
}

impl PatternsScene {
    pub fn new(db: Db, seed: u32, _now: i64) -> PatternsScene {
        let ps = {
            let kv = db.borrow_kv();
            load_patterns(&**kv)
        };
        let theme_choice = ThemeChoice::from_str(&ps.theme_choice).unwrap_or(ThemeChoice::Mix);
        let difficulty = Difficulty::from_str(&ps.difficulty).unwrap_or(Difficulty::Auto);
        let mode = GameMode::from_str(&ps.mode).unwrap_or(GameMode::Next);
        let mut rng = Mulberry32::new(seed);
        let round = gen(1, theme_choice, mode, difficulty, &mut rng);
        PatternsScene {
            db,
            rng,
            theme_choice,
            difficulty,
            mode,
            show_hint: ps.show_hint,
            level: 1,
            stars: 0,
            streak: 0,
            correct_count: 0,
            round,
            selected: None,
            result: None,
            fb_time: 0.0,
            advance_in: None,
            sel: None,
            confetti: crate::confetti::Confetti::new(seed ^ 0x00c0_ffee),
        }
    }

    fn next_round(&mut self) {
        self.round = gen(self.level, self.theme_choice, self.mode, self.difficulty, &mut self.rng);
        self.selected = None;
        self.result = None;
        self.fb_time = 0.0;
        self.advance_in = None;
        self.sel = None;
    }

    fn on_choice(&mut self, i: usize, ctx: &Ctx) {
        if self.advance_in.is_some() {
            return; // locked while a correct answer animates out
        }
        let correct = self.round.choices[i].id() == self.round.answer.id();
        self.selected = Some(i);
        self.fb_time = 0.0;
        if correct {
            self.score_correct(ctx);
        } else {
            self.result = Some(false);
            self.streak = 0;
            ctx.audio.incorrect();
        }
    }

    fn score_correct(&mut self, ctx: &Ctx) {
        let p = plan(&ctx.frame, self.round.choices.len(), self.round.visible.len() + 1, self.mode);
        // Burst from the thing the kid just touched (the picked choice, or the
        // unit submit FAB) so the celebration reads as a reaction to the tap —
        // chips fan upward from there across to the sequence above.
        let (origin, spread) = match (self.mode, self.selected) {
            (GameMode::Next, Some(i)) => {
                let r = p.choices[i];
                (vec2(r.x + r.w / 2.0, r.y + r.h / 2.0), r.w / 2.0)
            }
            _ => {
                let fab = unit_fab(&ctx.frame);
                (fab.0, fab.1)
            }
        };
        self.confetti.burst(origin, 80, spread);
        self.stars += 1;
        self.streak += 1;
        self.correct_count += 1;
        ctx.audio.correct(self.streak);
        if self.correct_count % CORRECT_PER_LEVEL == 0 && self.level < MAX_LEVEL {
            self.level += 1;
            ctx.audio.level_up();
        }
        self.result = Some(true);
        self.advance_in = Some(ADVANCE_DELAY);
    }

    /// Unit mode: tap cell `i` to start / extend / shrink the contiguous range.
    fn unit_tap(&mut self, i: usize) {
        if self.advance_in.is_some() {
            return;
        }
        let n = self.round.visible.len();
        self.sel = Some(match self.sel {
            None => (i, i + 1),
            Some((s, e)) => {
                if i == e && e < n {
                    (s, e + 1) // extend right
                } else if i + 1 == s {
                    (s - 1, e) // extend left
                } else if i + 1 == e && e - s > 1 {
                    (s, e - 1) // shrink right
                } else if i == s && e - s > 1 {
                    (s + 1, e) // shrink left
                } else {
                    return; // non-adjacent: ignore
                }
            }
        });
    }

    /// Unit mode: check the selection length against the period.
    fn unit_submit(&mut self, ctx: &Ctx) {
        if self.advance_in.is_some() {
            return;
        }
        if let Some((s, e)) = self.sel {
            if e - s == self.round.unit_len {
                self.score_correct(ctx);
            } else {
                self.streak = 0;
                self.result = Some(false);
                self.fb_time = 0.0;
                self.sel = None;
                ctx.audio.incorrect();
            }
        }
    }

    // Test hooks (used by --playtest).
    pub(crate) fn round(&self) -> &Round {
        &self.round
    }
    pub(crate) fn correct_index(&self) -> usize {
        self.round
            .choices
            .iter()
            .position(|c| c.id() == self.round.answer.id())
            .unwrap_or(0)
    }
    pub(crate) fn choice_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        let p = plan(f, self.round.choices.len(), self.round.visible.len() + 1, self.mode);
        let r = p.choices[i];
        vec2(r.x + r.w / 2.0, r.y + r.h / 2.0)
    }
    pub(crate) fn cell_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        let p = plan(f, self.round.choices.len(), self.round.visible.len() + 1, self.mode);
        let (x, y) = p.cell_center(i);
        vec2(x, y)
    }
}

fn unit_fab(f: &crate::layout::Frame) -> (Vec2, f32) {
    (vec2(f.w / 2.0, f.h * 0.78), (f.w * 0.06).clamp(60.0, 90.0))
}

fn gen(level: u32, choice: ThemeChoice, mode: GameMode, diff: Difficulty, rng: &mut Mulberry32) -> Round {
    let theme = themes::resolve_theme(choice, rng);
    generate_round(level, theme, mode, diff, rng)
}

impl Scene for PatternsScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.fb_time += ctx.dt;
        self.confetti.update(ctx.dt);
        if let Some(t) = self.advance_in {
            let t = t - ctx.dt;
            if t <= 0.0 {
                self.next_round();
            } else {
                self.advance_in = Some(t);
            }
        } else if self.result == Some(false) && self.fb_time > RETRY_DELAY {
            // Errorless: clear the wrong mark and let them try again.
            self.result = None;
            self.selected = None;
        }

        let pt = ctx.pointer;
        let p = plan(&ctx.frame, self.round.choices.len(), self.round.visible.len() + 1, self.mode);
        if pt.long_fired && input::hit_circle(pt.pos, p.home.0.x, p.home.0.y, p.home.1) {
            return Nav::OpenParent;
        }
        if !pt.tapped() {
            return Nav::Stay;
        }
        if input::hit_circle(pt.pos, p.home.0.x, p.home.0.y, p.home.1) {
            return Nav::Home;
        }
        if input::hit_circle(pt.pos, p.mute.0.x, p.mute.0.y, p.mute.1) {
            let m = !ctx.audio.muted();
            ctx.audio.set_muted(m);
            crate::store::persist_mute(&self.db, m);
            return Nav::Stay;
        }
        match self.mode {
            GameMode::Next => {
                for (i, r) in p.choices.iter().enumerate() {
                    if input::hit_rect(pt.pos, r.x, r.y, r.w, r.h) {
                        self.on_choice(i, ctx);
                        break;
                    }
                }
            }
            GameMode::Unit => {
                let fab = unit_fab(&ctx.frame);
                if self.sel.is_some() && input::hit_circle(pt.pos, fab.0.x, fab.0.y, fab.1) {
                    self.unit_submit(ctx);
                } else {
                    for i in 0..self.round.visible.len() {
                        let (cx, cy) = p.cell_center(i);
                        if input::hit_rect(pt.pos, cx - p.cell / 2.0, cy - p.cell / 2.0, p.cell, p.cell) {
                            self.unit_tap(i);
                            break;
                        }
                    }
                }
            }
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        clear_background(palette::BG);
        let p = plan(&ctx.frame, self.round.choices.len(), self.round.visible.len() + 1, self.mode);

        // Topbar: home, stars + level pips, mute.
        draw::circle_btn(p.home.0.x, p.home.0.y, p.home.1, palette::CARD);
        draw::chevron_left(p.home.0.x, p.home.0.y, p.home.1 * 0.9, palette::INK);
        draw::circle_btn(p.mute.0.x, p.mute.0.y, p.mute.1, palette::CARD);
        draw::speaker(p.mute.0.x, p.mute.0.y, p.mute.1 * 0.9, palette::INK, ctx.audio.muted());
        draw_hud(&p, self.stars, self.level);

        // Sequence bar.
        draw::card(p.seq.x, p.seq.y, p.seq.w, p.seq.h, palette::CARD);
        let pulse = 1.0 + 0.06 * crate::anim::pulse(ctx.time, 1.6).max(0.0);
        for (i, item) in self.round.visible.iter().enumerate() {
            let (cx, cy) = p.cell_center(i);
            let selected = matches!(self.sel, Some((s, e)) if i >= s && i < e);
            if selected {
                draw::rounded_rect(
                    cx - p.cell / 2.0 - 4.0, cy - p.cell / 2.0 - 4.0,
                    p.cell + 8.0, p.cell + 8.0, p.cell * 0.2, palette::ACCENT,
                );
                draw_cell(cx, cy, p.cell, palette::ACCENT_SOFT);
            } else {
                draw_cell(cx, cy, p.cell, palette::WHITE);
            }
            draw_item(item, cx, cy, p.cell * 0.78, ctx);
        }

        match self.mode {
            GameMode::Next => {
                // The pink `?` slot to fill.
                let (sx, sy) = p.cell_center(self.round.visible.len());
                draw_cell(sx, sy, p.cell * pulse, palette::ACCENT_SOFT);
                text::draw_centered("?", sx, sy, (p.cell * 0.7) as u16, &ctx.fonts.cursive, palette::ACCENT);
                // Choice buttons.
                for (i, r) in p.choices.iter().enumerate() {
                    let mut fill = palette::CARD;
                    let mut dy = 0.0;
                    if self.selected == Some(i) {
                        match self.result {
                            Some(true) => {
                                fill = palette::OK;
                                let prog = (self.fb_time / 0.4).clamp(0.0, 1.0);
                                dy = -10.0 * crate::anim::back_out(prog).min(1.2) * (1.0 - prog);
                            }
                            Some(false) => {
                                fill = palette::BAD;
                                dy = (self.fb_time * 40.0).sin() * 6.0 * (1.0 - (self.fb_time / RETRY_DELAY)).max(0.0);
                            }
                            None => {}
                        }
                    }
                    draw::card(r.x, r.y + dy, r.w, r.h, fill);
                    draw_item(&self.round.choices[i], r.x + r.w / 2.0, r.y + r.h / 2.0 + dy, r.h * 0.5, ctx);
                }
            }
            GameMode::Unit => {
                // Submit FAB appears once a selection exists.
                if self.sel.is_some() {
                    let fab = unit_fab(&ctx.frame);
                    let s = 1.0 + 0.05 * crate::anim::pulse(ctx.time, 1.4).max(0.0);
                    draw::circle_btn(fab.0.x, fab.0.y, fab.1 * s, palette::OK);
                    draw::mark_check(fab.0.x, fab.0.y, fab.1, palette::OK_STRONG);
                }
            }
        }

        self.confetti.draw();
    }
}

// --- rendering helpers ------------------------------------------------------

fn draw_cell(cx: f32, cy: f32, size: f32, fill: Color) {
    let r = (size * 0.18).min(18.0);
    // subtle shadow so a white cell reads against the warm-white bar
    draw::rounded_rect(cx - size / 2.0, cy - size / 2.0 + 3.0, size, size, r, Color::new(0.17, 0.17, 0.2, 0.07));
    draw::rounded_rect(cx - size / 2.0, cy - size / 2.0, size, size, r, fill);
}

fn draw_item(item: &Item, cx: f32, cy: f32, sz: f32, ctx: &Ctx) {
    match item {
        Item::Glyph(g) => {
            if let Some(tex) = crate::emoji::texture(g) {
                let s = sz * 0.96;
                draw_texture_ex(
                    &tex,
                    cx - s / 2.0,
                    cy - s / 2.0,
                    WHITE,
                    DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() },
                );
            } else if g.chars().all(|c| c.is_ascii_alphanumeric()) {
                text::draw_centered(g, cx, cy, (sz * 0.95) as u16, &ctx.fonts.cursive, palette::INK);
            } else {
                draw::rounded_rect(cx - sz * 0.4, cy - sz * 0.4, sz * 0.8, sz * 0.8, sz * 0.18, palette::ACCENT_SOFT);
            }
        }
        Item::Shape { shape, .. } => draw_shape(cx, cy, sz, shape),
    }
}

fn draw_shape(cx: f32, cy: f32, sz: f32, shape: &Shape) {
    let color = palette::hex(shape.color);
    let r = sz / 2.0;
    if shape.radius == Some("50%") {
        draw_circle(cx, cy, r, color);
    } else if shape.clip.is_some() {
        // upward triangle
        draw_triangle(
            vec2(cx, cy - r),
            vec2(cx - r, cy + r),
            vec2(cx + r, cy + r),
            color,
        );
    } else {
        draw::rounded_rect(cx - r, cy - r, sz, sz, sz * 0.12, color);
    }
}

fn draw_hud(p: &PLayout, stars: u32, level: u32) {
    // stars pill
    let (hx, hy) = (p.hud.0.x, p.hud.0.y);
    draw::rounded_rect(hx, hy - 18.0, 96.0, 36.0, 18.0, palette::CARD);
    draw::star(hx + 22.0, hy, 11.0, palette::GOLD);
    text::ui_centered(&stars.to_string(), hx + 58.0, hy, 24, palette::INK);
    // level pips
    let py = hy + 0.0;
    let px0 = hx + 110.0;
    for i in 0..MAX_LEVEL as usize {
        let on = (i as u32) < level;
        let c = if on { palette::PIPS[i] } else { palette::PIP_EMPTY };
        draw_circle(px0 + i as f32 * 20.0, py, 7.0, c);
    }
}

// --- layout -----------------------------------------------------------------

struct PLayout {
    home: (Vec2, f32),
    mute: (Vec2, f32),
    hud: (Vec2, f32),
    seq: Rect,
    cell: f32,
    seq_n: usize, // visible + slot
    choices: Vec<Rect>,
}

impl PLayout {
    fn cell_center(&self, i: usize) -> (f32, f32) {
        let gap = self.cell * 0.16;
        let total = self.seq_n as f32 * self.cell + (self.seq_n as f32 - 1.0) * gap;
        let x0 = self.seq.x + self.seq.w / 2.0 - total / 2.0 + self.cell / 2.0;
        (x0 + i as f32 * (self.cell + gap), self.seq.y + self.seq.h / 2.0)
    }
}

fn plan(f: &crate::layout::Frame, n_choices: usize, seq_cells: usize, mode: GameMode) -> PLayout {
    let tb = f.topbar();
    let ir = f.icon_btn() / 2.0;
    let content = f.content();

    let seq_h = (f.h * 0.16).clamp(90.0, 150.0);
    let seq_w = (f.w * 0.8).clamp(300.0, 900.0);

    // Fit all sequence cells (incl. the `?` slot) across 92% of the bar.
    let n = seq_cells.max(1) as f32;
    let cell = ((seq_w * 0.92) / (n * 1.16)).clamp(28.0, 104.0).min(seq_h * 0.78);

    // Choices row (2- or 3-up grid) below the sequence.
    let cols = (if n_choices > 4 { 3 } else { 2 }).min(n_choices.max(1));
    let rows = (n_choices + cols - 1) / cols;
    let cw = (f.w * 0.2).clamp(140.0, 240.0);
    let ch = (f.h * 0.16).clamp(96.0, 180.0);
    let cgap = 20.0;
    let choices_h = rows as f32 * ch + (rows as f32 - 1.0) * cgap;

    // Place the sequence + the choices below it. In `next` mode we keep them as
    // one [sequence | gap | choices] group with a controlled gap, biased toward
    // the upper third of the play region (below the topbar, above the safe
    // bottom). This keeps the pattern in its familiar reading position while
    // pulling the choices up close to it — no big mid-screen void on short
    // phone-landscape. Unit mode keeps its own anchor (sequence high, FAB low).
    let (seq_y, gy0) = match mode {
        GameMode::Next => {
            let group_gap = (f.h * 0.07).clamp(36.0, 110.0);
            let group_h = seq_h + group_gap + choices_h;
            let region_top = tb.y + tb.h;
            let region_bot = content.y + content.h;
            let slack = (region_bot - region_top - group_h).max(0.0);
            let top = region_top + slack * 0.34;
            (top, top + seq_h + group_gap)
        }
        GameMode::Unit => (f.h * 0.30 - seq_h / 2.0, f.h * 0.62),
    };
    let seq = Rect::new(f.w / 2.0 - seq_w / 2.0, seq_y, seq_w, seq_h);

    let mut choices = Vec::new();
    for i in 0..n_choices {
        let r = i / cols;
        let c = i % cols;
        let row_n = if r == rows - 1 { n_choices - r * cols } else { cols };
        let row_w = row_n as f32 * cw + (row_n as f32 - 1.0) * cgap;
        let x0 = f.w / 2.0 - row_w / 2.0;
        choices.push(Rect::new(
            x0 + c as f32 * (cw + cgap),
            gy0 + r as f32 * (ch + cgap),
            cw,
            ch,
        ));
    }

    PLayout {
        home: (vec2(tb.x + ir, tb.y + ir), ir),
        mute: (vec2(tb.x + tb.w - ir, tb.y + ir), ir),
        hud: (vec2(tb.x + 2.2 * ir, tb.y + ir), ir),
        seq,
        cell,
        seq_n: seq_cells,
        choices,
    }
}
