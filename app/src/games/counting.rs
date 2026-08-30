//! Counting: a visual aid for rote counting to 30. SCAFFOLD — the whole screen
//! is the tap target: each tap advances the big handwriting-font numeral
//! (1 → 30) with a springy pop, the grown-up and child count aloud together
//! (co-play is the audio channel). Reaching 30 lands in a big confetti finale
//! with the usual replay / home corner buttons. No persistence — every session
//! counts from 1. More (quantity pictures, milestones, characters) comes later.
use crate::{
    anim, chrome, draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
    text,
};
use macroquad::prelude::*;

/// Count target: rote counting to thirty.
const MAX_COUNT: u32 = 30;
/// The springy entrance pop each new number makes.
const POP_DUR: f32 = 0.35;
/// Finale confetti: opening burst + gentle sustained rain.
const FINALE_BURST_N: usize = 150;
const RAIN_INTERVAL_S: f32 = 0.12;
/// How long a finale party firework shell lives, and how many can be in
/// flight at once (older shells recycle — a tap is never refused).
const FIREWORK_S: f32 = 0.8;
const FIREWORKS_MAX: usize = 3;
/// The finale numeral's tapped bounce burst.
const NUMERAL_BURST_N: usize = 26;

/// Tap-target ids for the per-target debounce (a stuttered press must not
/// double-count, but replay/home taps are distinct targets).
const TGT_COUNT: u32 = 1;
const TGT_REPLAY: u32 = 2;
const TGT_HOME: u32 = 3;
const TGT_NUMERAL: u32 = 5;
/// Party fireworks cycle `TGT_PARTY_BASE + (count % FIREWORKS_MAX)` so quick
/// taps in different spots all launch (only a same-slot stutter is swallowed).
const TGT_PARTY_BASE: u32 = 10;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    /// Counting up: the current number fills the screen, a tap advances it.
    Count,
    /// 30 reached: the payoff — confetti + corner replay / home.
    Finale,
}

pub struct CountingScene {
    db: Db,
    n: u32,
    phase: Phase,
    /// Seconds since the current number appeared (drives the entrance pop).
    pop_t: f32,
    debounce: input::TapDebounce,
    confetti: crate::confetti::Confetti,
    /// Accumulator for the finale's sustained confetti rain.
    rain_acc: f32,
    /// Finale: taps on the big "30" itself — each re-pops the numeral, cycles
    /// its color through the rainbow, and climbs the reward chime.
    numeral_taps: u32,
    /// Finale: party fireworks — a recycled pool of shells `(center, t, color
    /// idx)`; `t >= FIREWORK_S` parks a slot. Plus the accepted-tap count
    /// (also the ascending-scale step, so tapping around plays a tune).
    fireworks: [(Vec2, f32, usize); FIREWORKS_MAX],
    party_taps: u32,
}

impl CountingScene {
    pub fn new(db: Db, seed: u32, _now: i64) -> CountingScene {
        CountingScene {
            db,
            n: 1,
            phase: Phase::Count,
            pop_t: 99.0,
            debounce: input::TapDebounce::new(),
            confetti: crate::confetti::Confetti::new(seed ^ 0x00c0_ffee),
            rain_acc: 0.0,
            numeral_taps: 0,
            fireworks: [(vec2(0.0, 0.0), FIREWORK_S, 0); FIREWORKS_MAX],
            party_taps: 0,
        }
    }

    fn restart(&mut self) {
        self.n = 1;
        self.phase = Phase::Count;
        self.pop_t = 0.0;
        self.rain_acc = 0.0;
        self.numeral_taps = 0;
        self.fireworks = [(vec2(0.0, 0.0), FIREWORK_S, 0); FIREWORKS_MAX];
        self.party_taps = 0;
    }

    fn advance(&mut self, ctx: &Ctx) {
        if self.n >= MAX_COUNT {
            // The tap on 30 (after it had its moment on screen) opens the party.
            self.phase = Phase::Finale;
            ctx.audio.finale();
            let f = &ctx.frame;
            self.confetti.burst(vec2(f.w / 2.0, f.h * 0.35), FINALE_BURST_N, f.w * 0.4);
            return;
        }
        self.n += 1;
        self.pop_t = 0.0;
        // The counts CLIMB: within each decade the tick rises strictly with the
        // count ((n-1) % 10 walks the 9-step pentatonic ladder, never wrapping
        // mid-decade), each completed ten lands a proper rising fanfare + a
        // sparkle burst (a real milestone), and 30 opens the big finale.
        if self.n.is_multiple_of(10) {
            ctx.audio.level_up();
            let f = &ctx.frame;
            self.confetti.burst(vec2(f.w / 2.0, f.h * 0.3), 30, f.w * 0.2);
        } else {
            ctx.audio.trace_tick((self.n - 1) % 10);
        }
    }

    // Test/capture hooks (used by --playtest and --capture).
    pub(crate) fn count(&self) -> u32 {
        self.n
    }
    pub(crate) fn in_finale(&self) -> bool {
        self.phase == Phase::Finale
    }
    /// A representative point inside the full-screen count target (below the
    /// topbar so the tap can't hit ← / mute).
    pub(crate) fn tap_target(&self, f: &crate::layout::Frame) -> Vec2 {
        vec2(f.w / 2.0, f.h * 0.55)
    }
    pub(crate) fn replay_center(&self, f: &crate::layout::Frame) -> Vec2 {
        chrome::corner_buttons(f).0
    }
    /// The finale numeral's center (its bounce tap target).
    pub(crate) fn numeral_center(&self, f: &crate::layout::Frame) -> Vec2 {
        vec2(f.w / 2.0, f.h * 0.52)
    }
    pub(crate) fn numeral_taps(&self) -> u32 {
        self.numeral_taps
    }
    /// A finale point outside the numeral + corners (the firework target).
    pub(crate) fn party_point(&self, f: &crate::layout::Frame) -> Vec2 {
        vec2(f.w * 0.12, f.h * 0.15)
    }
    pub(crate) fn party_taps(&self) -> u32 {
        self.party_taps
    }
    /// Pin the current number (capture only) so a golden can show any count.
    pub(crate) fn debug_set_count(&mut self, n: u32) {
        self.n = n.clamp(1, MAX_COUNT);
    }
    /// Jump straight to the finale (capture only).
    pub(crate) fn debug_finish(&mut self, ctx: &Ctx) {
        self.n = MAX_COUNT;
        self.advance(ctx);
    }
}

