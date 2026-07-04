//! Number Scales: a compare-two-numbers game — "which is more, 3 or 7?". The
//! stimulus is a friendly BALANCE SCALE: two number cards ride the two pans, and
//! the child picks the bigger (or, at the top level, the smaller). Magnitude is
//! made physical — on a correct answer the scale visibly TIPS so the heavier
//! (bigger) side drops, which is both the reveal and the reward.
//!
//! Pedagogy (see `core::compare` for the encoded ladder):
//! - Grounds the numeral in a QUANTITY at the teaching levels (a ten-frame of
//!   dots under each number), then FADES it so the child must actually read.
//! - Ramps the numeric distance (far→adjacent) and range (0–9→up to 20), and
//!   only introduces "which is smaller?" at the top level.
//!
//! Grading is PARENT-MEDIATED, like phonics — a coin-flip tap can't cheat the
//! monotonic progress, and it verifies the child read the numbers. The child
//! taps the card they think is the answer (the scale gives a neutral, non-
//! revealing weight-nudge toward whatever they tapped); then the grown-up taps
//! ✓ / ✗. Only ✓ advances a star. ✗ is errorless: the correct side is revealed
//! (its quantity shown) with no penalty, and play moves on. Completing a whole
//! session at a difficulty records it as the new best (`core::compare`,
//! generation+max merge — never decrements; a parent "start over" resets it).
//!
//! No in-play instruction text: the phase/turn read from the scale's motion, a
//! non-text "big vs small" cue placard, and audio. Ambient motion (the fulcrum
//! face's blink) freezes during the Present lead-in so the target owns attention.
use crate::{
    anim, chrome, draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
    text,
};
use fountouki_core::{compare as cmp, rng::Mulberry32, settings};
use macroquad::prelude::*;
use nanoserde::SerJson;
use std::f32::consts::TAU;

/// Correct comparisons needed to finish a session and reach the Finale. Only a
/// parent ✓ advances this; misses are errorless and don't (never decrements).
const GOAL: u32 = 6;

/// Phase durations (seconds). Judge has no timeout — it waits for the grown-up.
const PRESENT_DUR: f32 = 0.9; // lead-in: cards ride in, scale still, face frozen
const REWARD_DUR: f32 = 1.3; // celebrate a correct compare (scale tipped, confetti)
const REVEAL_DUR: f32 = 1.7; // a miss: show the correct side (a touch longer to teach)
const ENTRY_FADE: f32 = 0.5; // first-round scene-entry dim-bloom (orient before play)

/// Full beam tilt (radians) when a side has fully dropped.
const FULL_TILT: f32 = 0.19;
/// The small, non-revealing nudge toward the tapped pan during Judge.
const NUDGE_TILT: f32 = 0.045;

/// Reward confetti burst; finale opening burst + gentle rain cadence.
const REWARD_BURST_N: usize = 90;
const FINALE_BURST_N: usize = 150;
const RAIN_INTERVAL_S: f32 = 0.12;
/// Star-pop curve shared by reward + finale (springy overshoot, capped).
const STAR_POP_DUR: f32 = 0.42;
const STAR_POP_CAP: f32 = 1.25;

/// Confetti seed salts (kept independent of the gameplay RNG so goldens stay
/// reproducible — same scheme as clock / sing back).
const CONFETTI_SEED_SALT: u32 = 0x9E37_79B9;
const CONFETTI_RESTART_SALT: u32 = 0x85EB_CA6B;

/// Finale interactive elements.
const FINALE_BALLOONS: usize = 5;
const FINALE_STARS: usize = 8;
const FINALE_FRIENDS: usize = 3;
/// Parked timer value meaning "idle" (no animation in flight).
const IDLE: f32 = 99.0;
const BALLOON_BOB_S: f32 = 0.8;
const STAR_TWINKLE_S: f32 = 0.6;
const FACE_WINK_S: f32 = 0.9;
const FRIEND_HOP_S: f32 = 0.8;
const SUN_FLARE_S: f32 = 0.9;

/// Finale tap-target ids (distinct so the per-target debounce only swallows a
/// same-target re-fire).
const TGT_REPLAY: u32 = 1;
const TGT_HOME: u32 = 2;
const TGT_FACE: u32 = 3;
const TGT_SUN: u32 = 4;
const TGT_BALLOON_BASE: u32 = 10;
const TGT_STAR_BASE: u32 = 40;
const TGT_FRIEND_BASE: u32 = 70;

/// The state a golden capture pins the scene into.
#[derive(Clone, Copy)]
pub enum CaptureState {
    /// Choose phase at a given level (dots on for far/near) — the main shot.
    Choose,
    ChooseRead,
    ChooseTeens,
    ChooseFewer,
    /// Judge phase: the child has chosen, the parent ✓/✗ are up.
    Judge,
    /// A correct answer mid-celebration (scale tipped, confetti).
    Reward,
    /// A miss mid-reveal (the correct side shown with its quantity).
    Reveal,
    /// The carnival finale.
    Finale,
}

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    /// Lead-in: the two cards are shown, the scale sits level and the fulcrum
    /// face is frozen; not tappable. At `PRESENT_DUR` play opens (Choose).
    Present { t: f32 },
    /// The child taps the card they think is the answer.
    Choose,
    /// The child has chosen; the grown-up taps ✓ / ✗. `t` drives the weight-nudge.
    Judge { t: f32 },
    /// A correct compare: the scale tips (bigger side drops), confetti, a star.
    Reward { t: f32 },
    /// A miss: the scale tips to the true correct side, its quantity revealed. No
    /// star (errorless), then the next round presents.
    Reveal { t: f32 },
    /// The session's GOAL is met: a carnival celebration.
    Finale { t: f32 },
}

pub struct CompareScene {
    db: Db,
    seed: u32,
    rng: Mulberry32,
    state: cmp::CompareState,
    /// Difficulty level 1..=5 (from the parent setting); may change live on sync.
    level: u32,
    round: cmp::Round,
    /// Correct answers this session (drives the meter). Monotonic within a session.
    stars: u32,
    /// Rising-pitch streak for the correct chime (reset on a miss).
    streak: u32,
    phase: Phase,
    first: bool,
    /// The side the child tapped this round (0 = left, 1 = right), if any.
    chosen: Option<u8>,
    /// Set on the round that raised `best_level` (escalates the finale push).
    new_best: bool,
    rain_acc: f32,
    /// Drives the one-shot "tap this one" hand on the cue placard: counts up from
    /// scene start, taps once, then holds still (no looping motion during play).
    cue_t: f32,
    tap_debounce: input::TapDebounce,
    confetti: crate::confetti::Confetti,
    sync: crate::net::SyncClient,
    /// Parent-chosen difficulty + its last-edit timestamp; synced (last-write-
    /// wins) under `comparecfg` so the level follows the family.
    cfg: settings::CompareSettings,
    cfg_sync: crate::net::SyncClient,
    // --- finale interaction state ---
    balloon_t: [f32; FINALE_BALLOONS],
    balloon_taps: u32,
    star_t: [f32; FINALE_STARS],
    star_taps: u32,
    face_t: f32,
    face_taps: u32,
    friend_t: [f32; FINALE_FRIENDS],
    friend_taps: u32,
    sun_t: f32,
    sun_taps: u32,
}

/// Map the parent-chosen difficulty string to a level number.
fn level_of(difficulty: &str) -> u32 {
    match difficulty {
        "near" => 2,
        "read" => 3,
        "teens" => 4,
        "fewer" => 5,
        _ => 1, // "far"
    }
}

impl CompareScene {
    pub fn new(db: Db, seed: u32, now: i64) -> CompareScene {
        let cfg = {
            let kv = db.borrow_kv();
            settings::load_compare(&**kv)
        };
        let level = level_of(&cfg.difficulty);
        let state = {
            let kv = db.borrow_kv();
            cmp::load(&**kv, now)
        };
        let sync = crate::net::SyncClient::new(db.clone(), "compare");
        let mut cfg_sync = crate::net::SyncClient::new(db.clone(), "comparecfg");
        if cfg.last_seen > 0 {
            cfg_sync.queue_push(&cfg.serialize_json(), now);
        }
        let mut rng = Mulberry32::new(seed);
        let round = cmp::next_round(level, &mut rng);
        CompareScene {
            db,
            seed,
            rng,
            state,
            level,
            round,
            stars: 0,
            streak: 0,
            phase: Phase::Present { t: 0.0 },
            first: true,
            chosen: None,
            new_best: false,
            rain_acc: 0.0,
            cue_t: 0.0,
            tap_debounce: input::TapDebounce::new(),
            confetti: crate::confetti::Confetti::new(seed.wrapping_add(CONFETTI_SEED_SALT)),
            sync,
            cfg,
            cfg_sync,
            balloon_t: [IDLE; FINALE_BALLOONS],
            balloon_taps: 0,
            star_t: [IDLE; FINALE_STARS],
            star_taps: 0,
            face_t: IDLE,
            face_taps: 0,
            friend_t: [IDLE; FINALE_FRIENDS],
            friend_taps: 0,
            sun_t: IDLE,
            sun_taps: 0,
        }
    }

