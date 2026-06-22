//! Frog's Day: an analog-clock game scoped to what a ~4yo can actually do —
//! whole-hour "o'clock" reading anchored to a daily routine (the skill ladder
//! that *precedes* minute-reading, which is a grade-1+ skill). The frog walks
//! through its day; each round shows an activity (wake/breakfast/play/lunch/
//! snack/dinner/bed) with a TARGET TIME, and the child SETS the clock by
//! dragging the hands. Setting is easier + more engaging than decoding for this
//! age, and it's kinesthetic.
//!
//! Triple-coded target (per the "pictures WITH words" rule): the activity prop
//! (meaning), the hour NUMERAL on a badge (number recognition), and the hand
//! positions. Errorless + monotonic: the hands SNAP to valid positions (the
//! little hand clicks onto the 12 numbers; the big hand to o'clock/half-past),
//! so a hand can never sit "in between", and a wrong setting never penalizes —
//! the child just keeps adjusting until it matches, which auto-checks.
//!
//! Difficulty (parent-set, like Sing Back's tempo) is a SCAFFOLD ramp:
//!   1 `match`    — target number GLOWS on the dial + a faint ghost hand shows
//!                  where to go; only the little hand moves (big hand pinned up).
//!   2 `routine`  — no dial glow/ghost; still little-hand-only.
//!   3 `clock`    — set BOTH hands for o'clock; a mini MODEL clock shows the
//!                  target to copy.
//!   4 `halfpast` — adds half-past targets (set the big hand up OR down).
//!
//! No in-play instruction text (kids can't read it): phase/turn read from the
//! frog, the props, motion + audio. Completing the whole day lands on a calm
//! bedtime FINALE (closure + payoff), then offers replay. The only persisted +
//! synced thing is the highest difficulty completed (`core::clock::ClockState`);
//! the per-session day progress is a plain field. Sync wiring mirrors Sing Back
//! (pull+merge on mount, push on a new best, flush on every leave path).
use crate::{
    anim, chrome, draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
    text,
};
use fountouki_core::{clock as ck, rng::Mulberry32, settings::load_clock};
use macroquad::prelude::*;
use nanoserde::SerJson;
use std::f32::consts::{PI, TAU};

/// One routine event: the hour the little hand points to, and the activity prop
/// (a bundled emoji sprite — same set the patterns themes use). The half-past
/// variant (level 4) overrides the minute per event via [`HALF_MINUTES`].
struct Ev {
    hour: u8,
    glyph: &'static str,
}

/// The frog's day, in order — a coherent ~7-round arc that lands at bedtime.
/// Same activities at every level; only the TARGET MINUTES differ (o'clock for
/// levels 1–3, the half-past mix below for level 4).
const DAY: [Ev; 7] = [
    Ev { hour: 7, glyph: "☀️" },  // wake
    Ev { hour: 8, glyph: "🥚" },  // breakfast
    Ev { hour: 10, glyph: "🪁" }, // play
    Ev { hour: 12, glyph: "🍎" }, // lunch
    Ev { hour: 3, glyph: "🍌" },  // snack
    Ev { hour: 6, glyph: "🍉" },  // dinner
    Ev { hour: 8, glyph: "🌙" },  // bed
];
/// Level-4 target minutes per event (a mix of o'clock + half-past). Bedtime
/// stays a clean o'clock so the day closes on a tidy 8:00.
const HALF_MINUTES: [u8; 7] = [0, 30, 0, 30, 0, 30, 0];

/// Phase durations (seconds).
const PRESENT_DUR: f32 = 0.9; // the lead-in: show the activity + target, hands frozen
const REWARD_DUR: f32 = 1.3; // celebrate a matched clock before the next event
const ENTRY_FADE: f32 = 0.5; // first-event scene-entry dim-bloom (orient before play)

/// Reward confetti burst.
const REWARD_BURST_N: usize = 90;
/// Finale opening burst + gentle rain cadence.
const FINALE_BURST_N: usize = 150;
const RAIN_INTERVAL_S: f32 = 0.12;
/// Star-pop curve shared by reward + finale (springy overshoot, capped).
const STAR_POP_DUR: f32 = 0.42;
const STAR_POP_CAP: f32 = 1.25;

/// Confetti seed salts (kept independent of the gameplay RNG so goldens stay
/// reproducible — same scheme as Sing Back).
const CONFETTI_SEED_SALT: u32 = 0x9E37_79B9;
const CONFETTI_RESTART_SALT: u32 = 0x85EB_CA6B;

/// Finale tap targets (distinct ids so the per-target debounce only swallows a
/// same-target re-fire). Stars use `TGT_STAR_BASE + i`.
const TGT_FINALE_REPLAY: u32 = 1;
const TGT_FINALE_HOME: u32 = 2;
const TGT_FINALE_FROG: u32 = 3;
const TGT_STAR_BASE: u32 = 10;
/// How many twinkling stars dot the finale sky (tappable, errorless).
const FINALE_STARS: usize = 12;

/// Which hand the finger grabbed this press.
#[derive(PartialEq, Clone, Copy)]
enum Hand {
    Hour,
    Minute,
}

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    /// Lead-in: the activity + target are shown, the hands sit at their (wrong)
    /// start positions and are NOT draggable. `t` seconds in; at `PRESENT_DUR`
    /// the hands go live (Set). The very first event also runs an entry fade.
    Present { t: f32 },
    /// The child sets the clock: drag a hand → it snaps to a valid position;
    /// when both hands match the target, auto-advance to Reward.
    Set,
    /// A matched clock: the frog does the activity, confetti, the day advances.
    /// `t` seconds in; at `REWARD_DUR` the next event presents (or the Finale).
    Reward { t: f32 },
    /// The whole day is done: a calm bedtime celebration. `t` seconds in.
    Finale { t: f32 },
}

