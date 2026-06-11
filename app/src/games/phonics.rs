//! Phonics: parent-graded Leitner flashcards. The growing ROYGBIV rainbow is
//! the progress meter (no numeric score). Tap ✓ (got it) / ✗ (missed); a miss
//! reveals the canonical exemplar before advancing. Logic lives in
//! `fountouki_core::srs`; this is the rendering + interaction shell.
use crate::{
    chrome, draw, input,
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
    /// Time since the last frog tap (drives the reaction); large = idle.
    frog_t: f32,
    /// Which reaction is playing (cycles through `REACTIONS`).
    frog_kind: usize,
    /// Total frog taps this session (selects + cycles the reaction).
    frog_taps: u32,
    /// Seed for the done-scene garden — re-rolled each session so a fresh mix of
    /// plants "grows" at the payoff ("what grew this time?").
    garden_seed: u32,
    confetti: crate::confetti::Confetti,
    sync: crate::net::SyncClient,
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
        let sync = crate::net::SyncClient::new(db.clone(), "phonics");
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
            frog_t: 99.0,
            frog_kind: 0,
            frog_taps: 0,
            garden_seed: seed ^ 0x9e37_79b9,
            confetti: crate::confetti::Confetti::new(seed ^ 0x00c0_ffee),
            sync,
        }
    }

    fn restart_session(&mut self, now: i64) {
        self.stars = 0;
        self.streak = 0;
        self.phase = Phase::Card;
        self.frog_t = 99.0;
        self.frog_kind = 0;
        self.frog_taps = 0;
        self.hop_time = 99.0;
        // Re-roll the garden so replaying grows a fresh mix of plants.
        self.garden_seed = (self.rng.next_f64() * u32::MAX as f64) as u32 ^ 0x9e37_79b9;
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
                self.sync.flush();
                return Nav::Home;
            } else if input::hit_circle(pt.pos, frog_c.x, frog_c.y, fr) {
                // Cycle to the next reaction (does not escalate) + a sparkle.
                self.frog_taps += 1;
                self.frog_kind = (self.frog_taps as usize - 1) % REACTIONS.len();
                self.frog_t = 0.0;
                ctx.audio.frog();
                self.confetti.burst(vec2(frog_c.x, frog_c.y - fr * 0.95), 16, fr * 0.55);
            }
        }
        Nav::Stay
    }

    fn draw_done(&self, ctx: &Ctx) {
        let f = &ctx.frame;
        let (frog_c, fr, replay, home_b, br, gy) = done_layout(f);
        draw::vgradient(0.0, 0.0, f.w, gy, palette::SKY_TOP, palette::SKY_BOT);
        // Ambient drifting clouds (behind the sun + rainbow). They wrap across
        // the sky; deterministic in goldens since ctx.time is fixed in capture.
        let cloud_r = f.vmin(0.05).max(24.0);
        let span = f.w + cloud_r * 8.0;
        // (height as fraction of the sky band, scale mult, speed px/s, phase 0..1)
        for &(hy, sc, spd, ph) in &[
            (0.16f32, 1.15f32, 8.0f32, 0.05f32),
            (0.40, 0.78, 13.0, 0.33),
            (0.10, 0.95, 6.0, 0.56),
            (0.52, 0.66, 17.0, 0.74),
            (0.28, 1.0, 10.0, 0.88),
        ] {
            let x = (ctx.time * spd + ph * span).rem_euclid(span) - cloud_r * 4.0;
            draw::cloud(x, gy * hy, cloud_r * sc);
        }
        draw::sun(f.w * 0.17, gy * 0.34, f.vmin(0.07).max(40.0));
        let (rcx, rhoriz, rscale, rstroke) = done_rainbow(f, gy);
        draw::rainbow(rcx, rhoriz, rscale, rstroke, 7);
        draw::vgradient(0.0, gy, f.w, f.h - gy, palette::GROUND_TOP, palette::GROUND_BOT);
        draw_line(0.0, gy, f.w, gy, 3.0, palette::hex(0x2f7d2f));
        // The garden: a varied mix of vector plants that grows behind the frog.
        // Built from the per-session seed (deterministic) and drawn far→near so
        // foreground clumps overlap the back row; the frog (next) sits on top.
        let garden = build_garden(self.garden_seed, f, gy, replay, home_b, br);
        for g in &garden {
            if matches!(g.kind, GardenLayer::Grass) {
                draw::grass_tuft(g.pos.x, g.pos.y, g.size, palette::hex(0x47a64a), (ctx.time * 0.9 + g.phase).sin() * 0.5);
            }
        }
        for g in &garden {
            if let GardenLayer::Plant(kind) = g.kind {
                let sway = (ctx.time * 1.1 + g.phase).sin();
                draw::garden_plant(g.pos.x, g.pos.y, g.size, kind, g.color, sway);
            }
        }
        let rx = &REACTIONS[self.frog_kind];
        let pose = if self.frog_t < rx.dur {
            react_pose(rx, self.frog_t, fr)
        } else {
            idle_pose(ctx.time)
        };
        draw::frog(frog_c.x, frog_c.y, fr, palette::RAINBOW[3], pose);
        chrome::draw_corner_buttons(replay, home_b, br);
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
        // The companion frog celebrates every star — a different jump each time —
        // and a sparkle pops from the rainbow stripe that just filled in.
        self.frog_kind = (self.stars.saturating_sub(1) as usize) % REACTIONS.len();
        self.frog_t = 0.0;
        let stripe = self.stars.saturating_sub(1) as f32 / 6.0;
        let sag = (65.0 - 40.0 * stripe) * p.rb_scale;
        self.confetti.burst(vec2(p.rb_cx, p.rb_horizon - sag), 18, 30.0 * p.rb_scale);
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
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
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
    }

    // Test hooks (used by --playtest).
    pub(crate) fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }
    pub(crate) fn is_miss(&self) -> bool {
        self.phase == Phase::Miss
    }
    pub(crate) fn advance_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f).advance.0
    }
    pub(crate) fn got_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f).got.0
    }
    pub(crate) fn miss_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f).miss.0
    }
    pub(crate) fn frog_center(&self, f: &crate::layout::Frame) -> Vec2 {
        done_layout(f).0
    }
    pub(crate) fn frog_taps(&self) -> u32 {
        self.frog_taps
    }
    /// Force the current card to a specific letter (capture/playtest only) so a
    /// golden can show a chosen exemplar (e.g. the drawn igloo for 'i').
    pub(crate) fn debug_set_letter(&mut self, c: char) {
        self.queue = vec![c, c];
        self.qi = 0;
    }
}

