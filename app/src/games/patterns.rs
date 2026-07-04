//! Patterns: "what comes next?" (next mode) over a repeating sequence. Pick the
//! item that fills the pink `?` slot. Errorless (wrong answers shake + let you
//! retry); monotonic stars + level pips. Round generation lives in
//! `fountouki_core::patterns`; this is the rendering + interaction shell.
//!
//! Unit mode (select the repeating piece) is tracked separately — this builds
//! `next` mode first; unit mode falls back to next for now.
use crate::{
    chrome, draw, input,
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

/// Consecutive correct answers needed to level up. The streak resets on a
/// wrong answer, so a level only advances on a clean run (mastery), never on a
/// mistake-then-correct. Stars stay monotonic regardless. A clean streak *at*
/// `MAX_LEVEL` fires the finale instead of leveling up (you beat the last level).
///
/// A short gate (2) keeps the session brief — climbing all six difficulty tiers
/// to the train takes ~12 clean answers, in line with the newer games' ~5-min
/// arc (the old 4-streak needed 24, which dragged for a 4yo).
const LEVEL_UP_STREAK: u32 = 2;
const ADVANCE_DELAY: f32 = 0.7;
const RETRY_DELAY: f32 = 0.55;
/// Level-up drive-by: how long the mini train takes to cross the screen. Kept
/// snappy — it fires on every level-up (five per session), so a long crossing
/// piled up dead time.
const DRIVE_DUR: f32 = 2.0;

// --- Pattern Train finale: interactive tap targets --------------------------
// The finale mirrors the newer games (compare/clock/singback): several
// INDEPENDENT tap targets, each with its own debounce id, tap counter, and a
// short reaction timer that parks at `IDLE_T` when idle. Errorless + infinitely
// re-tappable; nothing escalates.

/// Parked timer value meaning "idle" (no reaction in flight).
const IDLE_T: f32 = 99.0;
/// Max cars we size the per-car bounce-timer array for (period 3 × 2 reps = 6 is
/// the real max; 12 leaves generous headroom).
const FINALE_MAX_CARS: usize = 12;
/// Party balloons bobbing in the finale sky.
const FINALE_BALLOONS: usize = 4;
/// Meadow flowers in the finale foreground (tablet only; tap → bloom).
const FINALE_FLOWERS: usize = 5;
/// Foreground flower placements: (x fraction of content, y fraction of the
/// ground band, vmin size). Shared by draw + hit-testing so taps line up.
const FINALE_FLOWER_POS: [(f32, f32, f32); FINALE_FLOWERS] = [
    (0.10, 0.46, 0.055),
    (0.22, 0.60, 0.042),
    (0.39, 0.50, 0.050),
    (0.55, 0.62, 0.040),
    (0.70, 0.48, 0.052),
];

/// World-space (root position, size) for finale meadow flower `i`; `by` is the
/// finale ground line.
fn finale_flower(f: &crate::layout::Frame, by: f32, i: usize) -> (Vec2, f32) {
    let content = f.content();
    let (fx, fy, fs) = FINALE_FLOWER_POS[i];
    (vec2(content.x + content.w * fx, by + (f.h - by) * fy), f.vmin(fs))
}

/// Reaction durations (seconds).
const SUN_FLARE_S: f32 = 0.9;
const FLAG_WAVE_S: f32 = 0.8;
const CAR_BOUNCE_S: f32 = 0.5;
const BALLOON_BOB_S: f32 = 0.8;
const FLOWER_POP_S: f32 = 0.5;

/// Finale tap-target ids (distinct so the per-target debounce only swallows a
/// same-target re-fire — a fast tap on a different target always lands).
const TGT_REPLAY: u32 = 1;
const TGT_HOME: u32 = 2;
const TGT_ENGINE: u32 = 3;
const TGT_SUN: u32 = 4;
const TGT_FLAG: u32 = 5;
const TGT_CAR_BASE: u32 = 20;
const TGT_BALLOON_BASE: u32 = 50;
const TGT_FLOWER_BASE: u32 = 80;

/// Which scene we're in: the round-by-round game, or the train celebration that
/// crowns mastering the final level.
#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Play,
    Finale,
}