    /// Build a scene pinned into a specific state for a golden capture.
    pub fn capture(db: Db, seed: u32, now: i64, cap: CaptureState, _ctx: &Ctx) -> CompareScene {
        let mut sc = CompareScene::new(db, seed, now);
        sc.first = false;
        // The cue hand has finished its one tap and holds still (goldens are a
        // single frame — pin it to the resting point).
        sc.cue_t = 3.0;
        // Pin a representative round + phase per capture (deterministic goldens).
        match cap {
            CaptureState::Choose => {
                sc.level = 1;
                sc.round = cmp::Round { left: 3, right: 7, want_smaller: false };
                sc.stars = 2;
                sc.phase = Phase::Choose;
            }
            CaptureState::ChooseRead => {
                sc.level = 3;
                sc.round = cmp::Round { left: 9, right: 4, want_smaller: false };
                sc.stars = 3;
                sc.phase = Phase::Choose;
            }
            CaptureState::ChooseTeens => {
                sc.level = 4;
                sc.round = cmp::Round { left: 12, right: 20, want_smaller: false };
                sc.stars = 1;
                sc.phase = Phase::Choose;
            }
            CaptureState::ChooseFewer => {
                sc.level = 5;
                sc.round = cmp::Round { left: 14, right: 8, want_smaller: true };
                sc.stars = 4;
                sc.phase = Phase::Choose;
            }
            CaptureState::Judge => {
                sc.level = 2;
                sc.round = cmp::Round { left: 6, right: 5, want_smaller: false };
                sc.stars = 2;
                sc.chosen = Some(0);
                sc.phase = Phase::Judge { t: 0.25 };
            }
            CaptureState::Reward => {
                sc.level = 1;
                sc.round = cmp::Round { left: 3, right: 7, want_smaller: false };
                sc.chosen = Some(1);
                sc.stars = 3;
                sc.phase = Phase::Reward { t: 0.5 };
                let lay = lay(&_ctx.frame);
                let wr = card_rect(&lay, 0.0, sc.round.correct_side());
                sc.confetti.burst(vec2(wr.x + wr.w / 2.0, wr.y + wr.h * 0.35), REWARD_BURST_N, lay.card_w * 0.6);
                // Step the physics so the burst reads as a spray, not a frozen row
                // (capture renders a single, un-updated frame).
                for _ in 0..30 {
                    sc.confetti.update(0.016);
                }
            }
            CaptureState::Reveal => {
                sc.level = 2;
                sc.round = cmp::Round { left: 5, right: 8, want_smaller: false };
                sc.chosen = Some(0); // tapped the smaller (wrong)
                sc.stars = 2;
                sc.phase = Phase::Reveal { t: 0.9 };
            }
            CaptureState::Finale => {
                sc.stars = GOAL;
                sc.level = 4;
                sc.state.best_level = 4;
                sc.phase = Phase::Finale { t: 0.7 };
                sc.sun_t = 0.2; // caught mid-flare so the golden shows the sun's rays
                let fl = finale_layout(&_ctx.frame);
                let trophy = vec2(fl.face.x, fl.face.y - fl.face_r * 2.2);
                sc.confetti.burst(trophy, FINALE_BURST_N, fl.face_r * 0.9);
                // Spread the burst + seed some falling rain so the frozen capture
                // frame reads as a live celebration.
                for _ in 0..34 {
                    sc.confetti.rain(_ctx.frame.w, 0.0, 1);
                    sc.confetti.update(0.016);
                }
            }
        }
        sc
    }

    fn save(&self) {
        let mut kv = self.db.borrow_kv_mut();
        cmp::save(&mut **kv, &self.state);
    }

    /// Deal the next round and open its Present lead-in.
    fn setup_round(&mut self) {
        self.round = cmp::next_round(self.level, &mut self.rng);
        self.chosen = None;
        self.phase = Phase::Present { t: 0.0 };
    }

    /// A correct compare: tip the scale, confetti, a star, and advance (or, at
    /// the GOAL, the Finale).
    fn enter_reward(&mut self, ctx: &Ctx) {
        self.stars += 1;
        self.streak += 1;
        let lay = lay(&ctx.frame);
        // Burst over the winning answer (the celebration lands ON the correct
        // card), plus a ribbit from Froggy as he hops.
        let win = self.round.correct_side();
        let wr = card_rect(&lay, 0.0, win);
        self.confetti.burst(vec2(wr.x + wr.w / 2.0, wr.y + wr.h * 0.35), REWARD_BURST_N, lay.card_w * 0.6);
        ctx.audio.frog();
        ctx.audio.correct(self.streak.saturating_sub(1));
        if self.stars >= GOAL {
            self.enter_finale(ctx);
        } else {
            self.phase = Phase::Reward { t: 0.0 };
        }
    }

    /// A miss: reveal the correct side (errorless — no star), then move on.
    fn enter_reveal(&mut self, ctx: &Ctx) {
        self.streak = 0;
        ctx.audio.incorrect();
        self.phase = Phase::Reveal { t: 0.0 };
    }

    fn enter_finale(&mut self, ctx: &Ctx) {
        cmp::record_level(&mut self.state, self.level, ctx.now);
        self.new_best = self.state.best_level == self.level;
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        self.balloon_t = [IDLE; FINALE_BALLOONS];
        self.star_t = [IDLE; FINALE_STARS];
        self.face_t = IDLE;
        self.friend_t = [IDLE; FINALE_FRIENDS];
        self.sun_t = IDLE;
        self.balloon_taps = 0;
        self.star_taps = 0;
        self.face_taps = 0;
        self.friend_taps = 0;
        self.sun_taps = 0;
        self.rain_acc = 0.0;
        let fl = finale_layout(&ctx.frame);
        let trophy = vec2(fl.face.x, fl.face.y - fl.face_r * 2.2);
        self.confetti.burst(trophy, FINALE_BURST_N, fl.face_r * 1.4);
        ctx.audio.finale();
        self.phase = Phase::Finale { t: 0.0 };
    }

    /// Replay from the Finale: fresh session, best kept (monotonic).
    fn restart(&mut self) {
        self.stars = 0;
        self.streak = 0;
        self.first = true;
        self.cue_t = 0.0; // replay is a fresh start — let the hand tap once again
        self.confetti = crate::confetti::Confetti::new(self.seed.wrapping_add(CONFETTI_RESTART_SALT));
        self.setup_round();
    }

    /// The current beam tilt (radians). Positive tips the RIGHT pan down.
    fn beam_tilt(&self) -> f32 {
        let drop_dir = |side: u8| if side == 1 { 1.0 } else { -1.0 };
        match self.phase {
            Phase::Judge { .. } => match self.chosen {
                Some(s) => drop_dir(s) * NUDGE_TILT,
                None => 0.0,
            },
            Phase::Reward { t } => {
                // back_out overshoots then settles — the heavier side drops with a
                // bit of weight, then bobs level, so the tip reads as physical.
                let k = anim::back_out(anim::clamp01(t / 0.55));
                drop_dir(self.round.correct_side()) * FULL_TILT * k
            }
            Phase::Reveal { t } => {
                let k = anim::ease_in_out_cubic(anim::clamp01(t / 0.75));
                drop_dir(self.round.correct_side()) * FULL_TILT * k
            }
            _ => 0.0,
        }
    }

    // --- input --------------------------------------------------------------

    /// Handle a tap during play (Choose / Judge). Returns true if consumed.
    fn handle_play_tap(&mut self, ctx: &Ctx) {
        let pt = ctx.pointer;
        if !pt.tapped() {
            return;
        }
        let l = lay(&ctx.frame);
        match self.phase {
            Phase::Choose => {
                for side in 0..2u8 {
                    let r = card_rect(&l, 0.0, side);
                    if input::hit_rect(pt.pos, r.x, r.y, r.w, r.h)
                        && self.tap_debounce.accept(side as u32, ctx.time)
                    {
                        self.chosen = Some(side);
                        ctx.audio.tap();
                        self.phase = Phase::Judge { t: 0.0 };
                        return;
                    }
                }
            }
            Phase::Judge { .. } => {
                // Parent verdict. ✓ = the child read + chose correctly.
                if input::hit_circle(pt.pos, l.got.x, l.got.y, l.grade_r)
                    && self.tap_debounce.accept(100, ctx.time)
                {
                    self.enter_reward(ctx);
                } else if input::hit_circle(pt.pos, l.miss.x, l.miss.y, l.grade_r)
                    && self.tap_debounce.accept(101, ctx.time)
                {
                    self.enter_reveal(ctx);
                }
            }
            _ => {}
        }
    }