pub struct ClockScene {
    db: Db,
    seed: u32,
    rng: Mulberry32,
    state: ck::ClockState,
    /// Difficulty level 1..=4 (from the parent setting); fixed for the session.
    level: u32,
    /// Index into [`DAY`] of the current event.
    ev: usize,
    /// Events completed this session (drives the progress meter). Monotonic.
    done: u32,
    phase: Phase,
    first: bool,
    /// The current hands the child has set: `hour` ∈ 1..=12, `minute` ∈ {0, 30}.
    hour: u8,
    minute: u8,
    /// The hand currently grabbed (Set phase), and whether the pointer was down
    /// last frame (so a press edge is detected from `down` alone — the scripted
    /// play-test pointers set `down` but not `just_pressed`).
    grabbed: Option<Hand>,
    was_down: bool,
    /// Set on the event that raised `best_level` (escalates the finale push).
    new_best: bool,
    rain_acc: f32,
    tap_debounce: input::TapDebounce,
    confetti: crate::confetti::Confetti,
    sync: crate::net::SyncClient,
    // --- finale interaction state ---
    /// Per-star twinkle timer (seconds since tapped, or IDLE).
    star_t: [f32; FINALE_STARS],
    star_taps: u32,
    /// The sleeping frog's stir timer (seconds since tapped, or IDLE).
    frog_t: f32,
    frog_taps: u32,
}

/// Parked timer value meaning "idle" (no animation in flight).
const IDLE: f32 = 99.0;
/// How long a tapped star twinkles / a tapped frog stirs.
const STAR_TWINKLE_S: f32 = 0.6;
const FROG_STIR_S: f32 = 0.8;

fn level_of(difficulty: &str) -> u32 {
    match difficulty {
        "routine" => 2,
        "clock" => 3,
        "halfpast" => 4,
        _ => 1, // "match"
    }
}

impl ClockScene {
    pub fn new(db: Db, seed: u32, now: i64) -> ClockScene {
        let level = {
            let kv = db.borrow_kv();
            level_of(&load_clock(&**kv).difficulty)
        };
        let state = {
            let kv = db.borrow_kv();
            ck::load(&**kv, now)
        };
        let sync = crate::net::SyncClient::new(db.clone(), "clock");
        let mut sc = ClockScene {
            db,
            seed,
            rng: Mulberry32::new(seed),
            state,
            level,
            ev: 0,
            done: 0,
            phase: Phase::Present { t: 0.0 },
            first: true,
            hour: 12,
            minute: 0,
            grabbed: None,
            was_down: false,
            new_best: false,
            rain_acc: 0.0,
            tap_debounce: input::TapDebounce::new(),
            confetti: crate::confetti::Confetti::new(seed.wrapping_add(CONFETTI_SEED_SALT)),
            sync,
            star_t: [IDLE; FINALE_STARS],
            star_taps: 0,
            frog_t: IDLE,
            frog_taps: 0,
        };
        sc.setup_event();
        sc
    }

    fn save(&self) {
        let mut kv = self.db.borrow_kv_mut();
        ck::save(&mut **kv, &self.state);
    }

    /// The target time for the current event at the active level.
    fn target(&self) -> (u8, u8) {
        let e = &DAY[self.ev];
        let m = if self.level >= 4 { HALF_MINUTES[self.ev] } else { 0 };
        (e.hour, m)
    }

    /// True once both hands match the target.
    fn matched(&self) -> bool {
        let (th, tm) = self.target();
        self.hour == th && self.minute == tm
    }

    fn minute_interactive(&self) -> bool {
        self.level >= 3
    }

    /// Place the hands at a deliberately-wrong start so there's always something
    /// to do (the auto-check would otherwise fire instantly). Opens the Present
    /// lead-in for the current event.
    fn setup_event(&mut self) {
        let (th, tm) = self.target();
        // Start hour: a different number (offset by 5 around the dial).
        self.hour = ((th + 5 - 1) % 12) + 1;
        if self.hour == th {
            self.hour = (th % 12) + 1;
        }
        // Start minute: the OTHER of {0,30} when the big hand is in play; else
        // pinned up (levels 1–2 are o'clock-only, big hand fixed at 12).
        self.minute = if self.minute_interactive() {
            if tm == 0 { 30 } else { 0 }
        } else {
            0
        };
        self.grabbed = None;
        self.was_down = false;
        self.phase = Phase::Present { t: 0.0 };
    }

    /// A matched clock: celebrate, advance the day, and (on the last event) fire
    /// the bedtime Finale.
    fn enter_reward(&mut self, ctx: &Ctx) {
        self.done += 1;
        self.grabbed = None;
        let lay = clock_layout(&ctx.frame, self.level);
        self.confetti.burst(lay.face, REWARD_BURST_N, lay.r * 0.9);
        ctx.audio.correct(self.done.saturating_sub(1));
        if self.ev + 1 >= DAY.len() {
            self.enter_finale(ctx);
        } else {
            self.phase = Phase::Reward { t: 0.0 };
        }
    }

    /// The bedtime payoff: the whole day is done. Records the level (a new best
    /// if the parent stepped difficulty up), persists + pushes, throws a calm
    /// night celebration.
    fn enter_finale(&mut self, ctx: &Ctx) {
        let was = self.state.best_level;
        ck::record_level(&mut self.state, self.level, ctx.now);
        self.new_best = self.state.best_level > was;
        self.phase = Phase::Finale { t: 0.0 };
        self.grabbed = None;
        self.tap_debounce = input::TapDebounce::new();
        self.star_t = [IDLE; FINALE_STARS];
        self.star_taps = 0;
        self.frog_t = IDLE;
        self.frog_taps = 0;
        self.rain_acc = 0.0;
        ctx.audio.finale();
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        let fl = finale_layout(&ctx.frame);
        self.confetti.burst(fl.trophy, FINALE_BURST_N, fl.r * 2.2);
    }