pub struct PatternsScene {
    db: Db,
    rng: Mulberry32,
    theme_choice: ThemeChoice,
    difficulty: Difficulty,
    mode: GameMode,
    show_hint: bool,
    phase: Phase,
    pub level: u32,
    pub stars: u32,
    streak: u32,
    round: Round,
    selected: Option<usize>,
    result: Option<bool>, // Some(true)=correct, Some(false)=wrong
    fb_time: f32,
    advance_in: Option<f32>,
    /// Unit mode: the currently-selected contiguous cell range [start, end).
    sel: Option<(usize, usize)>,
    confetti: crate::confetti::Confetti,
    // --- finale (the Pattern Train) ---
    /// Seconds since the finale was entered (drives the entrance + celebration).
    finale_t: f32,
    /// The kid's just-solved pattern, expanded over a few repetitions — one item
    /// per train car, read left→right. The layout caps how many actually fit.
    cars: Vec<Item>,
    /// Period of the pattern on the cars (template length), for legibility hints.
    car_period: usize,
    /// Seconds since the engine was last tapped (drives the reaction; large = idle).
    react_t: f32,
    /// Which engine reaction is playing (cycles, like the frog — does not escalate).
    react_kind: usize,
    /// Total engine taps this finale (selects + cycles the reaction).
    engine_taps: u32,
    /// Accumulator for the steady confetti-rain trickle.
    rain_acc: f32,
    /// Per-target tap debounce for the finale's many tap targets.
    tap_debounce: crate::input::TapDebounce,
    /// Sun tap reaction (rays flare + pop); parks at `IDLE_T`.
    sun_t: f32,
    sun_taps: u32,
    /// Finish-flag tap reaction (excited flutter + finial pop); parks at `IDLE_T`.
    flag_t: f32,
    flag_taps: u32,
    /// Per-car bounce reaction — poke your own mastered pattern and each piece
    /// boings. Indexed by car; parks at `IDLE_T`.
    car_t: [f32; FINALE_MAX_CARS],
    car_taps: u32,
    /// Party balloons bobbing in the sky (tap → pop-wobble); park at `IDLE_T`.
    balloon_t: [f32; FINALE_BALLOONS],
    balloon_taps: u32,
    /// Meadow flowers (tap → the bloom springs up); park at `IDLE_T`.
    flower_t: [f32; FINALE_FLOWERS],
    flower_taps: u32,
    // --- level-up drive-by (a mini Pattern Train crosses the bottom) ---
    /// Seconds since a level-up fired the drive-by; `None` when parked offstage.
    drive_t: Option<f32>,
    /// The just-solved unit riding the drive-by cars (one item per car).
    drive_items: Vec<Item>,
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
            phase: Phase::Play,
            level: 1,
            stars: 0,
            streak: 0,
            round,
            selected: None,
            result: None,
            fb_time: 0.0,
            advance_in: None,
            sel: None,
            confetti: crate::confetti::Confetti::new(seed ^ 0x00c0_ffee),
            finale_t: 0.0,
            cars: Vec::new(),
            car_period: 1,
            react_t: 99.0,
            react_kind: 0,
            engine_taps: 0,
            rain_acc: 0.0,
            tap_debounce: crate::input::TapDebounce::new(),
            sun_t: IDLE_T,
            sun_taps: 0,
            flag_t: IDLE_T,
            flag_taps: 0,
            car_t: [IDLE_T; FINALE_MAX_CARS],
            car_taps: 0,
            balloon_t: [IDLE_T; FINALE_BALLOONS],
            balloon_taps: 0,
            flower_t: [IDLE_T; FINALE_FLOWERS],
            flower_taps: 0,
            drive_t: None,
            drive_items: Vec::new(),
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
        ctx.audio.correct(self.streak);
        if self.streak >= LEVEL_UP_STREAK {
            if self.level < MAX_LEVEL {
                self.streak = 0;
                self.level += 1;
                ctx.audio.level_up();
                // Level-up spectacle: a mini Pattern Train carrying the unit the
                // kid just mastered drives across the bottom — a taste of the
                // finale that gets closer with every level. When it will actually
                // show, hold the next round until it parks: the spectacle owns
                // its moment and never competes with the new level's first trial.
                self.drive_t = Some(0.0);
                self.drive_items = unit_sequence(&self.round);
                if drive_band(&ctx.frame, &p, self.mode).is_some() {
                    self.result = Some(true);
                    self.advance_in = Some(DRIVE_DUR);
                    return;
                }
            } else {
                // Mastered the final level on a clean streak → All aboard!
                self.enter_finale(ctx);
                return;
            }
        }
        self.result = Some(true);
        self.advance_in = Some(ADVANCE_DELAY);
    }

    /// Flip to the train celebration: capture the just-solved pattern as the
    /// train's cargo, fire the grand fanfare + an opening confetti burst.
    fn enter_finale(&mut self, ctx: &Ctx) {
        self.phase = Phase::Finale;
        self.drive_t = None;
        self.finale_t = 0.0;
        self.react_t = 99.0;
        self.react_kind = 0;
        self.engine_taps = 0;
        self.rain_acc = 0.0;
        self.sun_t = IDLE_T;
        self.sun_taps = 0;
        self.flag_t = IDLE_T;
        self.flag_taps = 0;
        self.car_t = [IDLE_T; FINALE_MAX_CARS];
        self.car_taps = 0;
        self.balloon_t = [IDLE_T; FINALE_BALLOONS];
        self.balloon_taps = 0;
        self.flower_t = [IDLE_T; FINALE_FLOWERS];
        self.flower_taps = 0;
        self.build_cars();
        ctx.audio.finale();
        let f = &ctx.frame;
        self.confetti.burst(vec2(f.w * 0.5, f.h * 0.34), 130, f.w * 0.32);
    }

    /// Build the train's cargo: the kid's pattern repeated cleanly, ONE item per
    /// car, read left→right. Built from the unit sequence — never
    /// `round.visible` (whose partial tail would render a broken pattern).
    fn build_cars(&mut self) {
        let one = unit_sequence(&self.round);
        self.car_period = one.len().max(1);
        // Two repetitions is the most the layout ever shows; it caps how many fit.
        let mut cars = one.clone();
        cars.extend(one);
        self.cars = cars;
    }

    /// Replay: a fresh game from level 1 (stars are session-only, so reset).
    fn restart(&mut self) {
        self.phase = Phase::Play;
        self.level = 1;
        self.stars = 0;
        self.streak = 0;
        self.finale_t = 0.0;
        self.drive_t = None;
        self.next_round();
    }

    /// Draw the level-up drive-by: a mini engine + the just-mastered unit on
    /// cars, crossing the bottom band left→right. Purely decorative (no hit
    /// target); skipped when the band would touch the choices/FAB.
    fn draw_driveby(&self, ctx: &Ctx, p: &PLayout) {
        let Some(t) = self.drive_t else { return };
        let f = &ctx.frame;
        let Some((by, r)) = drive_band(f, p, self.mode) else { return };
        let wheel_r = r * 0.5;
        let n = self.drive_items.len().max(1);
        let car_h = r * 1.15;
        let car_w = car_h * 1.25;
        let pitch = car_w * 1.18;
        let train_w = r * 4.6 + n as f32 * pitch;
        let ex = -train_w + (f.w + 2.0 * train_w) * (t / DRIVE_DUR).clamp(0.0, 1.0);
        let wheel_ang = -ex / wheel_r;
        // Cars trail the engine; the unit still reads left→right.
        let leftmost = ex - r * 2.05 - car_w * 0.5 - (n - 1) as f32 * pitch;
        for (j, item) in self.drive_items.iter().enumerate() {
            let cx = leftmost + j as f32 * pitch;
            let body = Rect::new(cx - car_w / 2.0, by - wheel_r - car_h, car_w, car_h);
            draw::train_car_chassis(body, by, wheel_r);
            let seat = car_h * 0.62;
            let seat_cy = body.y + body.h * 0.46;
            draw_cell(cx, seat_cy, seat, palette::WHITE, palette::CELL_BORDER);
            draw_item(item, cx, seat_cy, seat * 0.78, ctx);
        }
        let ep = draw::EnginePose { dy: 0.4 * (ctx.time * 6.0).sin(), ..Default::default() };
        draw::train_engine(ex, by, r, ep, wheel_ang, 0.3, idle_frog(ctx.time));
        // A short trail of steam puffs, drifting up and back.
        let tip = draw::engine_funnel_tip(ex, by, r);
        let cad = 0.35;
        let life = 0.9;
        let kmax = (t / cad).floor() as i32;
        let kmin = (((t - life) / cad).ceil() as i32).max(0);
        for k in kmin..=kmax {
            let age = t - k as f32 * cad;
            if !(0.0..=life).contains(&age) {
                continue;
            }
            let a = age / life;
            // Puffs anchor where the funnel was when they were born.
            let born_ex = -train_w + (f.w + 2.0 * train_w) * ((k as f32 * cad) / DRIVE_DUR).clamp(0.0, 1.0);
            draw::steam_puff(born_ex + r * 0.95, tip.y - 30.0 * age, r * 0.20 * (1.0 + a), 0.7 * (1.0 - a));
        }
    }

    fn update_finale(&mut self, ctx: &Ctx) -> Nav {
        let fl = finale_layout(&ctx.frame, self.car_period);
        // A steady, gentle confetti rain over the celebration.
        self.rain_acc += ctx.dt;
        while self.rain_acc > 0.10 {
            self.confetti.rain(ctx.frame.w, -10.0, 1);
            self.rain_acc -= 0.10;
        }
        // Step every interactive reaction timer; each parks at IDLE_T once done.
        step_timers(&mut self.car_t, ctx.dt, CAR_BOUNCE_S);
        step_timers(&mut self.balloon_t, ctx.dt, BALLOON_BOB_S);
        step_timers(&mut self.flower_t, ctx.dt, FLOWER_POP_S);
        step_timers(std::slice::from_mut(&mut self.sun_t), ctx.dt, SUN_FLARE_S);
        step_timers(std::slice::from_mut(&mut self.flag_t), ctx.dt, FLAG_WAVE_S);

        let pt = ctx.pointer;
        if !pt.tapped() {
            return Nav::Stay;
        }
        if input::hit_circle(pt.pos, fl.replay.x, fl.replay.y, fl.btn_r)
            && self.tap_debounce.accept(TGT_REPLAY, ctx.time)
        {
            self.restart();
            return Nav::Stay;
        }
        if input::hit_circle(pt.pos, fl.home.x, fl.home.y, fl.btn_r)
            && self.tap_debounce.accept(TGT_HOME, ctx.time)
        {
            return Nav::Home;
        }
        // Tap the engine → a whistle TOOT + steam + confetti, cycling a
        // non-escalating reaction (errorless, infinitely re-tappable). The train's
        // own targets are tested before the sky ones so the hero always wins.
        let ex = fl.engine.x + train_offset(self.finale_t, &fl);
        let hit = crate::draw::engine_hit_rect(ex, fl.engine.y, fl.r_boiler);
        if input::hit_rect(pt.pos, hit.x, hit.y, hit.w, hit.h)
            && self.tap_debounce.accept(TGT_ENGINE, ctx.time)
        {
            self.engine_taps += 1;
            self.react_kind = (self.engine_taps as usize - 1) % REACTIONS.len();
            self.react_t = 0.0;
            ctx.audio.train_whistle();
            let tip = crate::draw::engine_funnel_tip(ex, fl.engine.y, fl.r_boiler);
            self.confetti.burst(tip, 44, fl.r_boiler * 0.9);
            return Nav::Stay;
        }
        // The finish flag → an excited flutter + a finial pop + confetti + toot.
        let flag_c = fl.flag_center();
        if input::hit_circle(pt.pos, flag_c.x, flag_c.y, fl.flag_w)
            && self.tap_debounce.accept(TGT_FLAG, ctx.time)
        {
            self.flag_t = 0.0;
            self.flag_taps += 1;
            ctx.audio.train_whistle();
            self.confetti.burst(vec2(fl.flag_x - fl.flag_w * 0.5, fl.flag_top), 40, fl.flag_w);
            return Nav::Stay;
        }
        // The sun → a burst of rays + a pop + a twinkle sparkle spray.
        if input::hit_circle(pt.pos, fl.sun_c.x, fl.sun_c.y, fl.sun_r * 1.4)
            && self.tap_debounce.accept(TGT_SUN, ctx.time)
        {
            self.sun_t = 0.0;
            self.sun_taps += 1;
            ctx.audio.twinkle();
            self.confetti.burst(fl.sun_c, 18, fl.sun_r * 0.9);
            return Nav::Stay;
        }
        // The cars → poke your own mastered pattern; each piece boings up.
        let tdx = train_offset(self.finale_t, &fl);
        let n_cars = fl.n_cars.min(self.cars.len()).min(FINALE_MAX_CARS);
        for i in 0..n_cars {
            let c = fl.car_seat(i, tdx);
            if input::hit_circle(pt.pos, c.x, c.y, fl.seat * 0.7)
                && self.tap_debounce.accept(TGT_CAR_BASE + i as u32, ctx.time)
            {
                self.car_t[i] = 0.0;
                self.car_taps += 1;
                ctx.audio.tap();
                return Nav::Stay;
            }
        }
        // Party balloons in the sky → a pop-wobble.
        for i in 0..FINALE_BALLOONS {
            let p = fl.balloon(i, self.finale_t);
            if input::hit_circle(pt.pos, p.x, p.y, fl.balloon_r * 1.25)
                && self.tap_debounce.accept(TGT_BALLOON_BASE + i as u32, ctx.time)
            {
                self.balloon_t[i] = 0.0;
                self.balloon_taps += 1;
                ctx.audio.tap();
                return Nav::Stay;
            }
        }
        // The meadow flowers (tablet only) → the bloom springs up + a twinkle.
        if !ctx.frame.is_phone() {
            for i in 0..FINALE_FLOWERS {
                let (root, size) = finale_flower(&ctx.frame, fl.ground_y, i);
                if input::hit_circle(pt.pos, root.x, root.y - size, (size * 0.6).max(20.0))
                    && self.tap_debounce.accept(TGT_FLOWER_BASE + i as u32, ctx.time)
                {
                    self.flower_t[i] = 0.0;
                    self.flower_taps += 1;
                    ctx.audio.twinkle();
                    self.confetti.burst(vec2(root.x, root.y - size), 9, size * 0.7);
                    return Nav::Stay;
                }
            }
        }
        Nav::Stay
    }

    fn draw_finale(&self, ctx: &Ctx) {
        let f = &ctx.frame;
        let fl = finale_layout(f, self.car_period);
        let by = fl.ground_y;
        let r = fl.r_boiler;
        let pi = std::f32::consts::PI;
        let content = f.content();

        // Sky (golden-hour) + low sun + far hills + ground band.
        draw::vgradient(0.0, 0.0, f.w, by, palette::SKY_DUSK_TOP, palette::SKY_DUSK_BOT);
        // The sun is tappable: it pops and throws a burst of rays when poked.
        let sun_pop = if self.sun_t < SUN_FLARE_S {
            1.0 + (self.sun_t / SUN_FLARE_S * pi).sin() * 0.18
        } else {
            1.0
        };
        draw::sun_rays(fl.sun_c.x, fl.sun_c.y, fl.sun_r, (1.0 - self.sun_t).max(0.0), ctx.time * 1.5);
        draw::sun(fl.sun_c.x, fl.sun_c.y, fl.sun_r * sun_pop);
        if fl.show_far_hills {
            draw::fill_ellipse(f.w * 0.30, by + f.h * 0.06, f.w * 0.42, f.h * 0.16, 0.0, palette::HILL_FAR);
            draw::fill_ellipse(f.w * 0.72, by + f.h * 0.05, f.w * 0.40, f.h * 0.14, 0.0, palette::HILL_FAR);
        }
        draw::vgradient(0.0, by, f.w, f.h - by, palette::HILL_NEAR, palette::GROUND_BOT);
        draw::fill_ellipse(f.w * 0.5, by + f.h * 0.10, f.w * 0.7, f.h * 0.12, 0.0, palette::HILL_NEAR);

        // Track: sleepers tiled across, then a darker rail line on top.
        let s_pitch = (fl.car_pitch * 0.5).max(28.0);
        let sw = s_pitch * 0.32;
        let sh = (r * 0.5).max(10.0);
        let mut sx = content.x.rem_euclid(s_pitch) - s_pitch;
        while sx < f.w + s_pitch {
            draw::rounded_rect(sx - sw / 2.0, by - sh * 0.18, sw, sh, sw * 0.3, palette::RAIL);
            sx += s_pitch;
        }
        draw_line(0.0, by, f.w, by, (r * 0.12).max(3.0), Color::new(0.40, 0.34, 0.28, 1.0));

        // A few cheerful meadow flowers in the foreground (tablet only — a phone
        // foreground is too short and would crowd the buttons). Tap → the bloom
        // springs up.
        if !f.is_phone() {
            for i in 0..FINALE_FLOWERS {
                let (root, size) = finale_flower(f, by, i);
                let pop = if self.flower_t[i] < FLOWER_POP_S {
                    (self.flower_t[i] / FLOWER_POP_S * pi).sin()
                } else {
                    0.0
                };
                draw::plant(root.x, root.y, size, pop);
            }
        }

        // Bunting (tablet only) high in the sky.
        if fl.show_bunting {
            draw::bunting(content.x, content.x + content.w, f.h * 0.12, f.h * 0.055, 12, ctx.time);
        }

        // Party balloons drifting in the sky (tap → pop-wobble). Each sways on
        // its own cadence with a curly string trailing below.
        for i in 0..FINALE_BALLOONS {
            let p = fl.balloon(i, self.finale_t);
            let col = palette::RAINBOW[i % palette::RAINBOW.len()];
            let sc = if self.balloon_t[i] < BALLOON_BOB_S {
                1.0 + 0.18 * (1.0 - self.balloon_t[i] / BALLOON_BOB_S)
            } else {
                1.0
            };
            let sway = 0.13 * (ctx.time * 0.6 + i as f32 * 1.7).sin();
            let tail = vec2(p.x + fl.balloon_r * 2.4 * sway.sin(), p.y + fl.balloon_r * 2.4 * sway.cos().max(0.3));
            draw::stroke_path(&[vec2(p.x, p.y + fl.balloon_r * 1.05), tail], 1.6, palette::hexa(0xffffff, 0.7));
            draw::fill_ellipse(p.x, p.y, fl.balloon_r * sc, fl.balloon_r * 1.18 * sc, sway.to_degrees(), col);
            draw::disc(p.x - fl.balloon_r * 0.32, p.y - fl.balloon_r * 0.42, fl.balloon_r * 0.18, palette::hexa(0xffffff, 0.5));
        }
        // Reaction state (engine scoot/squash, headlamp, frog-driver pose).
        let rx = &REACTIONS[self.react_kind];
        let (scoot, squash, lamp, cond) = if self.react_t < rx.dur {
            let p = (self.react_t / rx.dur).clamp(0.0, 1.0);
            let imp = (p * pi).sin();
            let cond = draw::FrogPose {
                dy: -0.12 * r * imp,
                rot: 0.05 * imp * rx.wave,
                sx: 1.0 + 0.02 * imp,
                sy: 1.0 - 0.02 * imp,
                blink: (imp * 0.6).min(0.6),
                // The "wave" reactions become a happy tongue-out ribbit.
                tongue: rx.wave * imp,
            };
            (rx.scoot * r * imp, rx.squash * imp, rx.lamp * (0.5 + 0.5 * (p * pi * 4.0).sin()), cond)
        } else {
            (0.0, 0.0, 0.0, idle_frog(ctx.time))
        };
        let train_dx = train_offset(self.finale_t, &fl) + scoot;
        let ex = fl.engine.x + train_dx;
        let wheel_ang = -ex / fl.wheel_r;

        // Cars: the kid's pattern, one item per car, read left→right (built from
        // `unit_items` so it's always a whole, unbroken unit).
        let n_cars = fl.n_cars.min(self.cars.len());
        for i in 0..n_cars {
            let item = &self.cars[i];
            let cx = fl.leftmost_cx + i as f32 * fl.car_pitch + train_dx;
            let body = Rect::new(cx - fl.car_w / 2.0, by - fl.wheel_r - fl.car_h, fl.car_w, fl.car_h);
            draw::train_car_chassis(body, by, fl.wheel_r);
            // Tap-bounce: poke a car and its pattern piece boings up + grows.
            let bounce = if i < FINALE_MAX_CARS && self.car_t[i] < CAR_BOUNCE_S {
                (self.car_t[i] / CAR_BOUNCE_S * pi).sin()
            } else {
                0.0
            };
            let lift = bounce * fl.car_h * 0.28;
            let seat_cy = body.y + body.h * 0.46 + (ctx.time * 3.0 + i as f32).sin() * 1.5 - lift;
            draw_cell(cx, seat_cy, fl.seat, palette::WHITE, palette::CELL_BORDER);
            draw_item(item, cx, seat_cy, fl.seat * 0.78 * (1.0 + bounce * 0.28), ctx);
        }

        // Engine + frog driver (the hero), in front of the cars.
        let ep = draw::EnginePose { dx: 0.0, dy: 0.5 * (ctx.time * 2.0).sin(), sx: 1.0 + squash * 0.5, sy: 1.0 - squash };
        draw::train_engine(ex, by, r, ep, wheel_ang, lamp, cond);

        // Steam puffs: a short, funnel-anchored trail of small puffs (drifts up-
        // LEFT into open sky, away from the right edge/flag) + a transient burst.
        let tip = draw::engine_funnel_tip(ex, by, r);
        let cad = 0.55;
        let life = 1.3;
        let kmax = (self.finale_t / cad).floor() as i32;
        let kmin = (((self.finale_t - life) / cad).ceil() as i32).max(0);
        for k in kmin..=kmax {
            let age = self.finale_t - k as f32 * cad;
            if age < 0.0 || age > life {
                continue;
            }
            let a = age / life;
            draw::steam_puff(tip.x - 16.0 * a, tip.y - 54.0 * age, r * 0.24 * (1.0 + a * 1.2), 0.8 * (1.0 - a));
        }
        if self.react_t < 0.7 {
            let a = (self.react_t / 0.7).clamp(0.0, 1.0);
            draw::steam_puff(tip.x - 14.0 * a, tip.y - 60.0 * a, r * 0.45 * (1.0 + a), 0.85 * (1.0 - a));
        }

        // Finish flag drawn LAST (over the steam) so its checkers stay crisp — the
        // engine is parked left of the pole, so nothing else occludes it. Tapping
        // it whips up the flutter and pops a bigger star on the finial.
        let flag_time = if self.flag_t < FLAG_WAVE_S {
            ctx.time + (1.0 - self.flag_t / FLAG_WAVE_S) * 1.6
        } else {
            ctx.time
        };
        draw::checker_flag(fl.flag_x, by, fl.flag_top, fl.flag_w, fl.flag_h, flag_time);
        if self.flag_t < FLAG_WAVE_S {
            let a = 1.0 - self.flag_t / FLAG_WAVE_S;
            draw::star(fl.flag_x, fl.flag_top - fl.flag_h * 0.22, fl.flag_h * 0.2 * (1.0 + 0.6 * a), palette::GOLD);
        }

        // Replay / Home (phonics-identical placement for cross-finale predictability).
        chrome::draw_corner_buttons(fl.replay, fl.home, fl.btn_r);
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
    pub(crate) fn in_finale(&self) -> bool {
        self.phase == Phase::Finale
    }
    pub(crate) fn engine_taps(&self) -> u32 {
        self.engine_taps
    }
    pub(crate) fn sun_taps(&self) -> u32 {
        self.sun_taps
    }
    pub(crate) fn flag_taps(&self) -> u32 {
        self.flag_taps
    }
    pub(crate) fn car_taps(&self) -> u32 {
        self.car_taps
    }
    pub(crate) fn balloon_taps(&self) -> u32 {
        self.balloon_taps
    }
    pub(crate) fn flower_taps(&self) -> u32 {
        self.flower_taps
    }
    pub(crate) fn finale_flower_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        let (root, size) = finale_flower(f, finale_layout(f, self.car_period).ground_y, i);
        vec2(root.x, root.y - size)
    }
    pub(crate) fn finale_sun_center(&self, f: &crate::layout::Frame) -> Vec2 {
        finale_layout(f, self.car_period).sun_c
    }
    pub(crate) fn finale_flag_center(&self, f: &crate::layout::Frame) -> Vec2 {
        finale_layout(f, self.car_period).flag_center()
    }
    pub(crate) fn finale_car_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        let fl = finale_layout(f, self.car_period);
        fl.car_seat(i, train_offset(self.finale_t, &fl))
    }
    pub(crate) fn finale_balloon_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        finale_layout(f, self.car_period).balloon(i, self.finale_t)
    }
    /// Center of the engine tap target at the current (possibly mid-entrance)
    /// position — a point guaranteed to land inside the hit rect.
    pub(crate) fn engine_center(&self, f: &crate::layout::Frame) -> Vec2 {
        let fl = finale_layout(f, self.car_period);
        let ex = fl.engine.x + train_offset(self.finale_t, &fl);
        let hit = crate::draw::engine_hit_rect(ex, fl.engine.y, fl.r_boiler);
        vec2(hit.x + hit.w / 2.0, hit.y + hit.h / 2.0)
    }
    pub(crate) fn replay_center(&self, f: &crate::layout::Frame) -> Vec2 {
        finale_layout(f, self.car_period).replay
    }
    /// Unit mode: center of the submit FAB.
    pub(crate) fn fab_center(&self, f: &crate::layout::Frame) -> Vec2 {
        unit_fab(f).0
    }
    pub(crate) fn unit_selection(&self) -> Option<(usize, usize)> {
        self.sel
    }
    pub(crate) fn drive_active(&self) -> bool {
        self.drive_t.is_some()
    }
}

