//! Phonics: parent-graded Leitner flashcards. The growing ROYGBIV rainbow is
//! the progress meter (no numeric score). Tap ✓ (got it) / ✗ (missed); a miss
//! reveals the canonical exemplar before advancing. Logic lives in
//! `fountouki_core::srs`; this is the rendering + interaction shell.
use crate::{
    draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
    text,
};
use fountouki_core::{
    deck,
    rng::Mulberry32,
    srs::{self, PhonicsState},
    storage::ns_key,
};
use macroquad::prelude::*;
use nanoserde::SerJson;

/// Stripes in the rainbow = stars needed to complete a session.
const GOAL: u32 = 7;
const HOP_DUR: f32 = 0.45;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Card,
    Miss,
    Done,
}

pub struct PhonicsScene {
    db: Db,
    state: PhonicsState,
    rng: Mulberry32,
    queue: Vec<char>,
    qi: usize,
    last: Option<char>,
    pub stars: u32,
    streak: u32,
    phase: Phase,
    reveal: Option<deck::Exemplar>,
    hop_time: f32,
}

impl PhonicsScene {
    pub fn new(db: Db, seed: u32, now: i64) -> PhonicsScene {
        let key = ns_key("phonics", "state");
        let mut state = db
            .get(&key)
            .and_then(|raw| srs::validate(&raw))
            .unwrap_or_else(srs::empty_state);
        srs::ensure_letters(&mut state, now);
        let mut rng = Mulberry32::new(seed);
        let mut queue = srs::build_queue(&state, now, &mut rng);
        srs::avoid_repeat(&mut queue, None);
        PhonicsScene {
            db,
            state,
            rng,
            queue,
            qi: 0,
            last: None,
            stars: 0,
            streak: 0,
            phase: Phase::Card,
            reveal: None,
            hop_time: 99.0,
        }
    }

    fn current(&self) -> char {
        self.queue.get(self.qi).copied().unwrap_or('a')
    }

    fn save(&self) {
        self.db
            .set(&ns_key("phonics", "state"), &self.state.serialize_json());
    }

    fn advance(&mut self, now: i64) {
        self.last = Some(self.current());
        self.qi += 1;
        if self.qi >= self.queue.len() {
            self.queue = srs::build_queue(&self.state, now, &mut self.rng);
            srs::avoid_repeat(&mut self.queue, self.last);
            self.qi = 0;
        }
        self.reveal = None;
        self.phase = Phase::Card;
        self.hop_time = 99.0;
    }

    fn on_got(&mut self, ctx: &Ctx) {
        let c = self.current();
        srs::grade_got_it(&mut self.state, c, ctx.now);
        self.stars = (self.stars + 1).min(GOAL);
        self.streak += 1;
        ctx.audio.correct(self.streak);
        self.hop_time = 0.0;
        self.save();
        if self.stars >= GOAL {
            self.phase = Phase::Done;
        } else {
            self.advance(ctx.now);
        }
    }

    fn on_miss(&mut self, ctx: &Ctx) {
        let c = self.current();
        srs::grade_missed(&mut self.state, c, ctx.now);
        self.streak = 0;
        ctx.audio.incorrect();
        self.reveal = deck::exemplar(c);
        self.phase = Phase::Miss;
        self.save();
    }

    fn toggle_mute(&self, ctx: &Ctx) {
        let muted = !ctx.audio.muted();
        ctx.audio.set_muted(muted);
        let mut s = {
            let kv = self.db.borrow_kv();
            fountouki_core::settings::load_shared(&**kv)
        };
        s.muted = muted;
        let mut kv = self.db.borrow_kv_mut();
        fountouki_core::settings::save_shared(&mut **kv, &s);
    }

    // Test hooks (used by --playtest).
    pub(crate) fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }
    pub(crate) fn got_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f).got.0
    }
}