    /// Replay from the Finale: back to the first event, day progress cleared,
    /// best_level untouched (monotonic). Fresh confetti stream.
    fn restart(&mut self) {
        self.ev = 0;
        self.done = 0;
        self.new_best = false;
        self.rain_acc = 0.0;
        self.first = false;
        self.confetti =
            crate::confetti::Confetti::new(self.seed.wrapping_add(CONFETTI_RESTART_SALT));
        self.setup_event();
    }

    fn pump_rain(&mut self, dt: f32, w: f32) {
        self.rain_acc += dt;
        while self.rain_acc > RAIN_INTERVAL_S {
            self.confetti.rain(w, -10.0, 1);
            self.rain_acc -= RAIN_INTERVAL_S;
        }
    }

    /// Handle a pointer in the Set phase: grab a hand on a fresh press, rotate
    /// the grabbed hand to the pointer's angle (snapped to valid positions), and
    /// auto-advance when the clock matches the target.
    fn update_set(&mut self, ctx: &Ctx) {
        let pt = ctx.pointer;
        let lay = clock_layout(&ctx.frame, self.level);
        let down = pt.down;
        if down && !self.was_down {
            self.grabbed = self.grab_hand(pt.press_pos, &lay);
        }
        if down {
            if let Some(h) = self.grabbed {
                let a = angle_of(lay.face, pt.pos);
                match h {
                    Hand::Hour => {
                        let nh = snap_hour(a);
                        if nh != self.hour {
                            self.hour = nh;
                            ctx.audio.tap();
                        }
                    }
                    Hand::Minute => {
                        let nm = snap_minute(a);
                        if nm != self.minute {
                            self.minute = nm;
                            ctx.audio.tap();
                        }
                    }
                }
            }
        } else {
            self.grabbed = None;
        }
        self.was_down = down;
        if self.matched() {
            self.enter_reward(ctx);
        }
    }

    /// Which interactive hand a press at `p` grabbed (by closeness to the hand's
    /// drawn segment), or `None` if the press missed the clock face.
    fn grab_hand(&self, p: Vec2, lay: &CLayout) -> Option<Hand> {
        if (p - lay.face).length() > lay.r * 1.15 {
            return None;
        }
        let hour_tip = point_at(lay.face, hour_angle(self.hour), lay.hour_len);
        let dh = dist_point_seg(p, lay.face, hour_tip);
        if !self.minute_interactive() {
            return Some(Hand::Hour);
        }
        let min_tip = point_at(lay.face, minute_angle(self.minute), lay.minute_len);
        let dm = dist_point_seg(p, lay.face, min_tip);
        Some(if dm < dh { Hand::Minute } else { Hand::Hour })
    }

    // --- test hooks (used by --capture / --playtest) ---
    pub(crate) fn level_id(&self) -> u32 {
        self.level
    }
    pub(crate) fn stars(&self) -> u32 {
        self.done
    }
    pub(crate) fn best_level(&self) -> u32 {
        self.state.best_level
    }
    pub(crate) fn day_len(&self) -> usize {
        DAY.len()
    }
    pub(crate) fn in_present(&self) -> bool {
        matches!(self.phase, Phase::Present { .. })
    }
    pub(crate) fn in_set(&self) -> bool {
        matches!(self.phase, Phase::Set)
    }
    pub(crate) fn in_reward(&self) -> bool {
        matches!(self.phase, Phase::Reward { .. })
    }
    pub(crate) fn in_finale(&self) -> bool {
        matches!(self.phase, Phase::Finale { .. })
    }
    pub(crate) fn target_hms(&self) -> (u8, u8) {
        self.target()
    }
    /// The current little-hand tip (a grab point for the play-test).
    pub(crate) fn hour_tip_px(&self, f: &crate::layout::Frame) -> Vec2 {
        let lay = clock_layout(f, self.level);
        point_at(lay.face, hour_angle(self.hour), lay.hour_len)
    }
    /// The current big-hand tip (a grab point for the play-test).
    pub(crate) fn minute_tip_px(&self, f: &crate::layout::Frame) -> Vec2 {
        let lay = clock_layout(f, self.level);
        point_at(lay.face, minute_angle(self.minute), lay.minute_len)
    }
    /// A point at the angle of hour number `h` (drag the little hand here).
    pub(crate) fn number_px(&self, f: &crate::layout::Frame, h: u8) -> Vec2 {
        let lay = clock_layout(f, self.level);
        point_at(lay.face, hour_angle(h), lay.hour_len)
    }
    /// A point at the angle of minute `m` ∈ {0,30} (drag the big hand here).
    pub(crate) fn minute_px(&self, f: &crate::layout::Frame, m: u8) -> Vec2 {
        let lay = clock_layout(f, self.level);
        point_at(lay.face, minute_angle(m), lay.minute_len)
    }
    pub(crate) fn finale_star_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        finale_layout(f).stars[i.min(FINALE_STARS - 1)].0
    }
    pub(crate) fn star_taps(&self) -> u32 {
        self.star_taps
    }
    pub(crate) fn finale_frog_center(&self, f: &crate::layout::Frame) -> Vec2 {
        finale_layout(f).frog
    }
    pub(crate) fn frog_taps(&self) -> u32 {
        self.frog_taps
    }
    pub(crate) fn replay_center(&self, f: &crate::layout::Frame) -> Vec2 {
        chrome::corner_buttons(f).0
    }
}