impl Scene for PhonicsScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.hop_time += ctx.dt;
        self.confetti.update(ctx.dt);
        self.frog_t += ctx.dt;
        // Drive cross-device sync: send debounced pushes, and merge the remote
        // blob once the initial pull lands (non-yanking — just updates state).
        self.sync.drive(ctx.now);
        if let Some(remote) = self.sync.poll_pull() {
            if let Some(rstate) = srs::validate(&remote) {
                self.state = srs::merge(&self.state, &rstate, ctx.now);
                self.save();
            }
        }
        if self.phase == Phase::Done {
            return self.update_done(ctx);
        }
        let p = plan(&ctx.frame);
        let pt = ctx.pointer;
        match chrome::handle_topbar(&chrome::topbar(&ctx.frame), ctx, &self.db) {
            Some(chrome::TopbarAction::OpenParent) => return Nav::OpenParent,
            Some(chrome::TopbarAction::Home) => {
                self.sync.flush();
                return Nav::Home;
            }
            Some(chrome::TopbarAction::MuteToggled) => return Nav::Stay,
            None => {}
        }
        if !pt.tapped() {
            return Nav::Stay;
        }
        // The companion frog responds to a tap (ribbit + a jump) — pure joy,
        // no game effect, mirroring the done-scene frog.
        if input::hit_circle(pt.pos, p.frog.0.x, p.frog.0.y, p.frog.1 * 1.3) {
            self.frog_taps += 1;
            self.frog_kind = (self.frog_taps.saturating_sub(1) as usize) % REACTIONS.len();
            self.frog_t = 0.0;
            ctx.audio.frog();
            self.confetti.burst(vec2(p.frog.0.x, p.frog.0.y - p.frog.1 * 0.95), 10, p.frog.1 * 0.5);
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
        chrome::draw_topbar(&chrome::topbar(&ctx.frame), ctx);

        // Rainbow progress meter: a pale ghost of all 7 bands (so the goal is
        // visible from zero stars), filled over in color star by star.
        let filled = if self.phase == Phase::Done { GOAL } else { self.stars };
        draw::rainbow_ghost(p.rb_cx, p.rb_horizon, p.rb_scale, p.rb_stroke, palette::BG);
        draw::rainbow(p.rb_cx, p.rb_horizon, p.rb_scale, p.rb_stroke, filled as usize);

        // The companion frog at the rainbow's foot: calm idle between answers,
        // a celebratory jump on each star (or when tapped).
        let rx = &REACTIONS[self.frog_kind];
        let pose = if self.frog_t < rx.dur {
            react_pose(rx, self.frog_t, p.frog.1)
        } else {
            idle_pose(ctx.time)
        };
        draw::frog(p.frog.0.x, p.frog.0.y, p.frog.1, palette::RAINBOW[3], pose);

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
                    p.card.y + p.card.h * 0.24,
                    (p.letter_size as f32 * 0.58) as u16,
                    &ctx.fonts.cursive,
                    palette::INK,
                );
                // Picture only — no word label (distracting at this age), and
                // pushed well below the letter so the two never crowd.
                if let Some(ex) = &self.reveal {
                    let ecy = p.card.y + p.card.h * 0.72;
                    if ex.word == "igloo" {
                        // No igloo glyph exists — draw it as a vector.
                        draw::igloo(cx, ecy, p.card.h * 0.46);
                    } else if let Some(tex) = crate::emoji::texture(ex.emoji) {
                        let s = p.card.h * 0.34;
                        draw_texture_ex(
                            &tex,
                            cx - s / 2.0,
                            ecy - s / 2.0,
                            WHITE,
                            DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() },
                        );
                    } else {
                        draw_circle(cx, ecy, p.card.h * 0.12, palette::ACCENT_SOFT);
                    }
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
    card: Rect,
    letter_size: u16,
    rb_cx: f32,
    rb_horizon: f32,
    rb_scale: f32,
    rb_stroke: f32,
    miss: (Vec2, f32),
    got: (Vec2, f32),
    advance: (Vec2, f32),
    /// The in-play companion frog: perched at the left foot of the rainbow
    /// (center, radius). Calm idle; reacts on a correct answer or a tap.
    frog: (Vec2, f32),
}

