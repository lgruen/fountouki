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
    frog_hop: f32,
    confetti: crate::confetti::Confetti,
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
            frog_hop: 99.0,
            confetti: crate::confetti::Confetti::new(seed ^ 0x00c0_ffee),
        }
    }

    fn restart_session(&mut self, now: i64) {
        self.stars = 0;
        self.streak = 0;
        self.phase = Phase::Card;
        self.frog_hop = 99.0;
        self.hop_time = 99.0;
        self.queue = srs::build_queue(&self.state, now, &mut self.rng);
        srs::avoid_repeat(&mut self.queue, self.last);
        self.qi = 0;
    }

    fn update_done(&mut self, ctx: &Ctx) -> Nav {
        let (frog_c, fr, replay, home_b, br, _gy) = done_layout(&ctx.frame);
        let pt = ctx.pointer;
        if pt.tapped() {
            if input::hit_circle(pt.pos, replay.x, replay.y, br) {
                self.restart_session(ctx.now);
            } else if input::hit_circle(pt.pos, home_b.x, home_b.y, br) {
                return Nav::Home;
            } else if input::hit_circle(pt.pos, frog_c.x, frog_c.y, fr) {
                self.frog_hop = 0.0;
                ctx.audio.frog();
            }
        }
        Nav::Stay
    }

    fn draw_done(&self, ctx: &Ctx) {
        let f = &ctx.frame;
        let (frog_c, fr, replay, home_b, br, gy) = done_layout(f);
        draw::vgradient(0.0, 0.0, f.w, gy, palette::SKY_TOP, palette::SKY_BOT);
        draw::sun(f.w * 0.17, gy * 0.34, f.vmin(0.07).max(40.0));
        let scale = 0.72 * f.w / 169.4;
        draw::rainbow(f.w / 2.0, gy * 0.95, scale, (14.0 * scale).max(10.0), 7);
        draw::vgradient(0.0, gy, f.w, f.h - gy, palette::GROUND_TOP, palette::GROUND_BOT);
        draw_line(0.0, gy, f.w, gy, 3.0, palette::hex(0x2f7d2f));
        draw::plant(f.w * 0.28, gy, f.vmin(0.06));
        draw::plant(f.w * 0.74, gy, f.vmin(0.05));
        let hop = if self.frog_hop < 0.5 {
            -(fr * 0.5) * ((self.frog_hop / 0.5) * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        draw::frog(frog_c.x, frog_c.y, fr, palette::RAINBOW[3], hop);
        let white = Color::new(1.0, 1.0, 1.0, 0.94);
        draw::circle_btn(replay.x, replay.y, br, white);
        draw::replay_icon(replay.x, replay.y, br, palette::INK);
        draw::circle_btn(home_b.x, home_b.y, br, white);
        draw::house_icon(home_b.x, home_b.y, br, palette::INK);
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
        let p = plan(&ctx.frame);
        self.confetti.burst(vec2(p.rb_cx, p.card.y), 70, p.card.w / 3.0);
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
        crate::store::persist_mute(&self.db, muted);
    }

    // Test hooks (used by --playtest).
    pub(crate) fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }
    pub(crate) fn got_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f).got.0
    }
    pub(crate) fn miss_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f).miss.0
    }
}

impl Scene for PhonicsScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.hop_time += ctx.dt;
        self.confetti.update(ctx.dt);
        self.frog_hop += ctx.dt;
        if self.phase == Phase::Done {
            return self.update_done(ctx);
        }
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
        if self.phase == Phase::Done {
            self.draw_done(ctx);
            self.confetti.draw();
            return;
        }
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
                    palette::INK,
                );
                if let Some(ex) = &self.reveal {
                    if let Some(tex) = crate::emoji::texture(ex.emoji) {
                        let s = p.card.h * 0.32;
                        draw_texture_ex(
                            &tex,
                            cx - s / 2.0,
                            p.card.y + p.card.h * 0.6 - s / 2.0,
                            WHITE,
                            DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() },
                        );
                    } else {
                        draw_circle(cx, p.card.y + p.card.h * 0.6, p.card.h * 0.12, palette::ACCENT_SOFT);
                    }
                    text::draw_centered(
                        ex.word,
                        cx,
                        p.card.y + p.card.h * 0.84,
                        (p.card.h * 0.11) as u16,
                        &ctx.fonts.cursive,
                        palette::INK,
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

        self.confetti.draw();
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

/// Done-scene geometry shared by update + draw.
/// Returns (frog_center, frog_radius, replay_btn, home_btn, btn_radius, ground_y).
fn done_layout(f: &crate::layout::Frame) -> (Vec2, f32, Vec2, Vec2, f32, f32) {
    let ground = if f.is_portrait() { 0.40 } else { 0.30 };
    let gy = f.h * (1.0 - ground);
    let fr = f.vmin(0.11).clamp(58.0, 140.0);
    let frog_c = vec2(f.w / 2.0, gy - fr * 0.78);
    let br = f.icon_btn() / 2.0 * 1.2;
    let m = 30.0 + f.safe.bottom.max(0.0);
    let replay = vec2(f.safe.left + 30.0 + br, f.h - m - br);
    let home_b = vec2(f.w - (f.safe.right + 30.0 + br), f.h - m - br);
    (frog_c, fr, replay, home_b, br, gy)
}

fn plan(f: &crate::layout::Frame) -> PLayout {
    let cx = f.w / 2.0;
    let tb = f.topbar();
    let ir = f.icon_btn() / 2.0;
    let phone = f.is_phone();

    // On short (phone-landscape) viewports, pack from the bottom up so the
    // action row never clips and the card clears the topbar + rainbow.
    let (card_w, card_h, card_y, got_r, miss_r, by) = if phone {
        let got_r = (f.h * 0.12).clamp(28.0, 52.0);
        let miss_r = got_r * 0.66;
        let by = f.h - f.safe.bottom.max(8.0) - got_r - 6.0;
        let card_w = (f.w * 0.26).clamp(190.0, 360.0);
        let top = tb.y + tb.h;
        let card_bottom = by - got_r - 12.0;
        let card_h = (card_bottom - top - 56.0).clamp(110.0, 230.0); // 56px reserves the rainbow
        (card_w, card_h, card_bottom - card_h, got_r, miss_r, by)
    } else {
        let card_w = (f.w * 0.34).clamp(300.0, 460.0);
        let card_h = (f.h * 0.46).clamp(260.0, 430.0);
        let card_y = f.h * 0.49 - card_h / 2.0;
        let got_r = (f.w * 0.045).clamp(40.0, 54.0);
        let miss_r = (f.w * 0.033).clamp(26.0, 35.0);
        let by = card_y + card_h + (f.h - (card_y + card_h)) * 0.42;
        (card_w, card_h, card_y, got_r, miss_r, by)
    };

    let card = Rect::new(cx - card_w / 2.0, card_y, card_w, card_h);
    let slot = got_r * 2.0;
    let gap = if phone { 22.0 } else { 34.0 };
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