impl Scene for ClockScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.confetti.update(ctx.dt);
        self.sync.drive(ctx.now);
        if let Some(remote) = self.sync.poll_pull() {
            if let Some(rstate) = ck::validate(&remote) {
                self.state = ck::merge(&self.state, &rstate, ctx.now);
                self.save();
                if self.state != rstate {
                    self.sync.queue_push(&self.state.serialize_json(), ctx.now);
                }
            }
        }

        // The Finale draws no topbar (full-screen night scene) — handle it FIRST
        // and return, so the invisible topbar corners never steal a tap.
        if matches!(self.phase, Phase::Finale { .. }) {
            return self.update_finale(ctx);
        }

        match chrome::handle_topbar(&chrome::topbar(&ctx.frame), ctx, &self.db) {
            Some(chrome::TopbarAction::OpenParent) => {
                self.sync.flush();
                return Nav::OpenParent;
            }
            Some(chrome::TopbarAction::Home) => {
                self.sync.flush();
                return Nav::Home;
            }
            Some(chrome::TopbarAction::MuteToggled) => return Nav::Stay,
            None => {}
        }

        match self.phase {
            Phase::Present { t } => {
                let prev = t;
                let t = t + ctx.dt;
                // One soft twinkle on the reveal (the "now" cue) before play opens.
                if prev <= PRESENT_DUR && t > PRESENT_DUR {
                    ctx.audio.twinkle();
                }
                if t >= PRESENT_DUR {
                    self.first = false;
                    self.phase = Phase::Set;
                } else {
                    self.phase = Phase::Present { t };
                }
            }
            Phase::Set => self.update_set(ctx),
            Phase::Reward { t } => {
                let t = t + ctx.dt;
                if t >= REWARD_DUR {
                    self.ev += 1;
                    self.setup_event();
                } else {
                    self.phase = Phase::Reward { t };
                }
            }
            Phase::Finale { .. } => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        let f = &ctx.frame;
        if let Phase::Finale { t } = self.phase {
            self.draw_finale(ctx, t);
            return;
        }

        let lay = clock_layout(f, self.level);
        // Sky wash by time-of-day (morning → noon → dusk), so the day visibly
        // passes as the events advance.
        let (sky_top, sky_bot) = sky_colors(self.ev);
        draw::vgradient(0.0, 0.0, f.w, f.h, sky_top, sky_bot);

        chrome::draw_topbar(&chrome::topbar(f), ctx);
        self.draw_meter(&lay);
        self.draw_vignette(ctx, &lay);
        self.draw_clock(ctx, &lay);

        // First-event entry fade: a soft dim-bloom so the child can orient before
        // the hands go live (the "lead into the task" rule).
        if let Phase::Present { t } = self.phase {
            if self.first {
                let a = (1.0 - anim::clamp01(t / ENTRY_FADE)) * 0.5;
                if a > 0.001 {
                    draw_rectangle(0.0, 0.0, f.w, f.h, palette::hexa(0x2a1b47, a));
                }
            }
        }

        self.confetti.draw();
    }
}

impl ClockScene {
    /// Progress meter: one little sun per event, filled gold up to `done`.
    fn draw_meter(&self, lay: &CLayout) {
        let n = DAY.len();
        let gap = lay.meter_r * 2.6;
        let total = (n as f32 - 1.0) * gap;
        let cx0 = lay.meter_c.x - total / 2.0;
        for i in 0..n {
            let x = cx0 + i as f32 * gap;
            let on = (i as u32) < self.done;
            if on {
                draw::disc(x, lay.meter_c.y, lay.meter_r * 1.15, palette::hexa(0xffd166, 0.35));
                draw::star(x, lay.meter_c.y, lay.meter_r, palette::GOLD);
            } else {
                draw::disc(x, lay.meter_c.y, lay.meter_r, palette::PIP_EMPTY);
            }
        }
    }

    /// The frog + the activity prop + the hour-number badge (+ the mini MODEL
    /// clock to copy at levels 3–4). This is the "what time is it / what are we
    /// doing" side; the big clock is the "set it" side.
    fn draw_vignette(&self, ctx: &Ctx, lay: &CLayout) {
        let (th, tm) = self.target();
        let c = lay.vignette;
        let r = lay.vig_r;

        // The frog, calm; a small celebratory hop on reward.
        let mut pose = draw::FrogPose { blink: 0.06 * anim::pulse(ctx.time, 3.1).max(0.0), ..Default::default() };
        if let Phase::Reward { t } = self.phase {
            let imp = (t / REWARD_DUR * PI).sin().max(0.0);
            pose.dy = -imp * r * 0.4;
            pose.sy = 1.0 + 0.12 * imp;
            pose.sx = 1.0 - 0.06 * imp;
            pose.tongue = imp * 0.4;
        }
        draw::frog(c.x, c.y + r * 0.2, r * 0.62, palette::RAINBOW[3], pose);

        // The activity prop, popped above the frog's head (the routine cue).
        let prop = r * 0.86;
        let prop_y = c.y - r * 0.85;
        let bob = if matches!(self.phase, Phase::Present { .. }) {
            anim::pulse(ctx.time, 1.6) * r * 0.05
        } else {
            0.0
        };
        draw::disc(c.x, prop_y + bob, prop * 0.62, palette::hexa(0xffffff, 0.7));
        draw_glyph(DAY[self.ev].glyph, c.x, prop_y + bob, prop);

        // The target slot below the frog. Levels 1–2 show a big hour NUMERAL on a
        // card (number recognition — the big clock carries the glow/ghost
        // scaffold); levels 3–4 show a mini MODEL clock to COPY (its dial
        // numerals + both hands convey the exact target, incl. half-past), since
        // the big clock then has no scaffold. One object, so they never collide.
        let slot = lay.model;
        if self.level <= 2 {
            draw::card(
                slot.x - lay.badge_r,
                slot.y - lay.badge_r * 0.7,
                lay.badge_r * 2.0,
                lay.badge_r * 1.4,
                palette::CARD,
            );
            text::draw_centered(
                &th.to_string(),
                slot.x,
                slot.y,
                (lay.badge_r * 1.4) as u16,
                &ctx.fonts.cursive,
                palette::INK,
            );
        } else {
            draw_face(slot, lay.model_r, ctx, true, 0);
            draw_hands(slot, lay.model_r, th, tm, true);
        }
    }