    // --- finale --------------------------------------------------------------

    fn update_finale(&mut self, ctx: &Ctx) -> Nav {
        if let Phase::Finale { t } = self.phase {
            self.phase = Phase::Finale { t: t + ctx.dt };
        }
        step_timers(&mut self.balloon_t, ctx.dt, BALLOON_BOB_S);
        step_timers(&mut self.star_t, ctx.dt, STAR_TWINKLE_S);
        step_timers(&mut self.friend_t, ctx.dt, FRIEND_HOP_S);
        step_timers(std::slice::from_mut(&mut self.face_t), ctx.dt, FACE_WINK_S);
        step_timers(std::slice::from_mut(&mut self.sun_t), ctx.dt, SUN_FLARE_S);
        // Gentle confetti rain.
        self.rain_acc += ctx.dt;
        while self.rain_acc >= RAIN_INTERVAL_S {
            self.rain_acc -= RAIN_INTERVAL_S;
            self.confetti.rain(ctx.frame.w, 0.0, 2);
        }

        let pt = ctx.pointer;
        if pt.tapped() {
            let f = &ctx.frame;
            let (replay, home, br) = chrome::corner_buttons(f);
            if input::hit_circle(pt.pos, replay.x, replay.y, br) && self.tap_debounce.accept(TGT_REPLAY, ctx.time) {
                self.restart();
                return Nav::Stay;
            }
            if input::hit_circle(pt.pos, home.x, home.y, br) && self.tap_debounce.accept(TGT_HOME, ctx.time) {
                self.sync.flush();
                self.cfg_sync.flush();
                return Nav::Home;
            }
            let fl = finale_layout(f);
            // The sun: tap → burst of rays, a pop, a twinkle + sparkle spray.
            if input::hit_circle(pt.pos, fl.sun_c.x, fl.sun_c.y, fl.sun_r * 1.4) && self.tap_debounce.accept(TGT_SUN, ctx.time) {
                self.sun_t = 0.0;
                self.sun_taps += 1;
                ctx.audio.twinkle();
                self.confetti.burst(fl.sun_c, 18, fl.sun_r * 0.9);
                return Nav::Stay;
            }
            // The hero frog cycles through three reactions (hop / spin / wink), each
            // with its own sound — so repeat taps stay surprising.
            if input::hit_circle(pt.pos, fl.face.x, fl.face.y, fl.face_r * 1.2) && self.tap_debounce.accept(TGT_FACE, ctx.time) {
                match self.face_taps % 3 {
                    0 => ctx.audio.frog(),
                    1 => ctx.audio.twinkle(),
                    _ => ctx.audio.level_up(),
                }
                self.face_t = 0.0;
                self.face_taps += 1;
                return Nav::Stay;
            }
            // The little friend frogs: each hops with a ribbit when poked.
            for i in 0..FINALE_FRIENDS {
                let (c, rr) = fl.friends[i];
                if input::hit_circle(pt.pos, c.x, c.y, rr * 1.35)
                    && self.friend_t[i] >= FRIEND_HOP_S
                    && self.tap_debounce.accept(TGT_FRIEND_BASE + i as u32, ctx.time)
                {
                    self.friend_t[i] = 0.0;
                    self.friend_taps += 1;
                    ctx.audio.frog();
                    return Nav::Stay;
                }
            }
            for i in 0..FINALE_BALLOONS {
                let p = fl.balloon(i, self.finale_time());
                if input::hit_circle(pt.pos, p.x, p.y, fl.balloon_r) && self.tap_debounce.accept(TGT_BALLOON_BASE + i as u32, ctx.time) {
                    self.balloon_t[i] = 0.0;
                    self.balloon_taps += 1;
                    ctx.audio.tap();
                    return Nav::Stay;
                }
            }
            for i in 0..FINALE_STARS {
                let p = fl.star(i, self.finale_time());
                if input::hit_circle(pt.pos, p.x, p.y, fl.star_r * 1.2) && self.tap_debounce.accept(TGT_STAR_BASE + i as u32, ctx.time) {
                    self.star_t[i] = 0.0;
                    self.star_taps += 1;
                    ctx.audio.twinkle();
                    return Nav::Stay;
                }
            }
        }
        Nav::Stay
    }

    fn finale_time(&self) -> f32 {
        match self.phase {
            Phase::Finale { t } => t,
            _ => 0.0,
        }
    }

    // --- play-test / capture hooks -----------------------------------------
    pub(crate) fn stars(&self) -> u32 {
        self.stars
    }
    pub(crate) fn best_level(&self) -> u32 {
        self.state.best_level
    }
    pub(crate) fn level_id(&self) -> u32 {
        self.level
    }
    pub(crate) fn in_choose(&self) -> bool {
        matches!(self.phase, Phase::Choose)
    }
    pub(crate) fn in_judge(&self) -> bool {
        matches!(self.phase, Phase::Judge { .. })
    }
    pub(crate) fn in_finale(&self) -> bool {
        matches!(self.phase, Phase::Finale { .. })
    }
    pub(crate) fn correct_side(&self) -> u8 {
        self.round.correct_side()
    }
    pub(crate) fn card_center(&self, f: &crate::layout::Frame, side: u8) -> Vec2 {
        let r = card_rect(&lay(f), 0.0, side);
        vec2(r.x + r.w / 2.0, r.y + r.h / 2.0)
    }
    pub(crate) fn got_center(&self, f: &crate::layout::Frame) -> Vec2 {
        lay(f).got
    }
    pub(crate) fn miss_center(&self, f: &crate::layout::Frame) -> Vec2 {
        lay(f).miss
    }
    pub(crate) fn replay_center(&self, f: &crate::layout::Frame) -> Vec2 {
        chrome::corner_buttons(f).0
    }
    pub(crate) fn home_center(&self, f: &crate::layout::Frame) -> Vec2 {
        chrome::corner_buttons(f).1
    }
    pub(crate) fn finale_face_center(&self, f: &crate::layout::Frame) -> Vec2 {
        finale_layout(f).face
    }
    pub(crate) fn face_taps(&self) -> u32 {
        self.face_taps
    }
    pub(crate) fn finale_balloon_center(&self, f: &crate::layout::Frame, time: f32, i: usize) -> Vec2 {
        finale_layout(f).balloon(i.min(FINALE_BALLOONS - 1), time)
    }
    pub(crate) fn balloon_taps(&self) -> u32 {
        self.balloon_taps
    }
    pub(crate) fn finale_star_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        finale_layout(f).star(i.min(FINALE_STARS - 1), self.finale_time())
    }
    pub(crate) fn star_taps(&self) -> u32 {
        self.star_taps
    }
    pub(crate) fn finale_friend_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        finale_layout(f).friends[i.min(FINALE_FRIENDS - 1)].0
    }
    pub(crate) fn friend_taps(&self) -> u32 {
        self.friend_taps
    }
    pub(crate) fn finale_sun_center(&self, f: &crate::layout::Frame) -> Vec2 {
        finale_layout(f).sun_c
    }
    pub(crate) fn sun_taps(&self) -> u32 {
        self.sun_taps
    }
}