fn unit_fab(f: &crate::layout::Frame) -> (Vec2, f32) {
    (vec2(f.w / 2.0, f.h * 0.78), (f.w * 0.06).clamp(60.0, 90.0))
}

/// The bottom band for the level-up drive-by: `(track_y, r_boiler)` when a mini
/// train clears the choices (and the unit FAB) with margin; `None` on cramped
/// viewports (short phone-landscape), which keep the fanfare + pips only.
fn drive_band(f: &crate::layout::Frame, p: &PLayout, mode: GameMode) -> Option<(f32, f32)> {
    let r = f.vmin(0.032).clamp(14.0, 26.0);
    let track = f.h - f.safe.bottom.max(6.0) - r * 0.3;
    let mut clutter_bottom = p.choices.iter().map(|c| c.y + c.h).fold(0.0_f32, f32::max);
    if mode == GameMode::Unit {
        let fab = unit_fab(f);
        clutter_bottom = clutter_bottom.max(fab.0.y + fab.1);
    }
    // 3.1r = the engine's full height (funnel + frog head included).
    if track - r * 3.1 > clutter_bottom + 8.0 {
        Some((track, r))
    } else {
        None
    }
}

fn gen(level: u32, choice: ThemeChoice, mode: GameMode, diff: Difficulty, rng: &mut Mulberry32) -> Round {
    let theme = themes::resolve_theme(choice, rng);
    generate_round(level, theme, mode, diff, rng)
}