    /// The big interactive clock the child sets.
    fn draw_clock(&self, ctx: &Ctx, lay: &CLayout) {
        let (th, tm) = self.target();
        // Glow the target number on the dial (scaffold, levels 1–2).
        let glow_num = if self.level <= 2 { Some(th) } else { None };
        draw_face(lay.face, lay.r, ctx, true, glow_num.unwrap_or(0));

        // Ghost target hands (level 1 only) — a faint "where to go" trace.
        if self.level == 1 {
            draw_ghost_hands(lay.face, lay.r, th, tm);
        }

        // The set hands. The grabbed hand gets a brighter hub ring.
        draw_hands(lay.face, lay.r, self.hour, self.minute, false);

        // A subtle pulse on the hand the child can move, in Set, to invite a drag
        // (calm — no text "your turn").
        if matches!(self.phase, Phase::Set) {
            let pulse = 0.5 + 0.5 * anim::pulse(ctx.time, 1.4);
            let tip = if self.minute_interactive() && self.grabbed == Some(Hand::Minute) {
                point_at(lay.face, minute_angle(self.minute), lay.minute_len)
            } else {
                point_at(lay.face, hour_angle(self.hour), lay.hour_len)
            };
            draw::disc(tip.x, tip.y, lay.r * (0.06 + 0.02 * pulse), palette::hexa(0xffffff, 0.35 * pulse));
        }
    }

    /// Drive + draw nothing here — the night Finale's per-frame update.
    fn update_finale(&mut self, ctx: &Ctx) -> Nav {
        let Phase::Finale { t } = self.phase else { return Nav::Stay };
        let dt = ctx.dt;
        self.pump_rain(dt, ctx.frame.w);
        for s in self.star_t.iter_mut() {
            if *s < STAR_TWINKLE_S {
                *s += dt;
            } else {
                *s = IDLE;
            }
        }
        if self.frog_t < FROG_STIR_S {
            self.frog_t += dt;
        } else {
            self.frog_t = IDLE;
        }
        self.phase = Phase::Finale { t: t + dt };

        let pt = ctx.pointer;
        if pt.tapped() {
            let fl = finale_layout(&ctx.frame);
            let (replay, home, br) = chrome::corner_buttons(&ctx.frame);
            if input::hit_circle(pt.pos, replay.x, replay.y, br) {
                if self.tap_debounce.accept(TGT_FINALE_REPLAY, ctx.time) {
                    self.restart();
                }
                return Nav::Stay;
            }
            if input::hit_circle(pt.pos, home.x, home.y, br) {
                if self.tap_debounce.accept(TGT_FINALE_HOME, ctx.time) {
                    self.sync.flush();
                    return Nav::Home;
                }
                return Nav::Stay;
            }
            if input::hit_circle(pt.pos, fl.frog.x, fl.frog.y, fl.frog_r * 1.2)
                && self.frog_t >= FROG_STIR_S
                && self.tap_debounce.accept(TGT_FINALE_FROG, ctx.time)
            {
                self.frog_t = 0.0;
                self.frog_taps += 1;
                ctx.audio.frog();
                return Nav::Stay;
            }
            for i in 0..FINALE_STARS {
                let (c, rr) = fl.stars[i];
                if input::hit_circle(pt.pos, c.x, c.y, rr * 2.0)
                    && self.tap_debounce.accept(TGT_STAR_BASE + i as u32, ctx.time)
                {
                    self.star_t[i] = 0.0;
                    self.star_taps += 1;
                    ctx.audio.twinkle();
                    self.confetti.burst(c, 10, rr * 1.5);
                    return Nav::Stay;
                }
            }
        }
        Nav::Stay
    }

