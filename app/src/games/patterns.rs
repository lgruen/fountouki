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

/// Consecutive correct answers needed to level up. The streak resets on a
/// wrong answer, so a level only advances on a clean run (mastery), never on a
/// mistake-then-correct. Stars stay monotonic regardless. A clean streak *at*
/// `MAX_LEVEL` fires the finale instead of leveling up (you beat the last level).
const LEVEL_UP_STREAK: u32 = 4;
const ADVANCE_DELAY: f32 = 0.85;
const RETRY_DELAY: f32 = 0.55;

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
        self.finale_t = 0.0;
        self.react_t = 99.0;
        self.react_kind = 0;
        self.engine_taps = 0;
        self.rain_acc = 0.0;
        self.build_cars();
        ctx.audio.finale();
        let f = &ctx.frame;
        self.confetti.burst(vec2(f.w * 0.5, f.h * 0.34), 130, f.w * 0.32);
    }

    /// Build the train's cargo: the kid's pattern repeated cleanly, ONE item per
    /// car, read left→right. Built from `unit_items` tiled over the template —
    /// never `round.visible` (whose partial tail would render a broken pattern).
    fn build_cars(&mut self) {
        let unit = &self.round.unit_items;
        let chars: Vec<char> = self.round.template.chars().collect();
        self.car_period = chars.len().max(1);
        // Two repetitions is the most the layout ever shows; it caps how many fit.
        let mut cars = Vec::with_capacity(chars.len() * 2);
        for _ in 0..2 {
            for &ch in &chars {
                let idx = (ch as u32).wrapping_sub('A' as u32) as usize;
                if let Some(it) = unit.get(idx) {
                    cars.push(it.clone());
                }
            }
        }
        if cars.is_empty() {
            cars = unit.to_vec(); // defensive: never an empty train
        }
        self.cars = cars;
    }

    /// Replay: a fresh game from level 1 (stars are session-only, so reset).
    fn restart(&mut self) {
        self.phase = Phase::Play;
        self.level = 1;
        self.stars = 0;
        self.streak = 0;
        self.finale_t = 0.0;
        self.next_round();
    }

    fn update_finale(&mut self, ctx: &Ctx) -> Nav {
        let fl = finale_layout(&ctx.frame, self.car_period);
        // A steady, gentle confetti rain over the celebration.
        self.rain_acc += ctx.dt;
        while self.rain_acc > 0.10 {
            self.confetti.rain(ctx.frame.w, -10.0, 1);
            self.rain_acc -= 0.10;
        }
        let pt = ctx.pointer;
        if !pt.tapped() {
            return Nav::Stay;
        }
        if input::hit_circle(pt.pos, fl.replay.x, fl.replay.y, fl.btn_r) {
            self.restart();
            return Nav::Stay;
        }
        if input::hit_circle(pt.pos, fl.home.x, fl.home.y, fl.btn_r) {
            return Nav::Home;
        }
        // Tap the engine → a whistle TOOT + steam + confetti, cycling a
        // non-escalating reaction (errorless, infinitely re-tappable).
        let ex = fl.engine.x + train_offset(self.finale_t, &fl);
        let hit = crate::draw::engine_hit_rect(ex, fl.engine.y, fl.r_boiler);
        if input::hit_rect(pt.pos, hit.x, hit.y, hit.w, hit.h) {
            self.engine_taps += 1;
            self.react_kind = (self.engine_taps as usize - 1) % REACTIONS.len();
            self.react_t = 0.0;
            ctx.audio.train_whistle();
            let tip = crate::draw::engine_funnel_tip(ex, fl.engine.y, fl.r_boiler);
            self.confetti.burst(tip, 44, fl.r_boiler * 0.9);
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
        draw::sun(fl.sun_c.x, fl.sun_c.y, fl.sun_r);
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
        // foreground is too short and would crowd the buttons).
        if !f.is_phone() {
            for &(fx, fy, fs) in &[
                (0.10_f32, 0.46_f32, 0.055_f32),
                (0.22, 0.60, 0.042),
                (0.39, 0.50, 0.050),
                (0.55, 0.62, 0.040),
                (0.70, 0.48, 0.052),
            ] {
                draw::plant(content.x + content.w * fx, by + (f.h - by) * fy, f.vmin(fs));
            }
        }

        // Bunting (tablet only) high in the sky.
        if fl.show_bunting {
            draw::bunting(content.x, content.x + content.w, f.h * 0.12, f.h * 0.055, 12, ctx.time);
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
            let seat_cy = body.y + body.h * 0.46 + (ctx.time * 3.0 + i as f32).sin() * 1.5;
            draw_cell(cx, seat_cy, fl.seat, palette::WHITE, palette::CELL_BORDER);
            draw_item(item, cx, seat_cy, fl.seat * 0.78, ctx);
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
        // engine is parked left of the pole, so nothing else occludes it.
        draw::checker_flag(fl.flag_x, by, fl.flag_top, fl.flag_w, fl.flag_h, ctx.time);

        // Replay / Home (phonics-identical placement for cross-finale predictability).
        let white = Color::new(1.0, 1.0, 1.0, 0.94);
        draw::circle_btn(fl.replay.x, fl.replay.y, fl.btn_r, white);
        draw::replay_icon(fl.replay.x, fl.replay.y, fl.btn_r, palette::INK);
        draw::circle_btn(fl.home.x, fl.home.y, fl.btn_r, white);
        draw::house_icon(fl.home.x, fl.home.y, fl.btn_r, palette::INK);
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
        if self.phase == Phase::Finale {
            self.draw_finale(ctx);
            self.confetti.draw();
            return;
        }
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
    // gets a warm ring to read as a distinct rectangle.
    let bw = (size * 0.055).max(2.5);
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
        home: (vec2(tb.x + ir, tb.y + ir), ir),
        mute: (vec2(tb.x + tb.w - ir, tb.y + ir), ir),
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
    replay: Vec2,
    home: Vec2,
    btn_r: f32,
    show_far_hills: bool,
    show_bunting: bool,
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
    let n_cars = if period <= max_fit { period * reps } else { max_fit };
    let pitch0 = (avail / n_cars as f32).min(pitch_of(max_h));
    let car_h = (pitch0 / 1.18 / 1.25).clamp(min_h, max_h);
    let car_w = car_h * 1.25;
    let car_pitch = car_w * 1.18;
    let seat = car_h * 0.62;
    let rightmost_cx = ex - r * 2.05 - car_w * 0.5;
    let leftmost_cx = rightmost_cx - n_cars.saturating_sub(1) as f32 * car_pitch;

    let sun_r = if f.is_phone() { f.vmin(0.08) } else { f.vmin(0.12) };
    let sun_c = vec2(content.x + content.w * 0.80, by - f.vmin(0.05));

    let br = f.icon_btn() / 2.0 * 1.2;
    let m = 30.0 + f.safe.bottom.max(0.0);
    let replay = vec2(f.safe.left + 30.0 + br, f.h - m - br);
    let home = vec2(f.w - (f.safe.right + 30.0 + br), f.h - m - br);

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
        replay,
        home,
        btn_r: br,
        show_far_hills: !f.is_phone(),
        show_bunting: !f.is_phone(),
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
            assert!(fl.n_cars >= 1, "{w}x{h}: no room for even one car");
            // The whole consist (even a period-5 unit) stays inside the content
            // box — the leftmost car never clips off the left edge.
            let left_edge = fl.leftmost_cx - fl.car_w / 2.0;
            assert!(left_edge >= c.x - 0.5, "{w}x{h}: leftmost car {left_edge} clips content left {}", c.x);
        }
    }
}