/// One whole unit of the round's pattern as items, read left→right: the
/// template (e.g. "AAB") tiled over `unit_items` (which is indexed by DISTINCT
/// letter, so an AAB unit is three items from two). Never empty.
fn unit_sequence(round: &Round) -> Vec<Item> {
    let unit = &round.unit_items;
    let mut out: Vec<Item> = Vec::new();
    for ch in round.template.chars() {
        let idx = (ch as u32).wrapping_sub('A' as u32) as usize;
        if let Some(it) = unit.get(idx) {
            out.push(it.clone());
        }
    }
    if out.is_empty() {
        out = unit.to_vec(); // defensive: never an empty train
    }
    out
}

impl Scene for PatternsScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.fb_time += ctx.dt;
        self.confetti.update(ctx.dt);
        if let Some(t) = self.drive_t {
            let t = t + ctx.dt;
            // Linger past DRIVE_DUR so the last steam puffs fade out instead of
            // vanishing the frame the (already offscreen) train parks.
            self.drive_t = if t > DRIVE_DUR + 0.9 { None } else { Some(t) };
        }
        if self.phase == Phase::Finale {
            self.finale_t += ctx.dt;
            self.react_t += ctx.dt;
            return self.update_finale(ctx);
        }
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
        match chrome::handle_topbar(&chrome::topbar(&ctx.frame), ctx, &self.db) {
            Some(chrome::TopbarAction::OpenParent) => return Nav::OpenParent,
            Some(chrome::TopbarAction::Home) => return Nav::Home,
            Some(chrome::TopbarAction::MuteToggled) => return Nav::Stay,
            None => {}
        }
        if !pt.tapped() {
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
        if self.phase == Phase::Finale {
            self.draw_finale(ctx);
            self.confetti.draw();
            return;
        }
        clear_background(palette::BG);
        let p = plan(&ctx.frame, self.round.choices.len(), self.round.visible.len() + 1, self.mode);

        // Topbar: home, stars + level pips, mute.
        chrome::draw_topbar(&chrome::topbar(&ctx.frame), ctx);
        draw_hud(&p, self.stars, self.level);

        // Sequence bar.
        draw::card(p.seq.x, p.seq.y, p.seq.w, p.seq.h, palette::CARD);
        // Gentle, slow breathing on the `?` slot — kept small + unhurried so it
        // draws the eye to the gap without becoming a distracting jiggle.
        let pulse = 1.0 + 0.035 * crate::anim::pulse(ctx.time, 3.2).max(0.0);
        for (i, item) in self.round.visible.iter().enumerate() {
            let (cx, cy) = p.cell_center(i);
            let selected = matches!(self.sel, Some((s, e)) if i >= s && i < e);
            if selected {
                draw::rounded_rect(
                    cx - p.cell / 2.0 - 4.0, cy - p.cell / 2.0 - 4.0,
                    p.cell + 8.0, p.cell + 8.0, p.cell * 0.2, palette::ACCENT,
                );
                draw_cell(cx, cy, p.cell, palette::ACCENT_SOFT, palette::ACCENT);
            } else {
                draw_cell(cx, cy, p.cell, palette::WHITE, palette::CELL_BORDER);
            }
            draw_item(item, cx, cy, p.cell * 0.78, ctx);
        }

        match self.mode {
            GameMode::Next => {
                // The pink `?` slot to fill — a pink ring + deep-rose glyph so it
                // pops against the bar and stays legible on the pale fill.
                let (sx, sy) = p.cell_center(self.round.visible.len());
                draw_cell(sx, sy, p.cell * pulse, palette::ACCENT_SOFT, palette::ACCENT);
                text::draw_centered("?", sx, sy, (p.cell * 0.7) as u16, &ctx.fonts.cursive, palette::ACCENT_STRONG);
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

        self.draw_driveby(ctx, &p);
        self.confetti.draw();
    }
}

// --- rendering helpers ------------------------------------------------------

fn draw_cell(cx: f32, cy: f32, size: f32, fill: Color, border: Color) {
    let r = (size * 0.18).min(18.0);
    let x = cx - size / 2.0;
    let y = cy - size / 2.0;
    // drop shadow so a cell lifts off the warm-white bar
    draw::rounded_rect(x, y + 3.0, size, size, r, Color::new(0.17, 0.17, 0.2, 0.10));
    // border ring: white-on-off-white tiles were near-invisible, so each cell
    // gets a warm ring to read as a distinct rectangle. Kept slim (3.8% of the
    // cell) so it reads as a clean outline, not a chunky frame.
    let bw = (size * 0.038).max(2.0);
    draw::rounded_rect(x - bw, y - bw, size + 2.0 * bw, size + 2.0 * bw, r + bw, border);
    draw::rounded_rect(x, y, size, size, r, fill);
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
        draw::disc(cx, cy, r, color);
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
        draw::disc(px0 + i as f32 * 20.0, py, 7.0, c);
    }
}