    /// The bedtime Finale: a calm night sky, a moon, twinkling stars, the frog
    /// asleep, a big trophy star, gentle confetti, and corner replay/home.
    fn draw_finale(&self, ctx: &Ctx, t: f32) {
        let f = &ctx.frame;
        let fl = finale_layout(f);
        draw::vgradient(0.0, 0.0, f.w, f.h, palette::hex(0x1b1c40), palette::hex(0x4a3a6b));

        // The moon, with a soft halo.
        draw::disc(fl.moon.x, fl.moon.y, fl.moon_r * 1.5, palette::hexa(0xfff3a8, 0.12));
        draw::disc(fl.moon.x, fl.moon.y, fl.moon_r, palette::hex(0xfff3c8));
        draw::disc(fl.moon.x + fl.moon_r * 0.35, fl.moon.y - fl.moon_r * 0.2, fl.moon_r * 0.82, palette::hex(0x4a3a6b));

        // Twinkling stars (tap → a brighter pop).
        for (i, &(c, rr)) in fl.stars.iter().enumerate() {
            let base = 0.55 + 0.45 * (t * 1.7 + i as f32 * 1.3).sin();
            let pop = if self.star_t[i] < STAR_TWINKLE_S {
                1.0 + 0.9 * (1.0 - self.star_t[i] / STAR_TWINKLE_S)
            } else {
                1.0
            };
            draw::star(c.x, c.y, rr * pop, palette::hexa(0xfff3c8, (0.5 + 0.5 * base).min(1.0)));
        }

        // The sleeping frog (eyes shut), with a gentle breathing + a tapped stir.
        let breathe = 1.0 + 0.04 * anim::pulse(t, 3.4).max(0.0);
        let mut pose = draw::FrogPose { blink: 1.0, sy: breathe, sx: 2.0 - breathe, ..Default::default() };
        if self.frog_t < FROG_STIR_S {
            let imp = (self.frog_t / FROG_STIR_S * PI).sin();
            pose.rot = (self.frog_t * 18.0).sin() * 0.12 * (1.0 - self.frog_t / FROG_STIR_S);
            pose.dy = -imp * fl.frog_r * 0.18;
            pose.tongue = imp * 0.3;
        }
        draw::frog(fl.frog.x, fl.frog.y, fl.frog_r, palette::RAINBOW[3], pose);
        // "zzz" floating up from the frog.
        for k in 0..3 {
            let zt = (t * 0.8 + k as f32 * 0.4) % 1.0;
            let zx = fl.frog.x + fl.frog_r * (0.7 + zt * 0.5);
            let zy = fl.frog.y - fl.frog_r * (0.6 + zt * 1.1);
            text::draw_centered("z", zx, zy, (fl.frog_r * (0.3 + zt * 0.2)) as u16, &ctx.fonts.cursive, palette::hexa(0xffffff, (1.0 - zt) * 0.8));
        }

        // The trophy star, bottom-anchored, throbbing.
        let pop = anim::back_out((t / STAR_POP_DUR).clamp(0.0, 1.0)).min(STAR_POP_CAP);
        let throb = 1.0 + 0.06 * anim::pulse(t, 0.9).max(0.0);
        let r = fl.r * pop * throb;
        // Radiating sparkle points (a slow twinkle) instead of a backing disc —
        // a half-opaque gold disc over the dark sky browns out to a dull ring, so
        // the radiance is carried by bright opaque sparkles (per the finale notes).
        for i in 0..8 {
            let a = t * 0.6 + i as f32 * (TAU / 8.0);
            let far = if i % 2 == 0 { 1.5 } else { 1.9 };
            let breathe = 0.5 + 0.5 * (t * 2.4 + i as f32 * 0.8).sin();
            let sp = fl.trophy + vec2(a.cos(), a.sin()) * r * far;
            let pr = r * (0.10 + 0.06 * breathe);
            draw::disc(sp.x, sp.y, pr, palette::hexa(0xffe066, 0.95));
            draw::disc(sp.x, sp.y, pr * 0.45, palette::hexa(0xfffcea, 0.95));
        }
        draw::star(fl.trophy.x, fl.trophy.y, r, palette::GOLD);
        draw::star(fl.trophy.x, fl.trophy.y, r * 0.62, palette::hexa(0xffe066, 0.9));

        self.confetti.draw();
        let (replay, home, br) = chrome::corner_buttons(f);
        chrome::draw_corner_buttons(replay, home, br);
    }
}

// --- geometry helpers -------------------------------------------------------

/// Screen angle (radians, y-down; up = -PI/2) the little hand makes for hour `h`.
fn hour_angle(h: u8) -> f32 {
    let h = (h % 12) as f32; // 12 → 0 (straight up)
    -PI / 2.0 + h / 12.0 * TAU
}
/// Screen angle the big hand makes for minute `m`.
fn minute_angle(m: u8) -> f32 {
    -PI / 2.0 + (m as f32 / 60.0) * TAU
}
/// The angle from `center` to point `p`.
fn angle_of(center: Vec2, p: Vec2) -> f32 {
    (p.y - center.y).atan2(p.x - center.x)
}
/// Snap an angle to the nearest hour number (1..=12).
fn snap_hour(a: f32) -> u8 {
    let raw = (a + PI / 2.0).rem_euclid(TAU);
    let h = (raw / TAU * 12.0).round() as i32 % 12;
    if h == 0 {
        12
    } else {
        h as u8
    }
}
/// Snap an angle to the nearer of o'clock (0) / half-past (30).
fn snap_minute(a: f32) -> u8 {
    let da = ang_dist(a, minute_angle(0));
    let db = ang_dist(a, minute_angle(30));
    if db < da {
        30
    } else {
        0
    }
}
/// Smallest absolute angular distance between two angles.
fn ang_dist(a: f32, b: f32) -> f32 {
    let mut d = (a - b).rem_euclid(TAU);
    if d > PI {
        d = TAU - d;
    }
    d.abs()
}
fn point_at(center: Vec2, angle: f32, len: f32) -> Vec2 {
    center + vec2(angle.cos(), angle.sin()) * len
}
/// Distance from point `p` to the segment `a`–`b`.
fn dist_point_seg(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() <= 1e-6 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).length()
}

// --- clock drawing ----------------------------------------------------------

/// Draw a clock face: rim, hour ticks, and the numerals 1..12. `numerals` keeps
/// the mini model clock clean (no numerals); `glow_num` (1..=12, 0 = none) lights
/// one numeral as the level-1/2 target scaffold.
fn draw_face(c: Vec2, r: f32, ctx: &Ctx, numerals: bool, glow_num: u8) {
    draw::disc(c.x, c.y, r * 1.06, palette::hexa(0x2b2c34, 0.10)); // soft shadow ring
    draw::disc(c.x, c.y, r, palette::hex(0xe3b96a)); // honey rim
    draw::disc(c.x, c.y, r * 0.93, palette::CARD); // face

    // Hour ticks.
    for h in 1..=12u8 {
        let a = hour_angle(h);
        let o = point_at(c, a, r * 0.88);
        let i = point_at(c, a, r * 0.80);
        draw::stroke_path(&[i, o], (r * 0.02).max(2.0), palette::MUTED);
    }
    if numerals {
        for h in 1..=12u8 {
            let a = hour_angle(h);
            let p = point_at(c, a, r * 0.70);
            if glow_num == h {
                draw::disc(p.x, p.y, r * 0.15, palette::hexa(0xffd166, 0.85));
            }
            text::draw_centered(
                &h.to_string(),
                p.x,
                p.y,
                (r * 0.22).max(10.0) as u16,
                &ctx.fonts.cursive,
                palette::INK,
            );
        }
    }
}