/// Done-scene geometry shared by update + draw.
/// Returns (frog_center, frog_radius, replay_btn, home_btn, btn_radius, ground_y).
fn done_layout(f: &crate::layout::Frame) -> (Vec2, f32, Vec2, Vec2, f32, f32) {
    let ground = if f.is_portrait() { 0.40 } else { 0.30 };
    let gy = f.h * (1.0 - ground);
    let fr = f.vmin(0.11).clamp(58.0, 140.0);
    let frog_c = vec2(f.w / 2.0, gy - fr * 0.78);
    let (replay, home_b, br) = chrome::corner_buttons(f);
    (frog_c, fr, replay, home_b, br, gy)
}

/// Done/garden rainbow geometry (center_x, horizon_y, scale, stroke). The scale
/// is capped so the outer band's apex clears the top edge — on a short
/// phone-landscape the width-derived scale would otherwise run off the top.
fn done_rainbow(f: &crate::layout::Frame, gy: f32) -> (f32, f32, f32, f32) {
    let horizon = gy * 0.95;
    let desired = 0.72 * f.w / 169.4;
    // Outer band rises ~65*scale + stroke/2 (stroke = 14*scale) ≈ 72*scale.
    let fit = ((horizon - 12.0) / 72.0).max(0.2);
    let scale = desired.min(fit);
    (f.w / 2.0, horizon, scale, (14.0 * scale).max(10.0))
}

/// One drawn element of the done-scene garden (sorted far→near for overlap).
enum GardenLayer {
    Grass,
    Plant(draw::Plant),
}

struct GardenItem {
    pos: Vec2,
    size: f32,
    kind: GardenLayer,
    color: Color,
    phase: f32,
}

/// Vivid bloom colors the garden flowers draw from (shuffled per session).
const BLOOM: [u32; 8] =
    [0xff5d8f, 0xff4d6d, 0xff8c42, 0xffd23f, 0xb364e5, 0x6e72e7, 0x38b3e2, 0xff8cbe];