// --- layout -----------------------------------------------------------------

struct PLayout {
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

    // Choices: a single row below the sequence — never wrap to a grid, so a
    // preschooler tracks one left-to-right strip of options (mirrors the
    // sequence bar above). Keep the familiar per-tile width, but shrink it if
    // needed so all `n_choices` fit across the play width on one line.
    let cgap = 20.0;
    let nf = n_choices.max(1) as f32;
    let fit_w = (f.w * 0.92 - (nf - 1.0) * cgap) / nf;
    let cw = (f.w * 0.2).clamp(140.0, 240.0).min(fit_w);
    let ch = (f.h * 0.16).clamp(96.0, 180.0);
    let choices_h = ch;

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

    let row_w = nf * cw + (nf - 1.0) * cgap;
    let x0 = f.w / 2.0 - row_w / 2.0;
    let mut choices = Vec::new();
    for i in 0..n_choices {
        choices.push(Rect::new(x0 + i as f32 * (cw + cgap), gy0, cw, ch));
    }

    PLayout {
        hud: (vec2(tb.x + 2.2 * ir, tb.y + ir), ir),
        seq,
        cell,
        seq_n: seq_cells,
        choices,
    }
}

// --- finale layout + motion --------------------------------------------------

/// Geometry for the Pattern Train finale, derived (like everything else) from
/// viewport size + safe insets + form factor. Car *count* depends on the pattern
/// period, so it's computed at draw time from `max_cars` + `rightmost_cx`.
struct FinaleLayout {
    ground_y: f32, // the track line (`by`)
    r_boiler: f32, // engine boiler radius (== 2× wheel radius)
    wheel_r: f32,
    engine: Vec2, // parked base (wheels on track)
    car_w: f32,
    car_h: f32,
    car_pitch: f32,
    seat: f32, // item seat size in a car
    n_cars: usize, // cars actually drawn (whole period(s) when they fit)
    leftmost_cx: f32, // center of the leftmost (first item) car
    flag_x: f32, // finish-flag pole x
    flag_top: f32,
    flag_w: f32,
    flag_h: f32,
    sun_c: Vec2,
    sun_r: f32,
    balloon_r: f32,
    balloon_anchor: [Vec2; FINALE_BALLOONS],
    replay: Vec2,
    home: Vec2,
    btn_r: f32,
    show_far_hills: bool,
    show_bunting: bool,
}