impl Scene for CompareScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.confetti.update(ctx.dt);
        self.cue_t += ctx.dt; // the cue hand's one-shot tap (holds still after)
        // Mastery sync.
        self.sync.drive(ctx.now);
        if let Some(remote) = self.sync.poll_pull() {
            if let Some(rstate) = cmp::validate(&remote) {
                self.state = cmp::merge(&self.state, &rstate, ctx.now);
                self.save();
                if self.state != rstate {
                    self.sync.queue_push(&self.state.serialize_json(), ctx.now);
                }
            }
        }
        // Parent-chosen difficulty (last-write-wins), adopted live.
        self.cfg_sync.drive(ctx.now);
        if let Some(remote) = self.cfg_sync.poll_pull() {
            let rcfg = settings::parse_compare(&remote);
            let merged = settings::merge_compare(&self.cfg, &rcfg);
            if merged != self.cfg {
                self.cfg = merged;
                self.level = level_of(&self.cfg.difficulty);
                let mut kv = self.db.borrow_kv_mut();
                settings::save_compare(&mut **kv, &self.cfg);
            }
            self.cfg_sync.queue_push(&self.cfg.serialize_json(), ctx.now);
        }

        // The Finale draws no topbar (full-screen scene) — handle it FIRST so the
        // invisible topbar corners never steal a tap.
        if matches!(self.phase, Phase::Finale { .. }) {
            return self.update_finale(ctx);
        }

        match chrome::handle_topbar(&chrome::topbar(&ctx.frame), ctx, &self.db) {
            Some(chrome::TopbarAction::OpenParent) => {
                self.sync.flush();
                self.cfg_sync.flush();
                return Nav::OpenParent;
            }
            Some(chrome::TopbarAction::Home) => {
                self.sync.flush();
                self.cfg_sync.flush();
                return Nav::Home;
            }
            Some(chrome::TopbarAction::MuteToggled) => return Nav::Stay,
            None => {}
        }

        match self.phase {
            Phase::Present { t } => {
                let prev = t;
                let t = t + ctx.dt;
                if prev <= PRESENT_DUR && t > PRESENT_DUR {
                    ctx.audio.twinkle(); // the non-text "now" cue
                }
                if t >= PRESENT_DUR {
                    self.first = false;
                    self.phase = Phase::Choose;
                } else {
                    self.phase = Phase::Present { t };
                }
            }
            Phase::Choose => self.handle_play_tap(ctx),
            Phase::Judge { t } => {
                self.phase = Phase::Judge { t: t + ctx.dt };
                self.handle_play_tap(ctx);
            }
            Phase::Reward { t } => {
                let t = t + ctx.dt;
                if t >= REWARD_DUR {
                    self.setup_round();
                } else {
                    self.phase = Phase::Reward { t };
                }
            }
            Phase::Reveal { t } => {
                let t = t + ctx.dt;
                if t >= REVEAL_DUR {
                    self.setup_round();
                } else {
                    self.phase = Phase::Reveal { t };
                }
            }
            Phase::Finale { .. } => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        if matches!(self.phase, Phase::Finale { .. }) {
            self.draw_finale(ctx);
            return;
        }

        let l = lay(&ctx.frame);
        // The pond world behind everything.
        self.draw_world(&l, ctx);
        // Progress meter + the big/small cue placard.
        draw_meter(&l, self.stars, ctx);
        draw_cue(&l, self.round.want_smaller, self.cue_t);

        // The scale (tilted per phase) with its two cards.
        self.draw_scale(&l, ctx);

        // Parent ✓/✗ during Judge.
        if matches!(self.phase, Phase::Judge { .. }) {
            draw::circle_btn(l.miss.x, l.miss.y, l.grade_r, palette::CARD);
            draw::mark_cross(l.miss.x, l.miss.y, l.grade_r, palette::MUTED);
            draw::circle_btn(l.got.x, l.got.y, l.grade_r, palette::OK);
            draw::mark_check(l.got.x, l.got.y, l.grade_r, palette::OK_STRONG);
        }

        chrome::draw_topbar(&chrome::topbar(&ctx.frame), ctx);
        self.confetti.draw();

        // First-round entry dim-bloom (the "lead into the task" cue).
        if let Phase::Present { t } = self.phase {
            if self.first {
                let a = (1.0 - anim::clamp01(t / ENTRY_FADE)) * 0.45;
                if a > 0.001 {
                    draw_rectangle(0.0, 0.0, ctx.frame.w, ctx.frame.h, palette::hexa(0x2b2c34, a));
                }
            }
        }
    }
}

// ===========================================================================
// Drawing
// ===========================================================================

impl CompareScene {
    /// The pond world behind the scale: a soft sky, calm water at the bottom, a
    /// grassy far bank with cattails, and a drifting dragonfly. Calm by design —
    /// the number cards are the target, so nothing busy sits near them.
    fn draw_world(&self, l: &Lay, ctx: &Ctx) {
        let f = &ctx.frame;
        let present = matches!(self.phase, Phase::Present { .. });
        let amb = if present { 0.0 } else { ctx.time };
        // Sky → a warm, pale wash so the cream cards still pop.
        draw::vgradient(0.0, 0.0, f.w, l.water_y, palette::hex(0xe4f2ff), palette::hex(0xeef8ea));
        // A hazy sun tucked in the top-right, clear of the meter/cue.
        draw::sun(f.w * 0.88, l.play.y + f.vmin(0.10), f.vmin(0.055).max(26.0));
        // Two slow clouds high in the sky.
        let cr = f.vmin(0.05).max(20.0);
        for &(hy, sc, spd, ph) in &[(0.30f32, 1.0f32, 6.0f32, 0.2f32), (0.5, 0.72, 9.0, 0.7)] {
            let span = f.w + cr * 8.0;
            let x = (amb * spd + ph * span).rem_euclid(span) - cr * 4.0;
            draw::cloud(x, l.play.y + (l.water_y - l.play.y) * hy, cr * sc);
        }
        // The pond, with a green bank strip along its near edge.
        draw::pond(0.0, l.water_y, f.w, f.h - l.water_y, amb);
        draw_rectangle(0.0, l.water_y - 5.0, f.w, 8.0, palette::hex(0x7cbf6a));
        // Cattails rising from the bank at both margins (kept out of the play area).
        let reed_h = (l.water_y - l.play.y) * 0.5 + l.frog_r;
        draw::cattail(f.w * 0.06, l.water_y + 6.0, reed_h, 0.15 * (amb * 0.8).sin());
        draw::cattail(f.w * 0.10, l.water_y + 10.0, reed_h * 0.8, -0.2 * (amb * 0.7).sin());
        draw::cattail(f.w * 0.95, l.water_y + 6.0, reed_h * 0.92, -0.15 * (amb * 0.8 + 1.0).sin());
        draw::cattail(f.w * 0.90, l.water_y + 10.0, reed_h * 0.7, 0.2 * (amb * 0.7 + 1.0).sin());
        // A floating lily pad + a dragonfly skimming just over the water (down in
        // the corner, well clear of the number cards).
        draw::lily_pad(f.w * 0.18, l.water_y + (f.h - l.water_y) * 0.55, l.frog_r * 0.7, palette::hex(0x57b061), 0.0);
        let dfy = l.water_y - (l.water_y - l.play.y) * 0.10 + 6.0 * (amb * 1.3).sin();
        draw::dragonfly(f.w * (0.82 + 0.03 * (amb * 0.5).sin()), dfy, f.vmin(0.06).max(26.0), amb * 9.0, palette::RAINBOW[5]);
    }

    fn draw_scale(&self, l: &Lay, ctx: &Ctx) {
        let tilt = self.beam_tilt();
        let (s, c) = tilt.sin_cos();
        let left_end = vec2(l.pivot.x - l.half_beam * c, l.pivot.y - l.half_beam * s);
        let right_end = vec2(l.pivot.x + l.half_beam * c, l.pivot.y + l.half_beam * s);

        // --- Froggy the weigh-master, balancing the pole on his head ---
        let pose = self.froggy_pose(ctx.time, l.frog_r);
        let r = l.frog_r;
        // The pole runs from the pivot down to rest in the dip between Froggy's
        // eyes — he balances the whole scale on his head (it rides his hop).
        let crown = vec2(l.frog_c.x, l.frog_c.y - r * 0.92 + pose.dy);
        let post_w = (l.half_beam * 0.08).max(7.0);
        draw::stroke_path(&[l.pivot, crown], post_w, palette::hex(0xe3b96a));
        // Lily pad + Froggy.
        draw::lily_pad(l.frog_c.x, l.frog_c.y + r * 1.24, r * 1.75, palette::hex(0x57b061), 0.0);
        draw::frog(l.frog_c.x, l.frog_c.y, r, palette::RAINBOW[3], pose);
        // A little contact cap where the pole meets his head, so it reads as
        // balanced-on-head, not stuck-through.
        draw::fill_ellipse(crown.x, crown.y, r * 0.34, r * 0.13, 0.0, palette::hex(0xe3b96a));
        draw::fill_ellipse(crown.x, crown.y - r * 0.03, r * 0.22, r * 0.08, 0.0, palette::hex(0xf6b73c));

        // --- pans (drawn first so the beam + hub sit on top of the cords) ---
        self.draw_pan(l, ctx, left_end, 0);
        self.draw_pan(l, ctx, right_end, 1);

        // --- beam ---
        let beam_w = (l.half_beam * 0.08).max(9.0);
        draw::stroke_path(&[left_end, right_end], beam_w + 3.0, palette::hex(0xc8881f));
        draw::stroke_path(&[left_end, right_end], beam_w, palette::hex(0xf6b73c));

        // --- pivot hub: a simple honey knob (Froggy carries the personality now) ---
        let hub = (l.half_beam * 0.11).max(11.0);
        draw::disc(l.pivot.x, l.pivot.y, hub, palette::hex(0xf6b73c));
        draw::disc(l.pivot.x, l.pivot.y, hub * 0.58, palette::hex(0xc8881f));
    }