/// Lay out the done-scene garden deterministically from `seed`. Four foreground
/// "hero" plants frame the frog in fixed spots (predictable layout across
/// sessions), each a freshly-rolled species + color; a sparse back row and grass
/// tufts fill the meadow. The frog column (center) and the two corner buttons
/// (`replay`/`home`) stay clear, so nothing the kid taps is occluded.
fn build_garden(
    seed: u32,
    f: &crate::layout::Frame,
    gy: f32,
    replay: Vec2,
    home_b: Vec2,
    br: f32,
) -> Vec<GardenItem> {
    let mut rng = Mulberry32::new(seed);
    let gh = f.h - gy;
    let base = f.vmin(0.06);
    // Keep every plant base above the corner buttons so none is overlapped.
    let floor = replay.y.min(home_b.y) - br - 6.0;

    let mut species: Vec<draw::Plant> = draw::GARDEN_SPECIES.to_vec();
    rng.shuffle(&mut species);
    let mut blooms: Vec<u32> = BLOOM.to_vec();
    rng.shuffle(&mut blooms);

    let mut items: Vec<GardenItem> = Vec::new();
    let mut si = 0usize;
    let place = |rng: &mut Mulberry32, items: &mut Vec<GardenItem>, si: &mut usize, xf: f32, depth: f32, smul: f32| {
        let kind = species[*si % species.len()];
        let color = palette::hex(blooms[*si % blooms.len()]);
        *si += 1;
        let x = xf * f.w + rng.range(-0.015, 0.015) * f.w;
        let y = (gy + depth * gh * rng.range(0.9, 1.1)).min(floor);
        let size = base * smul * rng.range(0.92, 1.10);
        items.push(GardenItem { pos: vec2(x, y), size, kind: GardenLayer::Plant(kind), color, phase: rng.range(0.0, std::f32::consts::TAU) });
    };

    // Foreground heroes: two clumps flanking the frog (back→front within each).
    for &(xf, depth, smul) in &[(0.27f32, 0.30f32, 0.98f32), (0.13, 0.58, 1.30), (0.73, 0.30, 0.98), (0.87, 0.58, 1.30)] {
        place(&mut rng, &mut items, &mut si, xf, depth, smul);
    }
    // Sparse back row along the horizon (count varies per session).
    for &xf in &[0.05f32, 0.16, 0.34, 0.66, 0.84, 0.95] {
        if rng.next_f32() < 0.7 {
            place(&mut rng, &mut items, &mut si, xf, 0.05, 0.55);
        }
    }
    // Grass tufts scattered through the meadow.
    for _ in 0..8 {
        let x = rng.range(0.03, 0.97) * f.w;
        let y = (gy + rng.range(0.0, 0.55) * gh).min(floor);
        let size = base * rng.range(0.40, 0.75);
        items.push(GardenItem { pos: vec2(x, y), size, kind: GardenLayer::Grass, color: palette::WHITE, phase: rng.range(0.0, std::f32::consts::TAU) });
    }

    items.sort_by(|a, b| a.pos.y.total_cmp(&b.pos.y));
    items
}

/// One frog reaction — a *real jump* (the frog leaves the ground). Tapping the
/// frog cycles through these in order; it does not escalate. Heights are in frog
/// radii so the jump tracks the frog's size across devices.
struct Reaction {
    dur: f32,
    height: f32, // apex, in frog radii
    turns: f32,  // full spins, signed (a backflip is negative)
    tilt: f32,   // peak mid-air lean, radians
    squash: f32, // crouch-on-launch / thud-on-land depth
    tongue: bool,
}

const REACTIONS: [Reaction; 5] = [
    Reaction { dur: 0.55, height: 1.05, turns: 0.0, tilt: 0.0, squash: 0.20, tongue: false }, // hop
    Reaction { dur: 0.60, height: 1.10, turns: 0.0, tilt: -0.36, squash: 0.20, tongue: false }, // twist
    Reaction { dur: 0.70, height: 1.85, turns: 0.0, tilt: -0.16, squash: 0.30, tongue: true }, // big hop + tongue
    Reaction { dur: 0.72, height: 1.55, turns: 1.0, tilt: 0.0, squash: 0.22, tongue: false }, // spin
    Reaction { dur: 0.80, height: 1.70, turns: -1.0, tilt: 0.0, squash: 0.24, tongue: false }, // backflip
];