impl FinaleLayout {
    /// Seat center of car `i` at the current train offset — the tap target and
    /// the drawn position share this so a poke never desyncs from the piece.
    fn car_seat(&self, i: usize, train_dx: f32) -> Vec2 {
        let cx = self.leftmost_cx + i as f32 * self.car_pitch + train_dx;
        let body_y = self.ground_y - self.wheel_r - self.car_h;
        vec2(cx, body_y + self.car_h * 0.46)
    }

    /// Center of the hanging checker flag (its tap target).
    fn flag_center(&self) -> Vec2 {
        vec2(self.flag_x - self.flag_w * 0.5, self.flag_top + self.flag_h * 0.5)
    }

    /// Balloon `i` bobbing on its own cadence (the tap hit-test uses the same
    /// value so it never desyncs from the drawn balloon).
    fn balloon(&self, i: usize, time: f32) -> Vec2 {
        let a = self.balloon_anchor[i];
        let ph = i as f32 * 1.7;
        vec2(
            a.x + 15.0 * (time * 0.5 + ph).sin() + 6.0 * (time * 0.23 + ph).cos(),
            a.y + 17.0 * (time * 0.42 + ph).cos(),
        )
    }
}

fn finale_layout(f: &crate::layout::Frame, car_period: usize) -> FinaleLayout {
    let content = f.content();
    // Lower track (more sky) on short/phone screens so the tall hat + flag clear
    // the top; a touch higher on the roomy tablet-landscape.
    let ground = if f.is_phone() {
        0.24
    } else if f.is_portrait() {
        0.40 // raise the train off the bottom so portrait isn't bottom-heavy
    } else {
        0.36
    };
    let by = f.h * (1.0 - ground);

    let wheel_r = f.vmin(0.045).clamp(16.0, 40.0);
    let r = wheel_r * 2.0;

    // Finish flag near the right edge, flying HIGH on a tall pole so the checker
    // + star finial read clearly ABOVE the engine (which arrives at the pole base).
    let flag_w = r * 1.5;
    let flag_h = r * 1.35;
    let flag_x = content.x + content.w - r * 0.5;
    let flag_top = by - r * 4.0;
    // Park the engine just LEFT of the flagpole so the pole + checker + funnel
    // steam all stay clear of it (the flag is the one non-reader "finish" symbol).
    let ex = flag_x - r * 2.5;

    // Size the cars to show a WHOLE number of pattern periods (never a partial
    // unit — that would teach the misconception the game fights), as big as fits
    // up to a cap, dropping from 2 reps → 1 → (only as a last resort on a tiny
    // screen) a partial that still clips off the left rather than the engine.
    let period = car_period.max(1);
    let avail = ((ex - r * 2.05) - content.x).max(80.0);
    // A modest flat min so even a long (period-5) unit fits as a WHOLE unit on
    // the narrow portrait rather than clipping to a partial; cars then size up.
    let min_h = 48.0;
    let max_h = f.vmin(0.20).clamp(92.0, 150.0);
    let pitch_of = |h: f32| h * 1.25 * 1.18;
    let max_fit = ((avail / pitch_of(min_h)).floor() as i32).max(1) as usize;
    // Short units (≤3 cells) ride twice so the repeat is unmistakable; the level-6
    // finale's longer 4–5-cell units ride ONCE, shown big — the whole unit is the
    // payoff, and AABCD/ABCBD already repeat within a single unit.
    let reps = if period <= 3 { (max_fit / period).clamp(1, 2) } else { 1 };
    // A whole unit always rides: if it doesn't fit at the comfortable minimum,
    // shrink the cars (down to a hard floor) before ever showing a partial — a
    // broken pattern would teach the exact misconception the game fights. The
    // partial fallback survives only for viewports too tiny to ship.
    let hard_min = 32.0;
    let n_cars = if period <= max_fit {
        period * reps
    } else if avail / period as f32 >= pitch_of(hard_min) {
        period
    } else {
        max_fit
    };
    let pitch0 = (avail / n_cars as f32).min(pitch_of(max_h));
    let car_h = (pitch0 / 1.18 / 1.25).clamp(hard_min, max_h);
    let car_w = car_h * 1.25;
    let car_pitch = car_w * 1.18;
    let seat = car_h * 0.62;
    let rightmost_cx = ex - r * 2.05 - car_w * 0.5;
    let leftmost_cx = rightmost_cx - n_cars.saturating_sub(1) as f32 * car_pitch;

    // The tappable sun sits high in the LEFT sky — clear of the train (engine +
    // cars park to the right/bottom) and the finish flag (top-right), so poking it
    // never collides with the engine's tap target.
    let sun_r = if f.is_phone() { f.vmin(0.07) } else { f.vmin(0.09) };
    let sun_c = vec2(content.x + content.w * 0.13, by * 0.30);

    // Party balloons drift across the mid sky between the sun (left) and the flag
    // (top-right), above the low cars — each its own tap target.
    let balloon_r = (f.w * 0.028).clamp(16.0, 34.0);
    let bspots = [(0.32, 0.32), (0.44, 0.46), (0.56, 0.30), (0.66, 0.44)];
    let mut balloon_anchor = [Vec2::ZERO; FINALE_BALLOONS];
    for (b, (fx, fy)) in balloon_anchor.iter_mut().zip(bspots.iter()) {
        *b = vec2(content.x + content.w * fx, by * fy);
    }

    let (replay, home, br) = chrome::corner_buttons(f);

    FinaleLayout {
        ground_y: by,
        r_boiler: r,
        wheel_r,
        engine: vec2(ex, by),
        car_w,
        car_h,
        car_pitch,
        seat,
        n_cars,
        leftmost_cx,
        flag_x,
        flag_top,
        flag_w,
        flag_h,
        sun_c,
        sun_r,
        balloon_r,
        balloon_anchor,
        replay,
        home,
        btn_r: br,
        show_far_hills: !f.is_phone(),
        // Bunting is the rainbow-triangle "finish line" string — show it on every
        // device. It hangs high in the sky (clear of the train), so the short
        // phone foreground that gates the far-hills/flowers doesn't apply here.
        show_bunting: true,
    }
}