/// Draw the two hands + the hub. `ghost` is handled by [`draw_ghost_hands`]; this
/// is the solid set/model pair: a short thick little hand (warm) + a long thin
/// big hand (cool), so big/little read at a glance.
fn draw_hands(c: Vec2, r: f32, hour: u8, minute: u8, model: bool) {
    let hour_len = r * 0.50;
    let min_len = r * 0.80;
    let big = point_at(c, minute_angle(minute), min_len);
    let little = point_at(c, hour_angle(hour), hour_len);
    let scale = if model { 0.8 } else { 1.0 };
    draw::stroke_path(&[c, big], (r * 0.045 * scale).max(2.0), palette::RAINBOW[4]);
    draw::stroke_path(&[c, little], (r * 0.075 * scale).max(3.0), palette::hex(0xe85c6b));
    draw::disc(c.x, c.y, (r * 0.06).max(3.0), palette::INK);
}

/// Faint target hands (level-1 scaffold): the same geometry as [`draw_hands`],
/// drawn translucent so the child traces onto them.
fn draw_ghost_hands(c: Vec2, r: f32, hour: u8, minute: u8) {
    let big = point_at(c, minute_angle(minute), r * 0.80);
    let little = point_at(c, hour_angle(hour), r * 0.50);
    draw::stroke_path(&[c, big], (r * 0.045).max(2.0), palette::hexa(0x38b3e2, 0.35));
    draw::stroke_path(&[c, little], (r * 0.075).max(3.0), palette::hexa(0xe85c6b, 0.35));
}

/// Draw a bundled emoji sprite centered at `(cx, cy)`, sized `size`×`size`.
fn draw_glyph(g: &str, cx: f32, cy: f32, size: f32) {
    if let Some(tex) = crate::emoji::texture(g) {
        draw_texture_ex(
            &tex,
            cx - size / 2.0,
            cy - size / 2.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(size, size)), ..Default::default() },
        );
    }
}

/// Sky gradient by how far into the day we are (event index): morning peach →
/// midday blue → dusky amber.
fn sky_colors(ev: usize) -> (Color, Color) {
    let f = (ev as f32 / (DAY.len() as f32 - 1.0)).clamp(0.0, 1.0);
    // Three stops blended: morning → noon → dusk.
    let morning = (palette::hex(0xffe0c2), palette::hex(0xfff3df));
    let noon = (palette::hex(0xcdefff), palette::hex(0xe6f6ff));
    let dusk = (palette::hex(0xff9d7e), palette::hex(0xffe7c4));
    let (a, b) = if f < 0.5 {
        let u = f / 0.5;
        (mix(morning.0, noon.0, u), mix(morning.1, noon.1, u))
    } else {
        let u = (f - 0.5) / 0.5;
        (mix(noon.0, dusk.0, u), mix(noon.1, dusk.1, u))
    };
    (a, b)
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        anim::lerp(a.r, b.r, t),
        anim::lerp(a.g, b.g, t),
        anim::lerp(a.b, b.b, t),
        1.0,
    )
}

// --- layout -----------------------------------------------------------------

struct CLayout {
    face: Vec2,
    r: f32,
    hour_len: f32,
    minute_len: f32,
    vignette: Vec2,
    vig_r: f32,
    badge_r: f32,
    model: Vec2,
    model_r: f32,
    meter_c: Vec2,
    meter_r: f32,
}

fn clock_layout(f: &crate::layout::Frame, _level: u32) -> CLayout {
    let tb = f.topbar();
    let content = f.content();
    let region_top = tb.y + tb.h;
    let region_bot = content.y + content.h;
    let region_h = region_bot - region_top;

    let meter_r = (f.vmin(0.014)).clamp(7.0, 14.0);
    let meter_c = vec2(f.w / 2.0, region_top + meter_r + 6.0);
    let play_top = meter_c.y + meter_r + f.vmin(0.03);

    let r = (f.vmin(0.30)).clamp(90.0, 320.0);
    let (face, vignette, vig_r);
    if f.is_portrait() {
        // Vignette up top, clock below; the target slot sits between them.
        let cx = f.w / 2.0;
        vig_r = (f.vmin(0.15)).clamp(54.0, 140.0);
        vignette = vec2(cx, play_top + vig_r * 1.15);
        let slot = vec2(cx, vignette.y + vig_r * 1.5);
        let model_r = vig_r * 0.6;
        let r = r.min((region_bot - (slot.y + model_r) - 18.0).max(80.0));
        face = vec2(cx, region_bot - r - 16.0);
        return CLayout {
            face,
            r,
            hour_len: r * 0.50,
            minute_len: r * 0.80,
            vignette,
            vig_r,
            badge_r: vig_r * 0.5,
            model: slot,
            model_r,
            meter_c,
            meter_r,
        };
    }
    // Landscape: clock right, vignette + target slot stacked on the left.
    let cy = (play_top + region_bot) / 2.0;
    let r = r.min((region_h * 0.42).max(80.0)).min((f.w * 0.28).max(80.0));
    face = vec2(f.w * 0.66, cy);
    vig_r = (f.vmin(0.155)).clamp(56.0, 140.0);
    vignette = vec2(f.w * 0.24, cy - vig_r * 0.55);
    CLayout {
        face,
        r,
        hour_len: r * 0.50,
        minute_len: r * 0.80,
        vignette,
        vig_r,
        badge_r: vig_r * 0.5,
        model: vec2(f.w * 0.24, vignette.y + vig_r * 1.5),
        model_r: vig_r * 0.62,
        meter_c,
        meter_r,
    }
}

struct FinaleLayout {
    moon: Vec2,
    moon_r: f32,
    stars: [(Vec2, f32); FINALE_STARS],
    frog: Vec2,
    frog_r: f32,
    trophy: Vec2,
    r: f32,
}