    /// Froggy's pose per phase. He stays CALM and near-still while the child is
    /// reading/comparing (Choose) or the answer is being shown (Reveal) — no
    /// competing motion on the axis between the two numerals — and saves his
    /// liveliness for the reward: a big open-eyed, tongue-out hop when a correct
    /// answer tips the scale. A slight lean toward the tapped pan during grading.
    /// He never sulks on a miss (errorless).
    fn froggy_pose(&self, _time: f32, r: f32) -> draw::FrogPose {
        match self.phase {
            Phase::Reward { t } => {
                let fly = (anim::clamp01(t / 0.55) * std::f32::consts::PI).sin();
                draw::FrogPose {
                    dy: -fly * r * 0.7,
                    sy: 1.0 + fly * 0.16,
                    sx: 1.0 - fly * 0.08,
                    tongue: fly,
                    ..Default::default()
                }
            }
            Phase::Judge { .. } => {
                let lean = match self.chosen {
                    Some(1) => 0.05,
                    Some(_) => -0.05,
                    None => 0.0,
                };
                draw::FrogPose { rot: lean, ..Default::default() }
            }
            // Present / Choose / Reveal: hold still (calm smile) so nothing
            // animates near the numerals while the child reads.
            _ => draw::FrogPose::default(),
        }
    }

    fn draw_pan(&self, l: &Lay, ctx: &Ctx, end: Vec2, side: u8) {
        // The card hangs BELOW the beam end on two cords (so the beam is never
        // drawn across it), with a slim plate under it.
        let card_top = end.y + l.cord;
        let cx = end.x;
        let hw = l.card_w * 0.42;
        draw::stroke_path(&[end, vec2(cx - hw, card_top)], 2.5, palette::hex(0xc8881f));
        draw::stroke_path(&[end, vec2(cx + hw, card_top)], 2.5, palette::hex(0xc8881f));
        // slim plate under the card
        let plate_y = card_top + l.card_h + 2.0;
        draw::rounded_rect(cx - hw, plate_y, hw * 2.0, 9.0, 4.0, palette::hex(0xe3b96a));
        draw::arc(cx, plate_y, hw * 0.9, 0.1, std::f32::consts::PI - 0.1, 3.0, palette::hex(0xc8881f));

        let value = if side == 0 { self.round.left } else { self.round.right };
        let cr = Rect::new(cx - l.card_w / 2.0, card_top, l.card_w, l.card_h);

        // per-phase highlight state
        let correct = self.round.correct_side() == side;
        let chosen = self.chosen == Some(side);
        let (glow, pop, tint, show_dots) = match self.phase {
            Phase::Reward { t } => {
                if correct {
                    let k = anim::back_out(anim::clamp01(t / STAR_POP_DUR)).min(STAR_POP_CAP);
                    (Some(palette::OK_STRONG), 1.0 + 0.10 * (k - 1.0).max(0.0), None, cmp::show_quantity(self.level))
                } else {
                    (None, 1.0, None, cmp::show_quantity(self.level))
                }
            }
            Phase::Reveal { t } => {
                let fade = anim::clamp01(t / 0.5);
                if correct {
                    (Some(palette::OK_STRONG), 1.0, None, value <= 10)
                } else if chosen {
                    (None, 1.0, Some(palette::hexa(0xf6b3a2, 0.5 * fade)), value <= 10)
                } else {
                    (None, 1.0, None, value <= 10)
                }
            }
            _ => (
                if chosen { Some(palette::ACCENT) } else { None },
                if chosen { 1.03 } else { 1.0 },
                None,
                cmp::show_quantity(self.level),
            ),
        };

        draw_card(cr, value, pop, glow, tint, show_dots, ctx);
    }

    // --- finale ------------------------------------------------------------
    /// The pond party: Froggy hoists a golden trophy on a big lily pad, friends
    /// party on their own pads, balloons bob in the sky, and the child can poke
    /// blossoms, balloons and Froggy — everything reachable does something.
    fn draw_finale(&self, ctx: &Ctx) {
        let f = &ctx.frame;
        let fl = finale_layout(f);
        let time = self.finale_time();
        // A bright, happy pond-noon sky (same world as play, dialled sunnier).
        draw::vgradient(0.0, 0.0, f.w, fl.water_y, palette::hex(0xbfe8ff), palette::hex(0xeaf7ea));
        // The sun — tappable: it pops and throws a burst of rays across the sky.
        let sun_pop = if self.sun_t < SUN_FLARE_S {
            1.0 + (self.sun_t / SUN_FLARE_S * std::f32::consts::PI).sin() * 0.18
        } else {
            1.0
        };
        draw::sun_rays(fl.sun_c.x, fl.sun_c.y, fl.sun_r, (1.0 - self.sun_t).max(0.0), time * 1.5);
        draw::sun(fl.sun_c.x, fl.sun_c.y, fl.sun_r * sun_pop);
        let cr = f.vmin(0.05).max(22.0);
        for &(hy, sc, spd, ph) in &[(0.18f32, 1.05f32, 6.0f32, 0.15f32), (0.30, 0.75, 9.0, 0.6), (0.12, 0.85, 4.5, 0.9)] {
            let span = f.w + cr * 8.0;
            let x = (time * spd + ph * span).rem_euclid(span) - cr * 4.0;
            draw::cloud(x, fl.water_y * hy, cr * sc);
        }
        // Number bunting: a flag per correct compare, popping in one by one.
        self.draw_number_bunting(ctx, &fl);

        // The pond, with cattails along the far bank.
        draw::pond(0.0, fl.water_y, f.w, f.h - fl.water_y, time);
        draw_rectangle(0.0, fl.water_y - 5.0, f.w, 8.0, palette::hex(0x7cbf6a));
        let reed_h = fl.face_r * 2.2;
        draw::cattail(f.w * 0.05, fl.water_y + 6.0, reed_h, 0.15 * (time * 0.8).sin());
        draw::cattail(f.w * 0.96, fl.water_y + 6.0, reed_h * 0.9, -0.15 * (time * 0.8).sin());
        // A dragonfly skims the pond in the near corner (a little life on the water).
        let dfy = fl.water_y + (f.h - fl.water_y) * 0.14 + 5.0 * (time * 1.3).sin();
        draw::dragonfly(f.w * (0.13 + 0.02 * (time * 0.5).sin()), dfy, f.vmin(0.055).max(24.0), time * 9.0, palette::RAINBOW[5]);

        // Friend frogs, each on a lily pad. They float on a gentle bob, hop on a
        // lazy ambient cadence, and each has its OWN tap reaction (hop / spin /
        // wink) so poking the little ones stays fun.
        for (i, &((fc, fr), (body, hat))) in [
            (fl.friends[0], (palette::RAINBOW[6], palette::GOLD)),
            (fl.friends[1], (palette::RAINBOW[1], palette::RAINBOW[4])),
            (fl.friends[2], (palette::RAINBOW[4], palette::RAINBOW[0])),
        ]
        .iter()
        .enumerate()
        {
            let bob = fr * 0.06 * (time * 1.3 + i as f32 * 2.1).sin(); // floats on the water
            let cy = fc.y + bob;
            draw::lily_pad(fc.x, cy + fr * 1.15, fr * 1.7, palette::hex(0x4fa85a), 0.0);
            let pose = if self.friend_t[i] < FRIEND_HOP_S {
                react_pose(i as u8, self.friend_t[i] / FRIEND_HOP_S, fr)
            } else {
                let phase = i as f32 * 2.1;
                let amb = (time + phase).rem_euclid(6.4);
                if amb < 0.7 {
                    let fly = (amb / 0.7 * std::f32::consts::PI).sin();
                    draw::FrogPose { dy: -fly * fr * 0.5, sy: 1.0 + fly * 0.12, sx: 1.0 - fly * 0.06, ..Default::default() }
                } else {
                    let breathe = (time * 1.85 + phase).sin();
                    draw::FrogPose { sx: 1.0 - 0.025 * breathe, sy: 1.0 + 0.03 * breathe, ..Default::default() }
                }
            };
            draw::frog(fc.x, cy, fr, body, pose);
            draw::frog_party_hat(fc.x, cy, fr, pose, hat);
        }

        // Blossoms floating on the water (tap → bloom + twinkle). `fl.star` bobs
        // them on the ripples so they visibly drift.
        for i in 0..FINALE_STARS {
            let p = fl.star(i, time);
            let bloom = if self.star_t[i] < STAR_TWINKLE_S {
                0.6 + 0.4 * (1.0 - self.star_t[i] / STAR_TWINKLE_S)
            } else {
                0.55 + 0.06 * anim::pulse(time + i as f32, 2.2)
            };
            draw::lily_pad(p.x, p.y, fl.star_r, palette::hex(0x53ab5d), bloom);
        }

        // Balloons drifting in the sky (tap → pop-wobble). Each sways on its own
        // cadence with a curly string trailing below.
        for i in 0..FINALE_BALLOONS {
            let p = fl.balloon(i, time);
            let col = palette::RAINBOW[i % palette::RAINBOW.len()];
            let sc = if self.balloon_t[i] < BALLOON_BOB_S { 1.0 + 0.18 * (1.0 - self.balloon_t[i] / BALLOON_BOB_S) } else { 1.0 };
            let sway = 0.13 * (time * 0.6 + i as f32 * 1.7).sin();
            let tail = vec2(p.x + fl.balloon_r * 2.4 * sway.sin(), p.y + fl.balloon_r * 2.4 * sway.cos().max(0.3));
            draw::stroke_path(&[vec2(p.x, p.y + fl.balloon_r * 1.05), tail], 1.6, palette::hexa(0xffffff, 0.7));
            draw::fill_ellipse(p.x, p.y, fl.balloon_r * sc, fl.balloon_r * 1.18 * sc, sway.to_degrees(), col);
            draw::disc(p.x - fl.balloon_r * 0.32, p.y - fl.balloon_r * 0.42, fl.balloon_r * 0.18, palette::hexa(0xffffff, 0.5));
        }

        // The hero: Froggy on his big pad, hoisting the golden trophy star. He
        // cycles reactions (hop / spin / wink) on repeat taps.
        let hero = fl.face;
        let hr = fl.face_r;
        draw::lily_pad(hero.x, hero.y + hr * 1.2, hr * 2.2, palette::hex(0x57b061), 0.0);
        let hop = if self.face_t < FACE_WINK_S {
            react_pose(((self.face_taps.saturating_sub(1)) % 3) as u8, self.face_t / FACE_WINK_S, hr)
        } else {
            let breathe = (time * 1.7).sin();
            draw::FrogPose { sx: 1.0 - 0.02 * breathe, sy: 1.0 + 0.025 * breathe, blink: 0.15, ..Default::default() }
        };
        draw::frog(hero.x, hero.y, hr, palette::RAINBOW[3], hop);
        draw::frog_party_hat(hero.x, hero.y, hr, hop, palette::ACCENT);

        // Trophy star popping above Froggy, with an orbiting sparkle ring.
        let pop = anim::back_out(anim::clamp01(time / STAR_POP_DUR)).min(STAR_POP_CAP);
        let throb = 1.0 + 0.05 * anim::pulse(time, 1.6);
        let tr = vec2(hero.x, hero.y - hr * 2.3 + hop.dy);
        for k in 0..8 {
            let a = k as f32 / 8.0 * TAU + time * 0.5;
            let rr = hr * 1.4 * pop;
            draw::disc(tr.x + a.cos() * rr, tr.y + a.sin() * rr, hr * 0.09, palette::hexa(0xfff3a8, 0.9));
        }
        draw::star(tr.x, tr.y, hr * 1.0 * pop * throb, palette::hex(0xf6b800));

        self.confetti.draw();
        let (replay, home, br) = chrome::corner_buttons(f);
        chrome::draw_corner_buttons(replay, home, br);
    }