/// Pose at time `t` into reaction `rx` for a frog of radius `r`. The spin lands
/// upright (ease-out → an integer number of turns); the squash anticipates the
/// launch and thuds on landing.
fn react_pose(rx: &Reaction, t: f32, r: f32) -> draw::FrogPose {
    use std::f32::consts::{PI, TAU};
    let p = (t / rx.dur).clamp(0.0, 1.0);
    // Airborne arc: 0 at launch, 1 at apex, 0 at landing.
    let fly = (((p - 0.12) / 0.74).clamp(0.0, 1.0) * PI).sin();
    let dy = -fly * rx.height * r;
    // Crouch on launch, thud on landing.
    let crouch = (1.0 - p / 0.13).clamp(0.0, 1.0);
    let land = (1.0 - (p - 0.86).abs() / 0.12).clamp(0.0, 1.0);
    let squash = crouch.max(land) * rx.squash;
    let stretch = fly * 0.22;
    let sy = 1.0 + stretch - squash;
    let sx = 1.0 - stretch * 0.55 + squash * 0.9;
    let rot = rx.turns * TAU * crate::anim::ease_out_cubic(p) + rx.tilt * (p * PI).sin();
    let tongue = if rx.tongue { (1.0 - (p - 0.5).abs() / 0.24).clamp(0.0, 1.0) } else { 0.0 };
    // Eyes squint with joy at the peak of a tongue or spin jump.
    let blink = (tongue * 0.6).max(if rx.turns != 0.0 { fly * 0.35 } else { 0.0 });
    draw::FrogPose { dy, rot, sx, sy, blink, tongue }
}

/// Resting frog: a gentle breathing squash + an occasional blink, so the mascot
/// feels alive between taps. Driven by the scene clock (deterministic in golds).
fn idle_pose(time: f32) -> draw::FrogPose {
    use std::f32::consts::PI;
    let breathe = (time * 1.85).sin();
    let bt = time.rem_euclid(3.4);
    let blink = if bt < 0.16 { (bt / 0.16 * PI).sin() } else { 0.0 };
    draw::FrogPose {
        sx: 1.0 - 0.025 * breathe,
        sy: 1.0 + 0.03 * breathe,
        blink,
        ..Default::default()
    }
}

fn plan(f: &crate::layout::Frame) -> PLayout {
    let cx = f.w / 2.0;
    let tb = f.topbar();
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

    // Rainbow. The outer (red) band rises ~65*scale + stroke/2 above the
    // horizon; cap the scale so that apex never climbs above the topbar (where
    // the buttons already sit clear of the top edge / status bar) — otherwise a
    // short phone-landscape clips the top of the arc.
    let rb_horizon = card_y - 16.0;
    let rb_desired = card_w / 240.0 * 1.45;
    let rb_fit = ((rb_horizon - tb.y - 4.0) / 70.0).max(0.2);
    let rb_scale = rb_desired.min(rb_fit);

    // Companion frog at the left foot of the rainbow, feet on the horizon. The
    // rainbow's outer band meets the horizon ~84.7*scale left of center (radius
    // 65s/(1-cos75°) ≈ 87.7s, half-width = r·sin75°); the frog sits just beyond
    // it — beside the card, never over it, clear of every tap target.
    let fr = (card_h * 0.17).clamp(24.0, 54.0);
    let frog_x = cx - 84.7 * rb_scale - fr * 0.55;
    let frog = (vec2(frog_x, rb_horizon - 0.92 * fr), fr);

    PLayout {
        card,
        letter_size: (card_h * 0.6) as u16,
        rb_cx: cx,
        rb_horizon,
        rb_scale,
        rb_stroke: (10.0 * rb_scale).max(8.0),
        miss: (vec2(x0 + slot / 2.0, by), miss_r),
        got: (vec2(x0 + slot + gap + slot / 2.0, by), got_r),
        advance: (vec2(cx, by), got_r),
        frog,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Frame, Insets};

    fn frame(w: f32, h: f32) -> Frame {
        Frame::new(w, h, Insets::default())
    }

    /// The rainbow's outer band must clear the top of the viewport on every
    /// device — it used to overshoot and clip on short phone-landscape.
    #[test]
    fn rainbow_apex_clears_the_top() {
        // ipad both ways + a couple of real phone-landscape sizes.
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0), (812.0, 375.0)] {
            let f = frame(w, h);
            let p = plan(&f);
            // Outer (red) band: sagitta 65*scale above the horizon, plus the round cap.
            let apex = p.rb_horizon - 65.0 * p.rb_scale - p.rb_stroke / 2.0;
            assert!(apex >= 0.0, "{w}x{h}: rainbow apex {apex} above the viewport");
            // ...and stays at least as clear as the topbar buttons.
            assert!(apex >= f.topbar().y - 2.0, "{w}x{h}: apex {apex} above topbar {}", f.topbar().y);

            // The done/garden celebration rainbow must also clear the top.
            let (.., gy) = done_layout(&f);
            let (_cx, horizon, scale, stroke) = done_rainbow(&f, gy);
            let done_apex = horizon - 65.0 * scale - stroke / 2.0;
            assert!(done_apex >= 0.0, "{w}x{h}: done rainbow apex {done_apex} above the viewport");
        }
    }
}