impl Scene for PhonicsScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.hop_time += ctx.dt;
        let p = plan(&ctx.frame);
        let pt = ctx.pointer;
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
            self.toggle_mute(ctx);
            return Nav::Stay;
        }
        match self.phase {
            Phase::Card => {
                if input::hit_circle(pt.pos, p.got.0.x, p.got.0.y, p.got.1) {
                    self.on_got(ctx);
                } else if input::hit_circle(pt.pos, p.miss.0.x, p.miss.0.y, p.miss.1) {
                    self.on_miss(ctx);
                }
            }
            Phase::Miss => {
                if input::hit_circle(pt.pos, p.advance.0.x, p.advance.0.y, p.advance.1) {
                    self.advance(ctx.now);
                }
            }
            Phase::Done => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        clear_background(palette::BG);
        let p = plan(&ctx.frame);

        // Topbar chrome.
        draw::circle_btn(p.home.0.x, p.home.0.y, p.home.1, palette::CARD);
        draw::chevron_left(p.home.0.x, p.home.0.y, p.home.1 * 0.9, palette::INK);
        draw::circle_btn(p.mute.0.x, p.mute.0.y, p.mute.1, palette::CARD);
        draw::speaker(p.mute.0.x, p.mute.0.y, p.mute.1 * 0.9, palette::INK, ctx.audio.muted());

        // Rainbow (filled = stars). When done, show the full arc.
        let filled = if self.phase == Phase::Done { GOAL } else { self.stars };
        draw::rainbow(p.rb_cx, p.rb_horizon, p.rb_scale, p.rb_stroke, filled as usize);

        // Card.
        let miss_tint = if self.phase == Phase::Miss {
            palette::hex(0xfff6ef)
        } else {
            palette::CARD
        };
        draw::card(p.card.x, p.card.y, p.card.w, p.card.h, miss_tint);

        let cx = p.card.x + p.card.w / 2.0;
        match self.phase {
            Phase::Card | Phase::Done => {
                let amp = p.card.h * 0.06;
                let prog = (self.hop_time / HOP_DUR).clamp(0.0, 1.0);
                let off = -amp * 4.0 * prog * (1.0 - prog);
                let glyph = self.current().to_string();
                let cy = p.card.y + p.card.h * 0.52 + off;
                text::draw_centered(&glyph, cx, cy, p.letter_size, &ctx.fonts.cursive, palette::INK);
            }
            Phase::Miss => {
                let glyph = self.current().to_string();
                text::draw_centered(
                    &glyph,
                    cx,
                    p.card.y + p.card.h * 0.34,
                    (p.letter_size as f32 * 0.62) as u16,
                    &ctx.fonts.cursive,
                    palette::MUTED,
                );
                // Picture placeholder (drawn vector art lands in the visual pass).
                draw_circle(cx, p.card.y + p.card.h * 0.6, p.card.h * 0.12, palette::ACCENT_SOFT);
                if let Some(ex) = &self.reveal {
                    text::draw_centered(
                        ex.word,
                        cx,
                        p.card.y + p.card.h * 0.84,
                        (p.card.h * 0.1) as u16,
                        &ctx.fonts.cursive,
                        palette::MUTED,
                    );
                }
            }
        }

        // Action buttons.
        match self.phase {
            Phase::Card => {
                draw::circle_btn(p.miss.0.x, p.miss.0.y, p.miss.1, palette::CARD);
                draw::mark_cross(p.miss.0.x, p.miss.0.y, p.miss.1, palette::MUTED);
                draw::circle_btn(p.got.0.x, p.got.0.y, p.got.1, palette::OK);
                draw::mark_check(p.got.0.x, p.got.0.y, p.got.1, palette::OK_STRONG);
            }
            Phase::Miss => {
                draw::circle_btn(p.advance.0.x, p.advance.0.y, p.advance.1, palette::ACCENT);
                draw::mark_arrow(p.advance.0.x, p.advance.0.y, p.advance.1, palette::WHITE);
            }
            Phase::Done => {
                text::draw_centered(
                    "yay!",
                    cx,
                    p.advance.0.y,
                    (p.card.h * 0.16) as u16,
                    &ctx.fonts.cursive,
                    palette::OK_STRONG,
                );
            }
        }
    }
}

struct PLayout {
    home: (Vec2, f32),
    mute: (Vec2, f32),
    card: Rect,
    letter_size: u16,
    rb_cx: f32,
    rb_horizon: f32,
    rb_scale: f32,
    rb_stroke: f32,
    miss: (Vec2, f32),
    got: (Vec2, f32),
    advance: (Vec2, f32),
}

fn plan(f: &crate::layout::Frame) -> PLayout {
    let cx = f.w / 2.0;
    let card_w = (f.w * 0.34).clamp(300.0, 460.0);
    let card_h = (f.h * 0.46).clamp(260.0, 430.0);
    let card_y = f.h * 0.49 - card_h / 2.0;
    let card = Rect::new(cx - card_w / 2.0, card_y, card_w, card_h);

    let tb = f.topbar();
    let ir = f.icon_btn() / 2.0;
    let got_r = (f.w * 0.045).clamp(40.0, 54.0);
    let miss_r = (f.w * 0.033).clamp(26.0, 35.0);
    let by = card.y + card.h + (f.h - (card.y + card.h)) * 0.42;
    let slot = got_r * 2.0;
    let gap = 34.0;
    let total = 2.0 * slot + gap;
    let x0 = cx - total / 2.0;

    PLayout {
        home: (vec2(tb.x + ir, tb.y + ir), ir),
        mute: (vec2(tb.x + tb.w - ir, tb.y + ir), ir),
        card,
        letter_size: (card_h * 0.6) as u16,
        rb_cx: cx,
        rb_horizon: card_y - 16.0,
        rb_scale: card_w / 240.0 * 1.45,
        rb_stroke: (10.0 * (card_w / 240.0 * 1.45)).max(8.0),
        miss: (vec2(x0 + slot / 2.0, by), miss_r),
        got: (vec2(x0 + slot + gap + slot / 2.0, by), got_r),
        advance: (vec2(cx, by), got_r),
    }
}