    /// A garland of numbered rainbow PENNANTS strung across the top — one per
    /// correct compare (the session's "trophies"). They pop in one by one, the
    /// whole string sways, each pennant swings on its own little pendulum, and a
    /// gold star caps each rope knot — a proper banner, not a thin line of tags.
    fn draw_number_bunting(&self, ctx: &Ctx, fl: &FinaleLayout) {
        let f = &ctx.frame;
        let time = self.finale_time();
        let (x0, x1) = (f.w * 0.06, f.w * 0.94);
        let y = f.h * 0.05;
        let sag = f.h * 0.055;
        // The whole garland sways slowly, as if in a light breeze.
        let sway = (time * 0.8).sin() * f.h * 0.006;
        let yat = |t: f32| y + sway + sag * 4.0 * t * (1.0 - t);
        // Two ropes for a fuller garland.
        const SEG: usize = 48;
        for rope in 0..2 {
            let off = rope as f32 * 4.0;
            let mut line = Vec::with_capacity(SEG + 1);
            for i in 0..=SEG {
                let t = i as f32 / SEG as f32;
                line.push(vec2(x0 + (x1 - x0) * t, yat(t) + off));
            }
            draw::stroke_path(&line, 3.5, palette::hexa(0x6f5a4a, 0.85));
        }
        let n = GOAL;
        let fs = fl.flag_s * 1.3; // bigger, bolder pennants
        for i in 0..n {
            let t = (i as f32 + 0.5) / n as f32;
            let popt = anim::clamp01((time - 0.25 - 0.12 * i as f32) / 0.42);
            if popt <= 0.0 {
                continue;
            }
            let sc = anim::back_out(popt);
            let fsw = fs * sc;
            let x = x0 + (x1 - x0) * t;
            let top = yat(t);
            // The rope's local slope + a gentle per-flag pendulum swing.
            let slope = (yat(t + 0.02) - yat(t - 0.02)).atan2((x1 - x0) * 0.04);
            let rot = slope + 0.07 * (time * 1.3 + i as f32 * 0.7).sin();
            let (sr, crot) = rot.sin_cos();
            // Local→screen: +ly points DOWN the pennant (toward its apex).
            let rp = |lx: f32, ly: f32| vec2(x + lx * crot - ly * sr, top + lx * sr + ly * crot);
            let col = palette::RAINBOW[i as usize % 7];
            let a = rp(-fsw * 0.5, 0.0);
            let b = rp(fsw * 0.5, 0.0);
            let apex = rp(0.0, fsw * 1.4);
            draw_triangle(a, b, apex, col);
            // A soft shine down the left face + a gold star over the rope knot.
            let mid = rp(-fsw * 0.16, fsw * 0.62);
            draw_triangle(a, mid, apex, palette::hexa(0xffffff, 0.16));
            draw::star(x, top - fsw * 0.05, fsw * 0.17, palette::GOLD);
            // The number, dark ink for contrast on the bright pennant, centered on
            // the upper body.
            let np = rp(0.0, fsw * 0.58);
            text::draw_centered_rot(
                &(i + 1).to_string(),
                np.x,
                np.y,
                (fsw * 0.72).max(1.0) as u16,
                &ctx.fonts.cursive,
                palette::INK,
                rot,
            );
        }
    }
}

/// Draw one number card: rounded tile + honey border, a cursive numeral (with
/// two-digit tracking), an optional ten-frame of dots, and highlight glow/tint.
fn draw_card(cr: Rect, value: u8, pop: f32, glow: Option<Color>, tint: Option<Color>, dots: bool, ctx: &Ctx) {
    // pop scales about the card center
    let (cx, cy) = (cr.x + cr.w / 2.0, cr.y + cr.h / 2.0);
    let cw = cr.w * pop;
    let ch = cr.h * pop;
    let r = Rect::new(cx - cw / 2.0, cy - ch / 2.0, cw, ch);

    if let Some(g) = glow {
        draw::rounded_rect(r.x - 8.0, r.y - 8.0, r.w + 16.0, r.h + 16.0, palette::RADIUS + 8.0, Color::new(g.r, g.g, g.b, 0.35));
    }
    draw::card(r.x, r.y, r.w, r.h, palette::CARD);
    // border ring
    let ring = glow.unwrap_or(palette::CELL_BORDER);
    draw::rounded_rect(r.x, r.y, r.w, r.h, palette::RADIUS, Color::new(ring.r, ring.g, ring.b, 0.0));
    stroke_rounded(r, palette::RADIUS, (r.w * 0.035).max(3.0), ring);
    if let Some(t) = tint {
        draw::rounded_rect(r.x, r.y, r.w, r.h, palette::RADIUS, t);
    }

    // numeral (top portion when dots show, else centered)
    let num_cy = if dots { r.y + r.h * 0.34 } else { cy };
    let num_px = if dots { (r.h * 0.40) as u16 } else { (r.h * 0.56) as u16 };
    text::draw_centered_tracked(&value.to_string(), cx, num_cy, num_px, &ctx.fonts.cursive, palette::INK, text::NUMERAL_TRACKING);

    if dots {
        let fh = r.h * 0.34;
        let fw = r.w * 0.66;
        let frame = Rect::new(cx - fw / 2.0, r.y + r.h * 0.60, fw, fh);
        draw_ten_frame(frame, value, ctx);
    }
}