/// Entrance slide: the train eases in from off the left and parks at the station
/// over 1.8 s (0 once parked). `wheel_ang = -x/wheel_r` then rolls it without slip.
fn train_offset(ft: f32, fl: &FinaleLayout) -> f32 {
    let dur = 1.8;
    let p = (ft / dur).clamp(0.0, 1.0);
    let start = -(fl.engine.x + fl.r_boiler * 2.5); // fully off-left at t=0
    start * (1.0 - crate::anim::ease_out_cubic(p))
}

/// Resting frog driver: a gentle breathing bob + an occasional blink, so the
/// mascot looks alive in the cab even when untapped (mirrors the phonics idle).
fn idle_frog(time: f32) -> draw::FrogPose {
    use std::f32::consts::PI;
    let breathe = (time * 2.0).sin();
    let bt = time.rem_euclid(3.6);
    let blink = if bt < 0.16 { (bt / 0.16 * PI).sin() } else { 0.0 };
    draw::FrogPose {
        dy: 2.0 * breathe,
        rot: 0.0,
        sx: 1.0 - 0.02 * breathe,
        sy: 1.0 + 0.025 * breathe,
        blink,
        tongue: 0.0,
    }
}

/// Step in-flight finale reaction timers by `dt`, parking each at [`IDLE_T`]
/// once it passes `dur` (mirrors the newer games' finale bookkeeping).
fn step_timers(timers: &mut [f32], dt: f32, dur: f32) {
    for s in timers.iter_mut() {
        if *s < dur {
            *s += dt;
        } else {
            *s = IDLE_T;
        }
    }
}