impl Scene for CountingScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.pop_t += ctx.dt;
        self.confetti.update(ctx.dt);

        if self.phase == Phase::Finale {
            // Sustained celebratory rain for the payoff.
            self.rain_acc += ctx.dt;
            while self.rain_acc >= RAIN_INTERVAL_S {
                self.rain_acc -= RAIN_INTERVAL_S;
                self.confetti.rain(ctx.frame.w, 0.0, 2);
            }
            for fw in &mut self.fireworks {
                if fw.1 < FIREWORK_S {
                    fw.1 += ctx.dt;
                }
            }
            let pt = ctx.pointer;
            if pt.tapped() {
                let f = &ctx.frame;
                let (replay, home_b, br) = chrome::corner_buttons(f);
                // Corner taps are consumed even when the debounce rejects the
                // re-fire, so a stuttered Replay press never leaks a party
                // burst onto the freshly restarted count screen.
                if input::hit_circle(pt.pos, replay.x, replay.y, br) {
                    if self.debounce.accept(TGT_REPLAY, ctx.time) {
                        self.restart();
                    }
                } else if input::hit_circle(pt.pos, home_b.x, home_b.y, br) {
                    if self.debounce.accept(TGT_HOME, ctx.time) {
                        return Nav::Home;
                    }
                } else if input::hit_circle(pt.pos, f.w / 2.0, f.h * 0.52, f.h * 0.30)
                    && self.debounce.accept(TGT_NUMERAL, ctx.time)
                {
                    // The big 30 itself: it re-pops springy, cycles its color
                    // through the rainbow, climbs the reward chime, and throws
                    // gold — the star of the show answers every poke.
                    self.numeral_taps += 1;
                    self.pop_t = 0.0;
                    ctx.audio.correct(self.numeral_taps % 6);
                    self.confetti.burst(
                        vec2(f.w / 2.0, f.h * 0.40),
                        NUMERAL_BURST_N,
                        f.w * 0.12,
                    );
                } else {
                    // Anywhere else launches a FIREWORK under the finger, each
                    // tap a step up a little scale — errorless, re-tappable,
                    // and no pixel of the party is dead.
                    let slot = self.party_taps as usize % FIREWORKS_MAX;
                    if self.debounce.accept(TGT_PARTY_BASE + slot as u32, ctx.time) {
                        self.fireworks[slot] = (pt.pos, 0.0, self.party_taps as usize % 7);
                        ctx.audio.memory_tone(self.party_taps % 7);
                        self.party_taps += 1;
                        self.confetti.burst(pt.pos, 12, f.vmin(0.04));
                    }
                }
            }
            return Nav::Stay;
        }

        match chrome::handle_topbar(&chrome::topbar(&ctx.frame), ctx, &self.db) {
            Some(chrome::TopbarAction::OpenParent) => return Nav::OpenParent,
            Some(chrome::TopbarAction::Home) => return Nav::Home,
            Some(chrome::TopbarAction::MuteToggled) => return Nav::Stay,
            None => {}
        }
        let pt = ctx.pointer;
        if pt.tapped() && self.debounce.accept(TGT_COUNT, ctx.time) {
            self.advance(ctx);
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        clear_background(palette::BG);
        let f = &ctx.frame;

        if self.phase == Phase::Count {
            chrome::draw_topbar(&chrome::topbar(f), ctx);
        }

        // The number, huge and alone (one stimulus, nothing competing). Digits
        // are cap-height (~0.78 em) in the handwriting font, so this font size
        // keeps "30" inside every form factor with generous margins.
        let size = (f.h * 0.78).min(f.w * 0.42) as u16;
        // Springy entrance: overshoot in, settle at 1.
        let pop = if self.pop_t < POP_DUR {
            anim::back_out((self.pop_t / POP_DUR).clamp(0.0, 1.0))
        } else {
            1.0
        };
        // Finale: gold until poked, then each numeral tap steps the color
        // around the rainbow (a small surprise that keeps the 30 alive).
        let color = match self.phase {
            Phase::Count => palette::INK,
            Phase::Finale if self.numeral_taps == 0 => palette::GOLD,
            Phase::Finale => palette::RAINBOW[(self.numeral_taps as usize - 1) % 7],
        };
        text::draw_centered(
            &self.n.to_string(),
            f.w / 2.0,
            f.h * 0.52,
            (size as f32 * (0.85 + 0.15 * pop)) as u16,
            &ctx.fonts.handwriting,
            color,
        );

        if self.phase == Phase::Finale {
            // In-flight party fireworks, blooming wherever the finger landed.
            for &(c, ft, ci) in &self.fireworks {
                if ft < FIREWORK_S {
                    draw::firework(c.x, c.y, f.vmin(0.12), ft / FIREWORK_S, palette::RAINBOW[ci]);
                }
            }
            let (replay, home_b, br) = chrome::corner_buttons(f);
            chrome::draw_corner_buttons(replay, home_b, br);
        }
        self.confetti.draw();
    }
}