/// A ten-frame (2 rows × 5): draw `n` filled dots over a light grid. Only used
/// for 0..=10 (the teaching levels + the reveal), where the count is subitizable.
fn draw_ten_frame(r: Rect, n: u8, _ctx: &Ctx) {
    let n = n.min(10);
    let cw = r.w / 5.0;
    let chh = r.h / 2.0;
    let dot = (cw.min(chh) * 0.36).max(3.0);
    for row in 0..2 {
        for col in 0..5 {
            let cx = r.x + (col as f32 + 0.5) * cw;
            let cy = r.y + (row as f32 + 0.5) * chh;
            let idx = row * 5 + col;
            if idx < n as usize {
                draw::disc(cx, cy, dot, palette::RAINBOW[3]);
            } else {
                draw::disc(cx, cy, dot * 0.8, palette::hexa(0x2b2c34, 0.08));
            }
        }
    }
}

/// Stroke the outline of a rounded rect (macroquad has no rounded stroke).
fn stroke_rounded(r: Rect, rad: f32, w: f32, color: Color) {
    let rad = rad.min(r.w / 2.0).min(r.h / 2.0);
    draw::stroke_path(&[vec2(r.x + rad, r.y), vec2(r.x + r.w - rad, r.y)], w, color);
    draw::stroke_path(&[vec2(r.x + rad, r.y + r.h), vec2(r.x + r.w - rad, r.y + r.h)], w, color);
    draw::stroke_path(&[vec2(r.x, r.y + rad), vec2(r.x, r.y + r.h - rad)], w, color);
    draw::stroke_path(&[vec2(r.x + r.w, r.y + rad), vec2(r.x + r.w, r.y + r.h - rad)], w, color);
    draw::arc(r.x + rad, r.y + rad, rad, std::f32::consts::PI, 1.5 * std::f32::consts::PI, w, color);
    draw::arc(r.x + r.w - rad, r.y + rad, rad, 1.5 * std::f32::consts::PI, TAU, w, color);
    draw::arc(r.x + rad, r.y + r.h - rad, rad, 0.5 * std::f32::consts::PI, std::f32::consts::PI, w, color);
    draw::arc(r.x + r.w - rad, r.y + r.h - rad, rad, 0.0, 0.5 * std::f32::consts::PI, w, color);
}

/// Progress meter: a centered row of GOAL tokens, filled per correct answer.
fn draw_meter(l: &Lay, stars: u32, _ctx: &Ctx) {
    let r0 = (l.play.w * 0.012).clamp(7.0, 12.0);
    let gap = r0 * 1.4;
    let total = (GOAL as f32 - 1.0) * (2.0 * r0 + gap);
    let x0 = l.play.x + l.play.w / 2.0 - total / 2.0;
    for i in 0..GOAL {
        let cx = x0 + i as f32 * (2.0 * r0 + gap);
        if i < stars {
            draw::disc(cx, l.meter_y, r0, palette::RAINBOW[(i as usize) % palette::RAINBOW.len()]);
        } else {
            draw::disc(cx, l.meter_y, r0, palette::hexa(0x2b2c34, 0.10));
        }
    }
}

/// The non-text direction cue: a placard with a small circle and a big circle
/// (same colour, so ONLY size differs — size IS the instruction), and a pointing
/// hand that TAPS the target one. "Bigger" points at the big circle, "smaller" at
/// the little one. The hand taps once when the scene opens, then holds still,
/// pointing — no looping motion to distract from reading the numbers.
fn draw_cue(l: &Lay, want_smaller: bool, cue_t: f32) {
    let c = l.cue;
    draw::card(c.x, c.y, c.w, c.h, palette::CARD);
    // The two size emblems sit low in the card, leaving room for the hand above.
    let cy = c.y + c.h * 0.70;
    let small = vec2(c.x + c.w * 0.30, cy);
    let big = vec2(c.x + c.w * 0.70, cy);
    let sr = c.h * 0.11;
    let br = c.h * 0.22;
    let dim = palette::hexa(0x2bd5a0, 0.32); // pale green — the "other" size
    draw::disc(small.x, small.y, sr, dim);
    draw::disc(big.x, big.y, br, dim);
    // The wanted size: a solid green circle with a calm, static ring (no pulse).
    let (tp, tr) = if want_smaller { (small, sr) } else { (big, br) };
    draw::disc(tp.x, tp.y, tr, palette::RAINBOW[3]);
    draw::arc(tp.x, tp.y, tr + (c.h * 0.05).max(3.0), 0.0, TAU, (c.h * 0.03).max(2.5), palette::hexa(0x2b9d5f, 0.9));
    // The pointing hand, tapping the target then resting just above it.
    let press = cue_press(cue_t);
    let hand_s = c.h * 0.24;
    let hover = tr * 0.6 + hand_s * 0.2;
    let tip = vec2(tp.x, tp.y - tr - hover * (1.0 - press));
    draw_tap_hand(tip, hand_s);
}

/// The cue hand's one-shot tap: raised → presses down onto the target → settles
/// to a resting hover and holds there. Returns 0 (fully raised) .. 1 (touching).
fn cue_press(t: f32) -> f32 {
    let t = t - 0.25; // a short lead-in so the tap lands after the cards settle
    if t <= 0.0 {
        return 0.0;
    }
    let rest = 0.5;
    let down = 0.30;
    let up = 0.42;
    if t < down {
        anim::ease_out_cubic(t / down) // 0 → 1: press down
    } else if t < down + up {
        1.0 - (1.0 - rest) * anim::ease_in_out_cubic((t - down) / up) // 1 → rest
    } else {
        rest // hold, still, pointing
    }
}

/// A friendly cartoon hand pointing straight DOWN, fingertip at `tip`. A curled
/// fist up top, an extended index finger, a thumb off to the side — flat fills,
/// matching the app's neutral-vector look.
fn draw_tap_hand(tip: Vec2, s: f32) {
    let skin = palette::hex(0xf6c99e);
    let cuff = palette::hex(0xe86a8e); // a cheerful pink sleeve cuff
    let fist = vec2(tip.x - s * 0.04, tip.y - s * 1.18);
    // Sleeve cuff behind the fist.
    draw::rounded_rect(fist.x - s * 0.46, fist.y - s * 0.62, s * 0.92, s * 0.44, s * 0.16, cuff);
    // Fist / palm.
    draw::rounded_rect(fist.x - s * 0.44, fist.y - s * 0.30, s * 0.88, s * 0.72, s * 0.28, skin);
    // Curled fingers (two knuckle bumps on the right).
    draw::disc(fist.x + s * 0.34, fist.y - s * 0.02, s * 0.15, skin);
    draw::disc(fist.x + s * 0.33, fist.y + s * 0.24, s * 0.14, skin);
    // Thumb across the front.
    draw::disc(fist.x - s * 0.36, fist.y + s * 0.12, s * 0.15, skin);
    // Extended index finger down to the tip.
    draw::stroke_path(&[vec2(tip.x, fist.y + s * 0.20), tip], s * 0.30, skin);
    draw::disc(tip.x, tip.y, s * 0.15, skin); // rounded fingertip
}

// ===========================================================================
// Layout
// ===========================================================================

struct Lay {
    play: Rect,
    pivot: Vec2,
    half_beam: f32,
    cord: f32,
    card_w: f32,
    card_h: f32,
    cue: Rect,
    meter_y: f32,
    grade_r: f32,
    grade_top: f32,
    got: Vec2,
    miss: Vec2,
    /// Froggy the weigh-master: he stands on a lily pad at the base and holds the
    /// balance post up. `frog_c` is his body center, `frog_r` his body radius.
    frog_c: Vec2,
    frog_r: f32,
    /// The waterline: the pond fills the screen below this y.
    water_y: f32,
}