/// One engine tap reaction. Tapping cycles these in order; they don't escalate
/// (mirrors the phonics frog). `scoot` is in boiler-radii; `wave`/`lamp` 0..1.
struct FinaleReaction {
    dur: f32,
    scoot: f32,
    squash: f32,
    wave: f32,
    lamp: f32,
}

const REACTIONS: [FinaleReaction; 5] = [
    FinaleReaction { dur: 0.55, scoot: 0.0, squash: 0.14, wave: 0.0, lamp: 0.0 }, // toot + cap-bob
    FinaleReaction { dur: 0.70, scoot: 0.28, squash: 0.20, wave: 0.0, lamp: 0.0 }, // chuff-scoot
    FinaleReaction { dur: 0.75, scoot: 0.0, squash: 0.08, wave: 1.0, lamp: 0.0 }, // whistle-wave
    FinaleReaction { dur: 0.62, scoot: 0.0, squash: 0.12, wave: 0.4, lamp: 0.0 }, // big puff
    FinaleReaction { dur: 0.55, scoot: 0.0, squash: 0.06, wave: 0.0, lamp: 1.0 }, // headlamp flare
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Frame, Insets};

    fn frame(w: f32, h: f32) -> Frame {
        Frame::new(w, h, Insets::default())
    }

    /// The finale must fit every device: the engine+hat tap target and the
    /// finish flag never clip the top/right, and both buttons stay inside the
    /// safe viewport. Mirrors phonics' `rainbow_apex_clears_the_top` idiom.
    #[test]
    fn finale_fits_every_device() {
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)] {
            let f = frame(w, h);
            let fl = finale_layout(&f, 5); // worst case: a long period-5 unit
            let c = f.content();
            let hit = crate::draw::engine_hit_rect(fl.engine.x, fl.engine.y, fl.r_boiler);
            assert!(hit.y >= c.y - 0.5, "{w}x{h}: engine/hat apex {} clips content top {}", hit.y, c.y);
            assert!(hit.x + hit.w <= c.x + c.w + 0.5, "{w}x{h}: engine clips the right edge");
            assert!(fl.flag_top >= c.y - 0.5, "{w}x{h}: finish flag clips the top");
            assert!(fl.flag_x <= c.x + c.w + 0.5, "{w}x{h}: finish flag off the right");
            for b in [fl.replay, fl.home] {
                assert!(b.x - fl.btn_r >= f.safe.left - 0.5, "{w}x{h}: button off the left");
                assert!(b.x + fl.btn_r <= f.w - f.safe.right + 0.5, "{w}x{h}: button off the right");
                assert!(b.y + fl.btn_r <= f.h - f.safe.bottom + 0.5, "{w}x{h}: button below the viewport");
            }
            // The tappable sun must sit clear of the engine's tap rect, else a
            // poke at the engine would land on the sun (or vice versa) — the two
            // targets have to be distinct. Nearest-point of the rect to the sun
            // center must be outside the sun's (generous 1.4×) tap circle.
            let nx = fl.sun_c.x.clamp(hit.x, hit.x + hit.w);
            let ny = fl.sun_c.y.clamp(hit.y, hit.y + hit.h);
            let d2 = (fl.sun_c.x - nx).powi(2) + (fl.sun_c.y - ny).powi(2);
            assert!(d2 > (fl.sun_r * 1.4).powi(2), "{w}x{h}: sun tap circle overlaps the engine");
            assert!(fl.n_cars >= 1, "{w}x{h}: no room for even one car");
            // The whole consist (even a period-5 unit) stays inside the content
            // box — the leftmost car never clips off the left edge.
            let left_edge = fl.leftmost_cx - fl.car_w / 2.0;
            assert!(left_edge >= c.x - 0.5, "{w}x{h}: leftmost car {left_edge} clips content left {}", c.x);
        }
    }

    /// The train must always carry WHOLE pattern units — a partial unit on the
    /// cars would render a broken pattern, the exact misconception the game
    /// fights. Every period × every shipping form factor.
    #[test]
    fn finale_never_shows_a_partial_unit() {
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0), (812.0, 375.0)] {
            let f = frame(w, h);
            for period in 2..=5usize {
                let fl = finale_layout(&f, period);
                assert!(
                    fl.n_cars.is_multiple_of(period),
                    "{w}x{h} period {period}: {} cars is a partial unit",
                    fl.n_cars
                );
                let left_edge = fl.leftmost_cx - fl.car_w / 2.0;
                let c = f.content();
                assert!(left_edge >= c.x - 0.5, "{w}x{h} period {period}: consist clips left");
            }
        }
    }
}