fn finale_layout(f: &crate::layout::Frame) -> FinaleLayout {
    let vmin = f.vmin(1.0);
    let moon_r = (vmin * 0.10).clamp(36.0, 110.0);
    let moon = vec2(f.w * 0.80, f.h * 0.22 + f.safe.top);
    let frog_r = (vmin * 0.16).clamp(60.0, 170.0);
    let frog = vec2(f.w * 0.5, f.h * 0.62);
    let r = (vmin * 0.11).clamp(40.0, 120.0);
    let trophy = vec2(f.w * 0.5, f.h * 0.30);

    // Scattered stars on a fixed pseudo-random spread (deterministic for goldens).
    let specs: [(f32, f32, f32); FINALE_STARS] = [
        (0.10, 0.16, 0.9),
        (0.22, 0.40, 0.6),
        (0.31, 0.12, 1.1),
        (0.43, 0.28, 0.7),
        (0.58, 0.14, 0.8),
        (0.66, 0.36, 1.0),
        (0.74, 0.10, 0.6),
        (0.88, 0.30, 0.9),
        (0.16, 0.62, 0.7),
        (0.36, 0.70, 0.6),
        (0.84, 0.58, 0.8),
        (0.92, 0.46, 0.6),
    ];
    let base = (vmin * 0.02).clamp(7.0, 18.0);
    let mut stars = [(Vec2::ZERO, base); FINALE_STARS];
    for (i, &(fx, fy, sz)) in specs.iter().enumerate() {
        stars[i] = (vec2(f.w * fx, f.h * fy), base * sz);
    }
    FinaleLayout { moon, moon_r, stars, frog, frog_r, trophy, r }
}

// --- capture construction (goldens) -----------------------------------------

pub(crate) enum CaptureState {
    /// Level-1 "match": the dial target number glows, a ghost hand shows the way,
    /// the little hand mid-set (big hand pinned up).
    SetMatch,
    /// Level-3 "clock": both hands in play + the mini model clock to copy.
    SetClock,
    /// Level-4 "halfpast": a half-past target (big hand down) + the model clock.
    SetHalfpast,
    /// A matched clock mid-celebration (confetti + frog hop).
    Reward,
    /// The bedtime night Finale.
    Finale,
}

impl ClockScene {
    /// Build the scene pinned into `cap` for a golden capture (fixed seed). The
    /// difficulty is read from `db`, so the caller writes the matching
    /// `ClockSettings` before constructing (mirrors patterns/singback captures).
    pub(crate) fn capture(db: Db, seed: u32, now: i64, cap: CaptureState, ctx0: &Ctx) -> ClockScene {
        let mut sc = ClockScene::new(db, seed, now);
        match cap {
            CaptureState::SetMatch => {
                // Level 1: open play, little hand one step shy of the glowing
                // target (clearly mid-set); big hand pinned up.
                sc.phase = Phase::Set;
                sc.first = false;
                let (th, _) = sc.target();
                sc.hour = if th == 1 { 12 } else { th - 1 };
            }
            CaptureState::SetClock => {
                // Level 3: little hand already on the number, big hand still to be
                // raised to o'clock — shows the "set the other hand" beat + the
                // model clock to copy.
                sc.phase = Phase::Set;
                sc.first = false;
                let (th, _) = sc.target();
                sc.hour = th; // placed
                sc.minute = 30; // start (wrong) → child raises it to 12
            }
            CaptureState::SetHalfpast => {
                // Level 4: a half-past event (8:30) — the model shows the big hand
                // DOWN; the big clock is mid-set (hand still up) for contrast.
                sc.ev = 1; // breakfast → 8:30 at level 4
                sc.setup_event();
                sc.phase = Phase::Set;
                sc.first = false;
                let (th, _) = sc.target();
                sc.hour = th; // placed
                sc.minute = 0; // start (wrong) → child lowers it to half-past
            }
            CaptureState::Reward => {
                sc.enter_reward(ctx0);
                drive(&mut sc, ctx0, 8);
            }
            CaptureState::Finale => {
                // Jump to the last event and complete it to reach the Finale.
                sc.ev = DAY.len() - 1;
                sc.done = (DAY.len() - 1) as u32;
                sc.enter_finale(ctx0);
                drive(&mut sc, ctx0, 12);
            }
        }
        sc
    }
}

/// Step `sc` `n` idle frames to land a representative mid-animation capture.
fn drive(sc: &mut ClockScene, ctx0: &Ctx, n: usize) {
    let idle = crate::input::Pointer::default();
    for _ in 0..n {
        let ctx = Ctx {
            dt: 0.03,
            time: ctx0.time,
            now: ctx0.now,
            pointer: &idle,
            frame: ctx0.frame,
            fonts: ctx0.fonts,
            audio: ctx0.audio,
        };
        sc.update(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_snapping_round_trips() {
        for h in 1..=12u8 {
            assert_eq!(snap_hour(hour_angle(h)), h, "hour {h}");
        }
    }

    #[test]
    fn minute_snapping_picks_nearest() {
        assert_eq!(snap_minute(minute_angle(0)), 0);
        assert_eq!(snap_minute(minute_angle(30)), 30);
        // Just past 12 toward 3 still rounds to o'clock; near 6 to half-past.
        assert_eq!(snap_minute(minute_angle(10)), 0);
        assert_eq!(snap_minute(minute_angle(40)), 30);
    }

    #[test]
    fn level_mapping() {
        assert_eq!(level_of("match"), 1);
        assert_eq!(level_of("routine"), 2);
        assert_eq!(level_of("clock"), 3);
        assert_eq!(level_of("halfpast"), 4);
        assert_eq!(level_of("garbage"), 1);
    }
}