fn lay(f: &crate::layout::Frame) -> Lay {
    let cx = f.w / 2.0;
    let tb = f.topbar();
    let top = tb.y + tb.h;
    let bottom = f.h - f.safe.bottom.max(8.0);

    let meter_y = top + (f.h * 0.02).clamp(10.0, 22.0);
    // cue placard, centered under the meter (tall enough for the pointing hand)
    let cue_w = (f.w * 0.17).clamp(150.0, 250.0);
    let cue_h = (f.h * 0.13).clamp(76.0, 118.0);
    let cue = Rect::new(cx - cue_w / 2.0, meter_y + (f.h * 0.03).clamp(18.0, 34.0), cue_w, cue_h);

    // grade buttons pinned near the bottom
    let grade_r = (f.w * 0.045).clamp(34.0, 54.0);
    let grade_cy = bottom - grade_r - 4.0;
    let grade_top = grade_cy - grade_r - 8.0;
    let gap = if f.is_phone() { 60.0 } else { 120.0 };
    let got = vec2(cx + gap / 2.0 + grade_r, grade_cy);
    let miss = vec2(cx - gap / 2.0 - grade_r, grade_cy);

    // scale between the cue and the grade band
    let region_top = cue.y + cue.h + (f.h * 0.02).clamp(8.0, 24.0);
    let region_bot = grade_top - 6.0;
    // Portrait is narrow but tall — spread the scale wider so it doesn't read as
    // a small object marooned in empty space. Landscape keeps its tuned 0.24.
    let beam_frac = if f.is_portrait() { 0.32 } else { 0.24 };
    let half_beam = (f.w * beam_frac).clamp(120.0, 320.0);
    let card_w = (half_beam * 0.62).clamp(90.0, 190.0);
    let card_h = card_w * 1.12;
    let cord = (card_h * 0.28).clamp(22.0, 60.0);
    // Center the scale assembly (fulcrum face down to the hanging pans) in the
    // region, but never let the pans push past the grade band.
    let pans_drop = cord + card_h + FULL_TILT * half_beam + 28.0;
    let face_r = (half_beam * 0.17).max(16.0);
    let pivot_y = (((region_top + region_bot) + face_r - pans_drop) / 2.0)
        .clamp(region_top + face_r + 6.0, (region_bot - pans_drop).max(region_top + face_r + 6.0));

    // Froggy the weigh-master stands at the base (feet on his lily pad), holding
    // the post. The pond's waterline sits at his lily pad so he floats on it.
    let base_y = (pivot_y + cord + card_h + 22.0).min(grade_top - 6.0);
    let frog_r = (half_beam * 0.30).clamp(34.0, 80.0);
    let frog_c = vec2(cx, base_y - 0.92 * frog_r);
    let water_y = base_y - frog_r * 0.30;

    let play = Rect::new(0.0, top, f.w, f.h - top);
    Lay {
        play,
        pivot: vec2(cx, pivot_y),
        half_beam,
        cord,
        card_w,
        card_h,
        cue,
        meter_y,
        grade_r,
        grade_top,
        got,
        miss,
        frog_c,
        frog_r,
        water_y,
    }
}

/// The card rect for one side at a given beam tilt (0 during Choose).
fn card_rect(l: &Lay, tilt: f32, side: u8) -> Rect {
    let (s, c) = tilt.sin_cos();
    let end = if side == 0 {
        vec2(l.pivot.x - l.half_beam * c, l.pivot.y - l.half_beam * s)
    } else {
        vec2(l.pivot.x + l.half_beam * c, l.pivot.y + l.half_beam * s)
    };
    let card_top = end.y + l.cord;
    Rect::new(end.x - l.card_w / 2.0, card_top, l.card_w, l.card_h)
}

// --- finale layout ---------------------------------------------------------

struct FinaleLayout {
    /// Froggy the hero: body center + radius.
    face: Vec2,
    face_r: f32,
    /// The tappable sun (center + radius).
    sun_c: Vec2,
    sun_r: f32,
    balloon_r: f32,
    /// Blossom (lily-pad) radius for the tappable water flowers.
    star_r: f32,
    balloon_anchor: [Vec2; FINALE_BALLOONS],
    /// Blossom positions floating on the pond.
    stars: [Vec2; FINALE_STARS],
    /// The pond's waterline (water fills the screen below it).
    water_y: f32,
    /// Party guests (center, radius): frogs on their own lily pads.
    friends: [(Vec2, f32); 3],
    /// Numbered-flag bunting size.
    flag_s: f32,
}

impl FinaleLayout {
    fn balloon(&self, i: usize, time: f32) -> Vec2 {
        let a = self.balloon_anchor[i];
        let ph = i as f32 * 1.7;
        vec2(
            a.x + 15.0 * (time * 0.5 + ph).sin() + 6.0 * (time * 0.23 + ph).cos(),
            a.y + 17.0 * (time * 0.42 + ph).cos(),
        )
    }
    /// Blossom `i` bobbing on the pond ripples (time-dependent so it truly
    /// floats; the tap hit-test uses the same value so it never desyncs).
    fn star(&self, i: usize, time: f32) -> Vec2 {
        let a = self.stars[i];
        let ph = i as f32 * 1.3;
        vec2(a.x + 4.0 * (time * 0.7 + ph).sin(), a.y + 5.0 * (time * 0.9 + ph).cos())
    }
}

fn finale_layout(f: &crate::layout::Frame) -> FinaleLayout {
    let cx = f.w / 2.0;
    let water_y = f.h * 0.60;
    let pondh = f.h - water_y;
    let face_r = (f.w * 0.058).clamp(36.0, 78.0);
    // Froggy stands front-and-center, near the top of the pond.
    let face = vec2(cx, water_y + pondh * 0.34);
    // The sun tucked in the top-right sky (tappable → rays + twinkle).
    let sun_c = vec2(f.w * 0.90, f.h * 0.15);
    let sun_r = f.vmin(0.065).max(32.0);
    let balloon_r = (f.w * 0.033).clamp(20.0, 40.0);
    let star_r = (f.w * 0.030).clamp(16.0, 36.0);
    let flag_s = f.vmin(0.10).clamp(38.0, 72.0);

    // Balloons drift in the mid-sky band — below the bunting, above the pond, so
    // they fill the space rather than crowding the flags at the very top.
    let bspots = [(0.12, 0.60), (0.28, 0.80), (0.72, 0.80), (0.88, 0.60), (0.50, 0.52)];
    let mut balloon_anchor = [Vec2::ZERO; FINALE_BALLOONS];
    for (b, (fx, fy)) in balloon_anchor.iter_mut().zip(bspots.iter()) {
        *b = vec2(f.w * fx, water_y * fy);
    }

    // Party guests on their own pads, flanking + behind Froggy.
    let fr = face_r * 0.66;
    let friends = [
        (vec2(cx - face_r * 2.5, water_y + pondh * 0.22), fr),
        (vec2(cx + face_r * 2.6, water_y + pondh * 0.46), fr * 0.9),
        (vec2(cx - face_r * 1.5, water_y + pondh * 0.66), fr * 0.72),
    ];

    // Blossoms scattered across the lower pond, clear of the hero + guests.
    let sspots = [
        (0.08, 0.56), (0.22, 0.84), (0.36, 0.62), (0.46, 0.90),
        (0.60, 0.88), (0.72, 0.60), (0.84, 0.84), (0.93, 0.56),
    ];
    let mut stars = [Vec2::ZERO; FINALE_STARS];
    for (s, (fx, fy)) in stars.iter_mut().zip(sspots.iter()) {
        *s = vec2(f.w * fx, water_y + pondh * fy);
    }

    FinaleLayout { face, face_r, sun_c, sun_r, balloon_r, star_r, balloon_anchor, stars, water_y, friends, flag_s }
}

/// A frog's tap reaction, cycled by `kind` for variety so repeat pokes surprise:
/// 0 = a big hop with the tongue out, 1 = a full happy spin, 2 = a wink + squish.
/// `p` runs 0..1 through the beat; `r` is the frog's body radius.
fn react_pose(kind: u8, p: f32, r: f32) -> draw::FrogPose {
    let fly = (p * std::f32::consts::PI).sin();
    match kind % 3 {
        0 => draw::FrogPose { dy: -fly * r * 0.9, sy: 1.0 + fly * 0.18, sx: 1.0 - fly * 0.09, tongue: fly, ..Default::default() },
        1 => draw::FrogPose { rot: anim::ease_in_out_cubic(p) * TAU, dy: -fly * r * 0.5, ..Default::default() },
        _ => draw::FrogPose { blink: fly, sx: 1.0 + 0.12 * fly, sy: 1.0 - 0.06 * fly, dy: -fly * r * 0.2, ..Default::default() },
    }
}

/// Step in-flight timers by `dt`, parking each at [`IDLE`] once past `dur`.
fn step_timers(timers: &mut [f32], dt: f32, dur: f32) {
    for s in timers.iter_mut() {
        if *s < dur {
            *s += dt;
        } else {
            *s = IDLE;
        }
    }
}
