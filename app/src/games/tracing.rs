//! Tracing: finger-trace VicModernCursive letters with the chart's stroke
//! order. Each letter plays an animated pen demo first (watch), then the kid
//! traces over the faded glyph (trace): a stroke arms only when the finger
//! touches the green start dot, the laid ink is the finger's *actual* path
//! (so a wobbly trace looks wobbly — the parent can judge it at grade time),
//! and the stroke completes only once the finger reaches the red end dot.
//! Errorless — a wandering finger just stops laying ink; progress never goes
//! backwards.
//! After the reward beat the parent grades the trace ✓/✗ (grade): ✓ installs
//! the next house part (the progress meter advances only on a correct grade,
//! like phonics' rainbow) and promotes the letter; ✗ only reschedules it.
//! Letters come from the shared Leitner SRS over the motor-skill order; state
//! syncs cross-device like phonics.
//! Stroke geometry + progress logic live in `fountouki_core::tracing`.
use crate::{
    anim, chrome, draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
    text,
};
use fountouki_core::{rng::Mulberry32, srs, tracing as tr};
use macroquad::prelude::*;
use nanoserde::SerJson;

/// Finger corridor half-width + dot tap radius, in font units (UPEM = 1000).
/// ~36 px on an iPad-sized letter: forgiving for small fingers, tight enough
/// that the path still shapes the movement.
const TOL: f32 = 130.0;
/// Demo pen speed (font units / s) and the pause between demo strokes.
const DEMO_SPEED: f32 = 620.0;
const DEMO_PAUSE: f32 = 0.4;
const DEMO_DOT_POP: f32 = 0.45;
/// Reward beat after a finished letter before the parent's grade row appears.
const ADVANCE_BEAT: f32 = 1.1;
/// House-part install timeline, relative to the parent's ✓ grade
/// (`install_t`): the crane lowers the part in starting at `INSTALL_START`,
/// the part is tapped home (hammer clonk) at `HAMMER_AT`, parked by
/// `INSTALL_START + INSTALL_DUR`. It overlaps the next letter's demo —
/// `install_t` keeps counting across the transition.
const INSTALL_START: f32 = 0.3;
const INSTALL_DUR: f32 = 0.9;
const HAMMER_AT: f32 = INSTALL_START + INSTALL_DUR * 0.72;
/// Golden shimmer sweep along the just-finished letter's ink.
const SWEEP_DUR: f32 = 0.55;
/// Laid ink (font units) between sparkle ticks while tracing.
const TICK_EVERY: f32 = 150.0;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Watch,
    Trace,
    /// Letter finished + celebrated; the parent grades it ✓/✗ before the next
    /// one. The finish celebration already happened (errorless), but the
    /// house only gains its part on ✓ — the grade drives both the Leitner
    /// schedule and the progress meter, like phonics.
    Grade,
    Done,
}

pub struct TracingScene {
    db: Db,
    state: srs::LeitnerState,
    rng: Mulberry32,
    queue: Vec<char>,
    qi: usize,
    /// The letter just shown, so a rebuilt queue never repeats it back-to-back.
    last: Option<char>,
    /// Letters graded ✓ this session, in order (drives the done-scene
    /// bunting — the trophies are the letters that built the house).
    traced: Vec<char>,
    pub stars: u32,
    phase: Phase,
    /// Current stroke being demoed/traced + arc-length progress along it.
    stroke_i: usize,
    progress: f32,
    /// The current stroke has been started at its green dot (the gate that
    /// keeps a mid-path touch from laying ink).
    armed: bool,
    /// The finger's actual laid ink for this letter, in font units: one
    /// polyline per engaged drag (broken when the finger lifts or leaves the
    /// corridor). This — not the canonical path — is what's drawn, so the
    /// parent can judge divergence at grade time.
    laid: Vec<Vec<(f32, f32)>>,
    /// The last `laid` polyline is still being extended.
    laid_open: bool,
    demo_t: f32,
    advance_in: Option<f32>,
    /// Time since the current letter finished (drives the completed-glyph pop
    /// and the shimmer sweep).
    finish_t: f32,
    /// Time since the last ✓ grade (drives the house-part install).
    install_t: f32,
    frog_t: f32,
    /// Sparkle-tick accumulator + pentatonic step for the current stroke.
    tick_acc: f32,
    tick_step: u32,
    // --- finale (the house-warming) ---
    /// Seconds since the done scene was entered (entry pops, smoke, flags).
    done_t: f32,
    /// Seconds since the door was last tapped (drives the swing); 99 = idle.
    door_t: f32,
    /// Window lamps: lit stays lit (monotonic); the clock drives the pop.
    lit_on: [bool; 2],
    lit_t: [f32; 2],
    /// Door taps this finale (playtest hook).
    door_taps: u32,
    /// The party guests: seconds since each friend frog was tapped (99 = idle).
    friend_t: [f32; 2],
    friend_taps: u32,
    confetti: crate::confetti::Confetti,
    sync: crate::net::SyncClient,
}

impl TracingScene {
    pub fn new(db: Db, seed: u32, now: i64) -> TracingScene {
        let state = {
            let kv = db.borrow_kv();
            tr::load(&**kv, now)
        };
        let mut rng = Mulberry32::new(seed);
        let mut queue = tr::build_queue(&state, now, &mut rng);
        srs::avoid_repeat(&mut queue, None);
        let sync = crate::net::SyncClient::new(db.clone(), "tracing");
        TracingScene {
            db,
            state,
            rng,
            queue,
            qi: 0,
            last: None,
            traced: Vec::new(),
            stars: 0,
            phase: Phase::Watch,
            stroke_i: 0,
            progress: 0.0,
            armed: false,
            laid: Vec::new(),
            laid_open: false,
            demo_t: 0.0,
            advance_in: None,
            finish_t: 99.0,
            install_t: 99.0,
            frog_t: 99.0,
            tick_acc: 0.0,
            tick_step: 0,
            done_t: 0.0,
            door_t: 99.0,
            lit_on: [false; 2],
            lit_t: [0.0; 2],
            door_taps: 0,
            friend_t: [99.0; 2],
            friend_taps: 0,
            confetti: crate::confetti::Confetti::new(seed ^ 0x7e11_e77a),
            sync,
        }
    }

    fn current(&self) -> char {
        self.queue.get(self.qi).copied().unwrap_or('c')
    }

    fn glyph(&self) -> &'static tr::GlyphTrace {
        tr::glyph(self.current()).expect("traced letter has stroke data")
    }

    fn save(&self) {
        let mut kv = self.db.borrow_kv_mut();
        tr::save(&mut **kv, &self.state);
    }

    fn restart_session(&mut self, now: i64) {
        self.queue = tr::build_queue(&self.state, now, &mut self.rng);
        srs::avoid_repeat(&mut self.queue, self.last);
        self.qi = 0;
        self.traced.clear();
        self.stars = 0;
        self.phase = Phase::Watch;
        self.stroke_i = 0;
        self.progress = 0.0;
        self.demo_t = 0.0;
        self.advance_in = None;
        self.finish_t = 99.0;
        self.install_t = 99.0;
        self.done_t = 0.0;
        self.door_t = 99.0;
        self.lit_on = [false; 2];
        self.lit_t = [0.0; 2];
        self.door_taps = 0;
        self.friend_t = [99.0; 2];
        self.friend_taps = 0;
    }

    fn start_trace(&mut self) {
        self.phase = Phase::Trace;
        self.stroke_i = 0;
        self.progress = 0.0;
        self.armed = false;
        self.laid.clear();
        self.laid_open = false;
        self.tick_acc = 0.0;
        self.tick_step = 0;
    }

    fn on_letter_done(&mut self, ctx: &Ctx) {
        // The kid's celebration is unconditional (errorless) — but the house
        // part waits for the parent's ✓ (on_grade), like phonics' rainbow.
        let p = plan(&ctx.frame, self.current());
        ctx.audio.correct(self.stars + 1);
        self.finish_t = 0.0;
        self.confetti
            .burst(vec2(p.card.x + p.card.w / 2.0, p.card.y + p.card.h * 0.3), 60, p.card.w / 3.0);
        // The reward beat, then the parent's ✓/✗ before the next letter.
        self.advance_in = Some(ADVANCE_BEAT);
    }

    fn on_grade(&mut self, ctx: &Ctx, got_it: bool) {
        let c = self.current();
        if got_it {
            srs::grade_got_it(&mut self.state, c, ctx.now);
            // ✓ builds: the next house part rides the crane down while the
            // next letter's demo plays.
            self.stars += 1;
            self.traced.push(c);
            self.install_t = 0.0;
        } else {
            srs::grade_missed(&mut self.state, c, ctx.now);
        }
        ctx.audio.tap();
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        self.advance_letter(ctx);
    }

    fn advance_letter(&mut self, ctx: &Ctx) {
        self.last = Some(self.current());
        self.qi += 1;
        if self.stars >= tr::SESSION_GOAL as u32 {
            self.phase = Phase::Done;
            // The frog enters the house-warming mid-hop, celebrating.
            self.frog_t = 0.0;
            self.done_t = 0.0;
            self.sync.flush();
            ctx.audio.finale();
            return;
        }
        if self.qi >= self.queue.len() {
            self.queue = tr::build_queue(&self.state, ctx.now, &mut self.rng);
            srs::avoid_repeat(&mut self.queue, self.last);
            self.qi = 0;
        }
        self.phase = Phase::Watch;
        self.demo_t = 0.0;
        self.stroke_i = 0;
        self.progress = 0.0;
        self.finish_t = 99.0;
    }

    /// Demo timeline: per stroke, draw time (length/speed, dots pop) + a pause.
    fn demo_schedule(&self) -> Vec<(f32, f32)> {
        self.glyph()
            .strokes
            .iter()
            .map(|st| {
                let draw = if tr::is_dot(st) {
                    DEMO_DOT_POP
                } else {
                    (tr::stroke_len(st) / DEMO_SPEED).max(0.6)
                };
                (draw, DEMO_PAUSE)
            })
            .collect()
    }

    fn demo_total(&self) -> f32 {
        self.demo_schedule().iter().map(|(d, p)| d + p).sum::<f32>() + 0.3
    }

    fn update_trace(&mut self, ctx: &Ctx) {
        let pt = ctx.pointer;
        if !pt.down {
            self.laid_open = false;
            return;
        }
        let g = self.glyph();
        if self.stroke_i >= g.strokes.len() {
            return;
        }
        let p = plan(&ctx.frame, self.current());
        let finger = p.map.px_to_units(pt.pos);
        let stroke = g.strokes[self.stroke_i];
        // Corridor half-width: TOL font units, but never under ~26 px — on a
        // small phone the letter shrinks, a 4yo's finger doesn't. The start /
        // end gates are tighter than the corridor (with their own px floors):
        // a stroke begins only at the green dot and finishes only at the red.
        let tol = TOL.max(26.0 / p.map.scale);
        let start_r = tr::START_RADIUS.max(30.0 / p.map.scale);
        let end_r = tr::END_RADIUS.max(24.0 / p.map.scale);
        let done = if tr::is_dot(stroke) {
            // The dot of i / j: tap it (a small wiggle of the finger is fine).
            let hit = units_dist(finger, stroke[0]) <= start_r * 1.3;
            if hit {
                // Ink the dot where the finger actually landed.
                self.laid.push(vec![finger]);
                self.laid_open = false;
            }
            hit
        } else {
            if self.progress <= 0.0 && !self.armed {
                // Not started yet: the finger must touch the green dot first.
                if units_dist(finger, stroke[0]) > start_r {
                    self.laid_open = false;
                    return;
                }
                self.armed = true;
            }
            let before = self.progress;
            self.progress = tr::advance_progress(stroke, self.progress, finger, tol);
            // Lay ink along the finger's real path while it stays in the
            // corridor; lifting or wandering breaks the polyline (errorless —
            // it just stops inking, nothing is undone).
            if units_dist(finger, tr::point_at(stroke, self.progress)) <= tol {
                if !self.laid_open {
                    self.laid.push(Vec::new());
                    self.laid_open = true;
                }
                let seg = self.laid.last_mut().unwrap();
                if seg.last().is_none_or(|&l| units_dist(l, finger) >= 4.0) {
                    seg.push(finger);
                }
            } else {
                self.laid_open = false;
            }
            // Laid ink rewards as it grows: a tiny sparkle + a tick that climbs
            // a pentatonic ladder, so the stroke literally sings upward.
            self.tick_acc += (self.progress - before).max(0.0);
            if self.tick_acc >= TICK_EVERY {
                self.tick_acc %= TICK_EVERY;
                ctx.audio.trace_tick(self.tick_step);
                self.tick_step += 1;
                let tip = p.map.to_px(tr::point_at(stroke, self.progress));
                self.confetti.burst(tip, 2, p.start_r * 0.4);
            }
            tr::stroke_done(stroke, self.progress, finger, end_r)
        };
        if done {
            let end = p.map.to_px(*stroke.last().unwrap());
            self.confetti.burst(end, 12, p.start_r);
            self.stroke_i += 1;
            self.progress = 0.0;
            self.armed = false;
            self.laid_open = false;
            self.tick_acc = 0.0;
            self.tick_step = 0;
            if self.stroke_i >= g.strokes.len() {
                self.on_letter_done(ctx);
            } else {
                ctx.audio.tap();
            }
        }
    }

    fn update_done(&mut self, ctx: &Ctx) -> Nav {
        let (replay, home_b, br) = chrome::corner_buttons(&ctx.frame);
        let dl = done_layout(&ctx.frame);
        let (hc, hs) = (dl.house_c, dl.house_s);
        let pt = ctx.pointer;
        if pt.tapped() {
            let door = draw::house_door_rect(hc.x, hc.y, hs);
            let wins = draw::house_window_centers(hc.x, hc.y, hs);
            if input::hit_circle(pt.pos, replay.x, replay.y, br) {
                self.restart_session(ctx.now);
            } else if input::hit_circle(pt.pos, home_b.x, home_b.y, br) {
                self.sync.flush();
                return Nav::Home;
            } else if input::hit_rect(pt.pos, door.x, door.y, door.w, door.h) {
                // Ding-dong! The door swings open with a burst of confetti.
                self.door_t = 0.0;
                self.door_taps += 1;
                ctx.audio.doorbell();
                self.confetti.burst(vec2(door.x + door.w / 2.0, door.y), 18, door.w * 0.6);
            } else if let Some(i) =
                (0..2).find(|&i| input::hit_circle(pt.pos, wins[i].x, wins[i].y, hs * 0.16))
            {
                // A window lamp flicks on (and stays on — monotonic).
                if !self.lit_on[i] {
                    self.lit_on[i] = true;
                    self.lit_t[i] = 0.0;
                }
                ctx.audio.twinkle();
                self.confetti.burst(wins[i], 8, hs * 0.12);
            } else if input::hit_circle(pt.pos, dl.frog_c.x, dl.frog_c.y, dl.frog_r * 1.3)
                && self.frog_t > 0.8
            {
                self.frog_t = 0.0;
                ctx.audio.frog();
                self.confetti.burst(vec2(dl.frog_c.x, dl.frog_c.y - dl.frog_r), 14, dl.frog_r * 0.5);
            } else if let Some(i) = (0..2).find(|&i| {
                let (fc, fr) = dl.friends[i];
                input::hit_circle(pt.pos, fc.x, fc.y, fr * 1.4) && self.friend_t[i] > 0.8
            }) {
                // A party guest ribbits + hops too.
                self.friend_t[i] = 0.0;
                self.friend_taps += 1;
                ctx.audio.frog();
                let (fc, fr) = dl.friends[i];
                self.confetti.burst(vec2(fc.x, fc.y - fr), 10, fr * 0.5);
            }
        }
        Nav::Stay
    }

    // Test hooks (used by --capture / --playtest).
    pub(crate) fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }
    pub(crate) fn in_watch(&self) -> bool {
        self.phase == Phase::Watch
    }
    pub(crate) fn in_grade(&self) -> bool {
        self.phase == Phase::Grade
    }
    pub(crate) fn stroke_index(&self) -> usize {
        self.stroke_i
    }
    pub(crate) fn stroke_progress(&self) -> f32 {
        self.progress
    }
    /// The reward beat between a finished letter and the parent's grade.
    pub(crate) fn awaiting_advance(&self) -> bool {
        self.advance_in.is_some()
    }
    pub(crate) fn current_letter(&self) -> char {
        self.current()
    }
    pub(crate) fn letter_box(&self, c: char) -> u8 {
        self.state.letters.get(&c.to_string()).map(|ls| ls.box_).unwrap_or(0)
    }
    pub(crate) fn skip_watch(&mut self) {
        if self.phase == Phase::Watch {
            self.start_trace();
        }
    }
    pub(crate) fn got_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f, self.current()).got.0
    }
    pub(crate) fn miss_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f, self.current()).miss.0
    }
    pub(crate) fn debug_set_letter(&mut self, c: char) {
        self.queue = vec![c];
        self.qi = 0;
    }
    pub(crate) fn debug_finish_session(&mut self) {
        self.traced = tr::ORDER[..tr::SESSION_GOAL].to_vec();
        self.stars = tr::SESSION_GOAL as u32;
        self.phase = Phase::Done;
        // Skip the entry pops so a single captured frame shows the settled
        // celebration (flags up, chimney smoking).
        self.done_t = 3.0;
    }
    pub(crate) fn door_center(&self, f: &crate::layout::Frame) -> Vec2 {
        let dl = done_layout(f);
        let d = draw::house_door_rect(dl.house_c.x, dl.house_c.y, dl.house_s);
        vec2(d.x + d.w / 2.0, d.y + d.h / 2.0)
    }
    pub(crate) fn window_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        let dl = done_layout(f);
        draw::house_window_centers(dl.house_c.x, dl.house_c.y, dl.house_s)[i.min(1)]
    }
    pub(crate) fn door_taps(&self) -> u32 {
        self.door_taps
    }
    pub(crate) fn window_lit(&self, i: usize) -> bool {
        self.lit_on[i.min(1)]
    }
    pub(crate) fn friend_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        done_layout(f).friends[i.min(1)].0
    }
    pub(crate) fn friend_taps(&self) -> u32 {
        self.friend_taps
    }
    /// Screen point at fraction `t` (0..1) along the current stroke — playtest
    /// feeds these as drag positions, exactly like a finger would.
    pub(crate) fn stroke_point_px(&self, f: &crate::layout::Frame, t: f32) -> Vec2 {
        let p = plan(f, self.current());
        let stroke = self.glyph().strokes[self.stroke_i.min(self.glyph().strokes.len() - 1)];
        let s = tr::stroke_len(stroke) * t.clamp(0.0, 1.0);
        p.map.to_px(tr::point_at(stroke, s))
    }
    pub(crate) fn watch_btn_center(&self, f: &crate::layout::Frame) -> Vec2 {
        plan(f, self.current()).watch.0
    }
}

impl Scene for TracingScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.confetti.update(ctx.dt);
        self.finish_t += ctx.dt;
        let install_prev = self.install_t;
        self.install_t += ctx.dt;
        self.frog_t += ctx.dt;
        self.done_t += ctx.dt;
        self.door_t += ctx.dt;
        for i in 0..2 {
            if self.lit_on[i] {
                self.lit_t[i] += ctx.dt;
            }
            self.friend_t[i] += ctx.dt;
        }
        // The house part lands: a hammer clonk + a confetti kick at the part.
        // Driven by install_t so it fires while the next letter's demo plays.
        if self.phase != Phase::Done && install_prev < HAMMER_AT && self.install_t >= HAMMER_AT {
            ctx.audio.hammer();
            let p = plan(&ctx.frame, self.current());
            let part = (self.stars as usize).saturating_sub(1).min(draw::HOUSE_PARTS - 1);
            let a = draw::house_part_anchor(p.house.0.x, p.house.0.y, p.house.1, part);
            self.confetti.burst(a, 10, p.house.1 * 0.25);
        }
        // Drive cross-device sync: send debounced pushes, and merge the remote
        // blob once the initial pull lands (non-yanking — just updates state).
        self.sync.drive(ctx.now);
        if let Some(remote) = self.sync.poll_pull() {
            if let Some(rstate) = srs::validate(&remote) {
                self.state = srs::merge(&self.state, &rstate, ctx.now);
                self.save();
            }
        }
        if let Some(t) = self.advance_in {
            let t = t - ctx.dt;
            if t <= 0.0 {
                self.advance_in = None;
                self.phase = Phase::Grade;
            } else {
                self.advance_in = Some(t);
            }
            return Nav::Stay;
        }
        if self.phase == Phase::Done {
            return self.update_done(ctx);
        }
        match chrome::handle_topbar(&chrome::topbar(&ctx.frame), ctx, &self.db) {
            Some(chrome::TopbarAction::OpenParent) => return Nav::OpenParent,
            Some(chrome::TopbarAction::Home) => {
                self.sync.flush();
                return Nav::Home;
            }
            Some(chrome::TopbarAction::MuteToggled) => return Nav::Stay,
            None => {}
        }
        match self.phase {
            Phase::Watch => {
                self.demo_t += ctx.dt;
                // An impatient tap skips straight to tracing.
                if self.demo_t >= self.demo_total() || ctx.pointer.tapped() {
                    self.start_trace();
                }
            }
            Phase::Trace => {
                let p = plan(&ctx.frame, self.current());
                let pt = ctx.pointer;
                if pt.tapped() && input::hit_circle(pt.pos, p.watch.0.x, p.watch.0.y, p.watch.1) {
                    self.phase = Phase::Watch;
                    self.demo_t = 0.0;
                    self.stroke_i = 0;
                    self.progress = 0.0;
                } else {
                    self.update_trace(ctx);
                }
            }
            Phase::Grade => {
                let p = plan(&ctx.frame, self.current());
                let pt = ctx.pointer;
                if pt.tapped() {
                    if input::hit_circle(pt.pos, p.got.0.x, p.got.0.y, p.got.1) {
                        self.on_grade(ctx, true);
                    } else if input::hit_circle(pt.pos, p.miss.0.x, p.miss.0.y, p.miss.1) {
                        self.on_grade(ctx, false);
                    }
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
        let p = plan(&ctx.frame, self.current());
        chrome::draw_topbar(&chrome::topbar(&ctx.frame), ctx);

        // The build-a-house progress meter beside the card (monotonic — one
        // part per ✓-graded letter, a finished session = a finished house).
        // Calm and static during play; the install animates after the ✓.
        let pose = self.house_pose(ctx.time);
        draw::house(p.house.0.x, p.house.0.y, p.house.1, &pose);

        draw::card(p.card.x, p.card.y, p.card.w, p.card.h, palette::CARD);
        self.draw_guides(&p);

        // The letter, rendered by the real font — faded while it's a guide,
        // popping to full ink for the just-finished beat.
        let finished = self.finish_t < ADVANCE_BEAT;
        let glyph_col = if finished {
            palette::INK
        } else {
            palette::hexa(0x2b2c34, 0.22)
        };
        let glyph = self.current().to_string();
        draw_text_ex(
            &glyph,
            p.map.pen.x,
            p.map.pen.y,
            TextParams {
                font: Some(&ctx.fonts.cursive),
                font_size: p.font_px,
                color: glyph_col,
                ..Default::default()
            },
        );

        match self.phase {
            Phase::Watch => self.draw_demo(&p),
            Phase::Trace | Phase::Grade => {
                if self.phase == Phase::Trace && !finished {
                    self.draw_trace(&p, ctx);
                } else {
                    // The finished letter pops: the kid's actual ink with a
                    // brief width swell + a golden shimmer sweeping along the
                    // stroke path. The same real ink stays up through the
                    // grade phase, over the faded glyph — that contrast is
                    // what the parent judges ✓/✗ on.
                    let pop = (self.finish_t / 0.45).clamp(0.0, 1.0);
                    let swell = 1.0 + 0.16 * (std::f32::consts::PI * pop).sin();
                    self.draw_laid_ink(&p, swell);
                    self.draw_finish_sweep(&p);
                }
            }
            Phase::Done => {}
        }

        // Action row under the card: while tracing, the watch-again button;
        // while grading, the parent's ✓/✗ (phonics' exact pair — ✓ schedules
        // AND installs the next house part).
        if self.phase == Phase::Trace && !finished {
            draw::circle_btn(p.watch.0.x, p.watch.0.y, p.watch.1, palette::CARD);
            draw::replay_icon(p.watch.0.x, p.watch.0.y, p.watch.1 * 0.9, palette::MUTED);
        }
        if self.phase == Phase::Grade {
            draw::circle_btn(p.miss.0.x, p.miss.0.y, p.miss.1, palette::CARD);
            draw::mark_cross(p.miss.0.x, p.miss.0.y, p.miss.1, palette::MUTED);
            draw::circle_btn(p.got.0.x, p.got.0.y, p.got.1, palette::OK);
            draw::mark_check(p.got.0.x, p.got.0.y, p.got.1, palette::OK_STRONG);
        }

        self.confetti.draw();
    }
}

impl TracingScene {
    /// Handwriting guides on the card: solid baseline + dotted x-height line
    /// ("dotted thirds" lite — two lines, not three, to keep the card calm).
    fn draw_guides(&self, p: &TLayout) {
        let x0 = p.card.x + p.card.w * 0.08;
        let x1 = p.card.x + p.card.w * 0.92;
        let base_y = p.map.pen.y;
        let xh_y = p.map.pen.y - tr::X_HEIGHT * p.map.scale;
        let col = palette::hexa(0x38b3e2, 0.35);
        draw_line(x0, base_y, x1, base_y, 2.0, col);
        let mut x = x0;
        while x < x1 {
            draw_line(x, xh_y, (x + 10.0).min(x1), xh_y, 2.0, palette::hexa(0x38b3e2, 0.22));
            x += 20.0;
        }
    }

    /// The finger's actual laid ink — every engaged drag segment of this
    /// letter (single points, like the dot of i, draw as discs).
    fn draw_laid_ink(&self, p: &TLayout, w_mult: f32) {
        let w = p.ink_w * w_mult;
        for seg in &self.laid {
            if seg.len() == 1 {
                let c = p.map.to_px(seg[0]);
                draw::disc(c.x, c.y, w * 0.62, palette::INK);
            } else {
                let pts: Vec<Vec2> = seg.iter().map(|&u| p.map.to_px(u)).collect();
                draw::stroke_path(&pts, w, palette::INK);
            }
        }
    }

    /// Ink along the canonical stroke path (the demo pen's ink).
    fn draw_stroke_ink(&self, p: &TLayout, stroke: &[(f32, f32)], upto: f32) {
        let ink_w = p.ink_w;
        if tr::is_dot(stroke) {
            let c = p.map.to_px(stroke[0]);
            draw::disc(c.x, c.y, ink_w * 0.62, palette::INK);
            return;
        }
        let total = tr::stroke_len(stroke);
        let upto = upto.min(total);
        if upto <= 0.0 {
            return;
        }
        let mut pts: Vec<Vec2> = Vec::new();
        let mut acc = 0.0;
        pts.push(p.map.to_px(stroke[0]));
        for w in stroke.windows(2) {
            let seg = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            if acc + seg >= upto {
                pts.push(p.map.to_px(tr::point_at(stroke, upto)));
                break;
            }
            acc += seg;
            pts.push(p.map.to_px(w[1]));
        }
        draw::stroke_path(&pts, ink_w, palette::INK);
    }

    fn draw_ink_full(&self, p: &TLayout) {
        for st in self.glyph().strokes {
            self.draw_stroke_ink(p, st, f32::MAX);
        }
    }

    /// A golden sparkle riding the whole letter's stroke path right after it's
    /// finished — the "magic" that turns the trace into ink.
    fn draw_finish_sweep(&self, p: &TLayout) {
        let t = self.finish_t / SWEEP_DUR;
        if !(0.0..1.0).contains(&t) {
            return;
        }
        let strokes = self.glyph().strokes;
        let total: f32 = strokes.iter().filter(|st| !tr::is_dot(st)).map(|st| tr::stroke_len(st)).sum();
        if total <= 0.0 {
            return;
        }
        let mut s = total * anim::ease_out_cubic(t);
        for st in strokes {
            if tr::is_dot(st) {
                continue;
            }
            let l = tr::stroke_len(st);
            if s <= l {
                let tip = p.map.to_px(tr::point_at(st, s));
                let fade = 1.0 - t;
                draw::disc(tip.x, tip.y, p.ink_w * 1.5, palette::hexa(0xf6b800, 0.55 * fade));
                draw::disc(tip.x, tip.y, p.ink_w * 0.7, palette::hexa(0xffffff, 0.9 * fade));
                return;
            }
            s -= l;
        }
    }

    /// The build state shown beside the card: installed parts = stars (one per
    /// ✓-graded letter), with the newest part animating in on the `install_t`
    /// timeline that starts at the parent's ✓.
    fn house_pose(&self, time: f32) -> draw::HousePose {
        let install_end = INSTALL_START + INSTALL_DUR;
        let (parts, installing) = if self.stars == 0 || self.install_t >= install_end {
            (self.stars as usize, None)
        } else if self.install_t < INSTALL_START {
            (self.stars as usize - 1, None)
        } else {
            (self.stars as usize - 1, Some((self.install_t - INSTALL_START) / INSTALL_DUR))
        };
        let smoke_t = if parts >= draw::HOUSE_PARTS { self.install_t - install_end } else { -1.0 };
        draw::HousePose { parts, installing, smoke_t, time, ..Default::default() }
    }

    /// Start dot (green, numbered when the letter has several strokes) and end
    /// dot (red) for stroke `i` — the chart's convention.
    fn draw_stroke_dots(&self, p: &TLayout, i: usize, pulse: f32) {
        let g = self.glyph();
        let stroke = g.strokes[i];
        let start = p.map.to_px(stroke[0]);
        let r = p.start_r * (1.0 + 0.12 * pulse);
        if !tr::is_dot(stroke) {
            let end = p.map.to_px(*stroke.last().unwrap());
            draw::disc(end.x, end.y, p.start_r * 0.55, palette::RAINBOW[0]);
        }
        draw::disc(start.x, start.y, r, palette::OK_STRONG);
        draw::disc(start.x, start.y, r * 0.78, palette::OK);
        if g.strokes.len() > 1 {
            text::ui_centered(
                &format!("{}", i + 1),
                start.x,
                start.y,
                (r * 1.1) as u16,
                palette::WHITE,
            );
        }
    }

    fn draw_demo(&self, p: &TLayout) {
        let g = self.glyph();
        let sched = self.demo_schedule();
        let mut t = self.demo_t;
        for (i, (draw_t, pause)) in sched.iter().enumerate() {
            let stroke = g.strokes[i];
            if t >= draw_t + pause {
                // Fully demoed stroke.
                self.draw_stroke_ink(p, stroke, f32::MAX);
                t -= draw_t + pause;
                continue;
            }
            // The stroke being drawn right now.
            let prog = (t / draw_t).clamp(0.0, 1.0);
            if tr::is_dot(stroke) {
                if prog > 0.4 {
                    self.draw_stroke_ink(p, stroke, f32::MAX);
                }
            } else {
                let s = tr::stroke_len(stroke) * anim::ease_in_out_cubic(prog);
                self.draw_stroke_ink(p, stroke, s);
                // The pen: a real pencil writing the letter, with a soft glow
                // where the ink comes out.
                let tip = p.map.to_px(tr::point_at(stroke, s));
                draw::disc(tip.x, tip.y, p.start_r * 0.8, palette::hexa(0xf582ae, 0.45));
                draw::pencil(tip.x, tip.y, p.start_r * 5.4);
            }
            self.draw_stroke_dots(p, i, 0.0);
            return;
        }
        // Demo finished — settle frame(s) before the trace phase begins.
        self.draw_ink_full(p);
    }

    fn draw_trace(&self, p: &TLayout, ctx: &Ctx) {
        let g = self.glyph();
        // The ink is the finger's real path (completed strokes included) — a
        // wobble inside the corridor stays visible instead of snapping to the
        // perfect glyph, so the trace can actually be judged.
        self.draw_laid_ink(p, 1.0);
        if self.stroke_i >= g.strokes.len() {
            return;
        }
        let stroke = g.strokes[self.stroke_i];
        // Breadcrumb dots from the pen position to the end of the stroke — the
        // route reminder for parts the faded glyph shows but direction doesn't
        // (retraces), without the noise of arrows.
        if !tr::is_dot(stroke) {
            let total = tr::stroke_len(stroke);
            let mut s = self.progress + 70.0;
            while s < total - 30.0 {
                let c = p.map.to_px(tr::point_at(stroke, s));
                draw::disc(c.x, c.y, p.ink_w * 0.16, palette::hexa(0x2b2c34, 0.3));
                s += 70.0;
            }
            // Pen position marker once underway: where to keep the finger.
            if self.progress > 0.0 {
                let tip = p.map.to_px(tr::point_at(stroke, self.progress));
                draw::disc(tip.x, tip.y, p.start_r * 0.7, palette::ACCENT);
            }
        }
        // Start dot pulses until the stroke is underway.
        let pulse = if self.progress <= 0.0 {
            anim::pulse(ctx.time, 1.1)
        } else {
            0.0
        };
        if self.progress <= 0.0 || tr::is_dot(stroke) {
            self.draw_stroke_dots(p, self.stroke_i, pulse.max(0.0));
        } else {
            // Underway: keep only the red end target visible.
            let end = p.map.to_px(*stroke.last().unwrap());
            draw::disc(end.x, end.y, p.start_r * 0.55, palette::RAINBOW[0]);
        }
    }

    /// The house-warming: the finished house front and center under a sunny
    /// sky, the session's letters strung up as bunting flags, the frog in its
    /// builder's hard hat out front. The door rings + swings, windows light up,
    /// the frog jumps — everything the kid can reach does something.
    fn draw_done(&mut self, ctx: &Ctx) {
        let f = &ctx.frame;
        let dl = done_layout(f);
        clear_background(palette::BG);
        draw::vgradient(0.0, 0.0, f.w, dl.ground_y, palette::SKY_TOP, palette::SKY_BOT);
        // A steady celebratory drizzle.
        self.confetti.rain(f.w, -10.0, 2);

        // Ambient sky: drifting clouds + the sun (phonics' celebration sky).
        let cloud_r = f.vmin(0.045).max(22.0);
        let span = f.w + cloud_r * 8.0;
        for &(hy, sc, spd, ph) in &[(0.30f32, 1.05f32, 9.0f32, 0.12f32), (0.52, 0.7, 14.0, 0.55), (0.22, 0.85, 6.5, 0.82)] {
            let x = (ctx.time * spd + ph * span).rem_euclid(span) - cloud_r * 4.0;
            draw::cloud(x, dl.ground_y * hy, cloud_r * sc);
        }
        // The sun sits mid-sky on the right, clear of the letter bunting.
        draw::sun(f.w * 0.84, dl.ground_y * 0.50, f.vmin(0.06).max(32.0));

        // Ground.
        draw::vgradient(0.0, dl.ground_y, f.w, f.h - dl.ground_y, palette::GROUND_TOP, palette::GROUND_BOT);
        draw_line(0.0, dl.ground_y, f.w, dl.ground_y, 3.0, palette::hex(0x2f7d2f));

        // The finished house — a springy entrance pop, then smoke + lights.
        let pop = anim::back_out(((self.done_t) / 0.5).clamp(0.0, 1.0));
        let hs = dl.house_s * pop.max(0.05);
        let door_open = door_swing(self.door_t);
        let lit = [
            if self.lit_on[0] { (self.lit_t[0] * 2.5).clamp(0.0, 1.0) } else { 0.0 },
            if self.lit_on[1] { (self.lit_t[1] * 2.5).clamp(0.0, 1.0) } else { 0.0 },
        ];
        let pose = draw::HousePose {
            parts: draw::HOUSE_PARTS,
            installing: None,
            door_open,
            lit,
            smoke_t: (self.done_t - 0.45).max(-1.0),
            time: ctx.time,
        };
        draw::house(dl.house_c.x, dl.house_c.y, hs, &pose);
        // A welcome path from the door (over the grass mound).
        draw::fill_ellipse(dl.house_c.x, dl.house_c.y + hs * 0.085, hs * 0.20, hs * 0.045, 0.0, palette::hexa(0xfff3d6, 0.9));

        // A couple of garden plants at the edges (tablet only — a phone's
        // foreground is too short).
        if !f.is_phone() {
            draw::plant(f.w * 0.07, dl.ground_y + (f.h - dl.ground_y) * 0.42, f.vmin(0.045));
            draw::plant(f.w * 0.92, dl.ground_y + (f.h - dl.ground_y) * 0.35, f.vmin(0.052));
        }

        // The letters this session wrote, strung up as bunting flags — the
        // trophies ARE the letters (no words needed).
        self.draw_letter_flags(ctx, &dl);

        // The party guests: two friend frogs in cone hats, back-left and in
        // the front yard. They hop on a lazy ambient cadence and ribbit-jump
        // when tapped (the front one pokes its tongue out).
        for (i, &((fc, fr), (body, hat), phase)) in [
            (dl.friends[0], (palette::RAINBOW[6], palette::GOLD), 2.0f32),
            (dl.friends[1], (palette::RAINBOW[1], palette::RAINBOW[4]), 4.2),
        ]
        .iter()
        .enumerate()
        {
            let pose = if self.friend_t[i] < 0.7 {
                let fly = (self.friend_t[i] / 0.7 * std::f32::consts::PI).sin();
                draw::FrogPose {
                    dy: -fly * fr * 1.1,
                    sy: 1.0 + fly * 0.18,
                    sx: 1.0 - fly * 0.09,
                    tongue: if i == 1 { fly } else { 0.0 },
                    ..Default::default()
                }
            } else {
                let amb = (ctx.time + phase).rem_euclid(5.6);
                if amb < 0.7 {
                    let fly = (amb / 0.7 * std::f32::consts::PI).sin();
                    draw::FrogPose { dy: -fly * fr * 0.55, sy: 1.0 + fly * 0.12, sx: 1.0 - fly * 0.06, ..Default::default() }
                } else {
                    let breathe = (ctx.time * 1.85 + phase).sin();
                    draw::FrogPose { sx: 1.0 - 0.025 * breathe, sy: 1.0 + 0.03 * breathe, ..Default::default() }
                }
            };
            draw::frog(fc.x, fc.y, fr, body, pose);
            draw::frog_party_hat(fc.x, fc.y, fr, pose, hat);
        }

        // The builder frog hosts out front (hard hat on); tap = jump.
        let fpose = if self.frog_t < 0.8 {
            let pr = (self.frog_t / 0.8).clamp(0.0, 1.0);
            let fly = ((pr - 0.1).max(0.0) / 0.8 * std::f32::consts::PI).sin();
            draw::FrogPose { dy: -fly * dl.frog_r * 1.2, sy: 1.0 + fly * 0.2, sx: 1.0 - fly * 0.1, ..Default::default() }
        } else {
            let breathe = (ctx.time * 1.85).sin();
            draw::FrogPose { sx: 1.0 - 0.025 * breathe, sy: 1.0 + 0.03 * breathe, ..Default::default() }
        };
        draw::frog(dl.frog_c.x, dl.frog_c.y, dl.frog_r, palette::RAINBOW[3], fpose);
        draw::frog_hard_hat(dl.frog_c.x, dl.frog_c.y, dl.frog_r, fpose);

        let (replay, home_b, br) = chrome::corner_buttons(f);
        chrome::draw_corner_buttons(replay, home_b, br);
    }

    /// The session's letters on rectangular flags hanging from a swag line,
    /// popping in one by one with a gentle sway.
    fn draw_letter_flags(&self, ctx: &Ctx, dl: &DoneLayout) {
        let (x0, x1, y, sag) = dl.bunt;
        let yat = |t: f32| y + sag * 4.0 * t * (1.0 - t);
        const SEG: usize = 40;
        let mut line = Vec::with_capacity(SEG + 1);
        for i in 0..=SEG {
            let t = i as f32 / SEG as f32;
            line.push(vec2(x0 + (x1 - x0) * t, yat(t)));
        }
        draw::stroke_path(&line, 3.0, palette::hexa(0x6f5a4a, 0.8));
        let n = self.traced.len().max(1);
        for (i, ch) in self.traced.iter().enumerate() {
            let t = (i as f32 + 0.5) / n as f32;
            let popt = ((self.done_t - 0.30 - 0.13 * i as f32) / 0.4).clamp(0.0, 1.0);
            if popt <= 0.0 {
                continue;
            }
            let sc = anim::back_out(popt);
            let fs = dl.flag_s * sc;
            let x = x0 + (x1 - x0) * t + (ctx.time * 1.6 + i as f32 * 1.3).sin() * 2.0;
            let top = yat(t);
            draw::rounded_rect(x - fs / 2.0, top, fs, fs * 1.22, fs * 0.12, palette::CARD);
            draw::rounded_rect(x - fs / 2.0, top, fs, fs * 0.18, fs * 0.10, palette::RAINBOW[i % 7]);
            text::draw_centered(
                &ch.to_string(),
                x,
                top + fs * 0.74,
                (fs * 0.62).max(1.0) as u16,
                &ctx.fonts.cursive,
                palette::INK,
            );
        }
    }
}

/// Euclidean distance between two font-unit points.
fn units_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    vec2(a.0 - b.0, a.1 - b.1).length()
}

/// Door swing for `door_t` seconds after a tap: springs open, holds, eases
/// shut — ready for the next ring.
fn door_swing(door_t: f32) -> f32 {
    if !(0.0..1.45).contains(&door_t) {
        0.0
    } else if door_t < 0.22 {
        anim::back_out(door_t / 0.22).clamp(0.0, 1.2)
    } else if door_t < 1.0 {
        1.0
    } else {
        1.0 - anim::ease_in_out_cubic((door_t - 1.0) / 0.45)
    }
}

/// Geometry for the house-warming finale, derived from the viewport like
/// every other layout.
struct DoneLayout {
    ground_y: f32,
    /// House footprint center (x) + ground line (y).
    house_c: Vec2,
    house_s: f32,
    frog_c: Vec2,
    frog_r: f32,
    /// Party guests (center, radius): one behind-left of the house, one in
    /// the front yard.
    friends: [(Vec2, f32); 2],
    flag_s: f32,
    /// Bunting swag: x0, x1, top y, center sag.
    bunt: (f32, f32, f32, f32),
}

fn done_layout(f: &crate::layout::Frame) -> DoneLayout {
    let ground_y = f.h * 0.68;
    let base_y = ground_y + (f.h - ground_y) * 0.18;
    let flag_s = f.vmin(0.11).clamp(40.0, 78.0);
    let bunt = (f.w * 0.08, f.w * 0.92, f.h * 0.045, f.h * 0.05);
    // The chimney top must clear the bunting band below the flags.
    let s_cap = (base_y - (bunt.2 + bunt.3 + flag_s * 1.3 + 10.0)) / draw::house_height(1.0);
    let s_want = if f.is_portrait() { f.w * 0.52 } else { f.vmin(0.42) };
    let s = s_want.clamp(140.0, 380.0).min(s_cap);
    let cx = f.w * 0.46;
    let fr = (s * 0.22).clamp(34.0, 95.0);
    // The host stands at the house's front-right corner (drawn after the
    // house, so it reads as "out front"); the guests flank the party.
    let frog_base = ground_y + (f.h - ground_y) * 0.55;
    let band = |frac: f32| ground_y + (f.h - ground_y) * frac;
    let fr0 = fr * 0.72;
    let fr1 = fr * 0.62;
    DoneLayout {
        ground_y,
        house_c: vec2(cx, base_y),
        house_s: s,
        frog_c: vec2(cx + s * 0.66, frog_base - 0.92 * fr),
        frog_r: fr,
        friends: [
            (vec2(cx - s * 0.72, band(0.40) - 0.92 * fr0), fr0),
            (vec2(cx + s * 0.30, band(0.78) - 0.92 * fr1), fr1),
        ],
        flag_s,
        bunt,
    }
}

/// Maps font units (y up, origin at the pen/baseline) ↔ screen px. `scale` is
/// derived from the integer font_size actually rasterized, so the baked stroke
/// paths land exactly on the rendered glyph.
pub(crate) struct GlyphMap {
    pub pen: Vec2,
    pub scale: f32,
}

impl GlyphMap {
    pub fn to_px(&self, p: (f32, f32)) -> Vec2 {
        vec2(self.pen.x + p.0 * self.scale, self.pen.y - p.1 * self.scale)
    }
    pub fn px_to_units(&self, px: Vec2) -> (f32, f32) {
        ((px.x - self.pen.x) / self.scale, (self.pen.y - px.y) / self.scale)
    }
}

struct TLayout {
    card: Rect,
    font_px: u16,
    map: GlyphMap,
    /// Traced-ink width (px) ≈ the glyph's own stroke weight.
    ink_w: f32,
    /// Start-dot radius (px).
    start_r: f32,
    /// Watch-again button (center, radius).
    watch: (Vec2, f32),
    /// Parent grade buttons (grade phase): ✗ left, ✓ right — phonics' pair.
    miss: (Vec2, f32),
    got: (Vec2, f32),
    /// Build-a-house progress meter: footprint center + ground line, width.
    house: (Vec2, f32),
}

fn plan(f: &crate::layout::Frame, ch: char) -> TLayout {
    let cx = f.w / 2.0;
    let tb = f.topbar();
    let phone = f.is_phone();

    let (card_w, card_h, card_y, btn_r, by) = if phone {
        let btn_r = (f.h * 0.10).clamp(24.0, 44.0);
        let by = f.h - f.safe.bottom.max(8.0) - btn_r - 6.0;
        let top = tb.y + tb.h + 30.0; // 30px reserves the star row
        let card_bottom = by - btn_r - 10.0;
        let card_h = (card_bottom - top).clamp(120.0, 300.0);
        let card_w = (card_h * 1.15).max(f.w * 0.3);
        (card_w, card_h, card_bottom - card_h, btn_r, by)
    } else {
        let card_w = (f.w * 0.42).clamp(320.0, 560.0);
        let card_h = (f.h * 0.5).clamp(280.0, 470.0);
        let card_y = f.h * 0.49 - card_h / 2.0;
        let btn_r = (f.w * 0.038).clamp(30.0, 46.0);
        let by = card_y + card_h + (f.h - (card_y + card_h)) * 0.45;
        (card_w, card_h, card_y, btn_r, by)
    };
    let card = Rect::new(cx - card_w / 2.0, card_y, card_w, card_h);

    // Scale: the full ascender→descender band fits the card height (the same
    // baseline + guide lines for every letter), and a wide glyph (m, w)
    // additionally caps on the card width.
    let bb = tr::ink_bbox(tr::glyph(ch).expect("glyph data"));
    let band = tr::ASCENT - tr::DESCENT;
    let scale_h = card_h * 0.92 / band;
    let scale_w = card_w * 0.80 / (bb.2 - bb.0).max(1.0);
    let font_px = ((scale_h.min(scale_w)) * tr::UPEM) as u16;
    let scale = font_px as f32 / tr::UPEM;

    // Baseline so the asc→desc band is vertically centered in the card; pen x
    // centers this letter's ink.
    let baseline = card.y + card_h / 2.0 + (tr::ASCENT + tr::DESCENT) / 2.0 * scale;
    let pen_x = cx - (bb.0 + bb.2) / 2.0 * scale;

    // Grade row: ✗ (smaller) left of ✓, the same arrangement as phonics so
    // the parent's hand already knows it.
    let slot = btn_r * 2.0;
    let ggap = if phone { 22.0 } else { 34.0 };
    let gx0 = cx - (2.0 * slot + ggap) / 2.0;

    // The build-a-house meter lives in the right margin beside the card,
    // grounded on the card's bottom edge — same spot every session.
    let margin = f.w - (card.x + card.w);
    let house_s = (margin * 0.62).min(card_h * 0.66).clamp(60.0, 150.0);
    let house_cx = card.x + card.w + margin / 2.0;
    let house_base = card.y + card_h - 2.0;

    TLayout {
        card,
        font_px,
        map: GlyphMap { pen: vec2(pen_x, baseline), scale },
        ink_w: (64.0 * scale).max(8.0),
        start_r: (90.0 * scale).clamp(12.0, 26.0),
        watch: (vec2(cx, by), btn_r),
        miss: (vec2(gx0 + slot / 2.0, by), btn_r * 0.66),
        got: (vec2(gx0 + slot + ggap + slot / 2.0, by), btn_r),
        house: (vec2(house_cx, house_base), house_s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Frame, Insets};

    fn frame(w: f32, h: f32) -> Frame {
        Frame::new(w, h, Insets::default())
    }

    /// Every letter's ink must land inside the card on the whole golden matrix
    /// (ascenders, descenders and the wide m included).
    #[test]
    fn glyphs_fit_inside_the_card() {
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)] {
            for g in fountouki_core::tracing::GLYPHS.iter() {
                let p = plan(&frame(w, h), g.ch);
                let bb = fountouki_core::tracing::ink_bbox(g);
                for c in [(bb.0, bb.1), (bb.2, bb.3)] {
                    let px = p.map.to_px(c);
                    assert!(
                        px.x >= p.card.x - 2.0
                            && px.x <= p.card.x + p.card.w + 2.0
                            && px.y >= p.card.y - 2.0
                            && px.y <= p.card.y + p.card.h + 2.0,
                        "{w}x{h} '{}': ink corner {px:?} outside card {:?}",
                        g.ch,
                        p.card
                    );
                }
            }
        }
    }

    #[test]
    fn map_roundtrips() {
        let p = plan(&frame(1194.0, 834.0), 'a');
        let u = (123.4, -56.7);
        let back = p.map.px_to_units(p.map.to_px(u));
        assert!((back.0 - u.0).abs() < 0.01 && (back.1 - u.1).abs() < 0.01);
    }

    /// The in-play house must fit its margin on the whole golden matrix:
    /// fully right of the card, inside the screen, chimney below the topbar.
    #[test]
    fn house_fits_beside_the_card() {
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)] {
            let f = frame(w, h);
            let p = plan(&f, 'm');
            let (c, s) = (p.house.0, p.house.1);
            assert!(c.x - s * 0.5 > p.card.x + p.card.w, "{w}x{h}: house overlaps card");
            assert!(c.x + s * 0.5 < w, "{w}x{h}: house off-screen right");
            let top = c.y - crate::draw::house_height(s);
            let tb = f.topbar();
            assert!(top > tb.y + tb.h, "{w}x{h}: chimney {top} under topbar");
            assert!(c.y <= p.card.y + p.card.h, "{w}x{h}: house floats below card");
        }
    }

    /// Finale layout: the house clears the letter bunting and the frog stays
    /// on screen, on every device shape.
    #[test]
    fn done_layout_fits_every_device() {
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)] {
            let f = frame(w, h);
            let dl = done_layout(&f);
            let house_top = dl.house_c.y - crate::draw::house_height(dl.house_s);
            let flags_bottom = dl.bunt.2 + dl.bunt.3 + dl.flag_s * 1.22;
            assert!(house_top > flags_bottom, "{w}x{h}: house {house_top} into flags {flags_bottom}");
            assert!(dl.frog_c.x + dl.frog_r < w, "{w}x{h}: frog off-screen");
            assert!(dl.frog_c.y + dl.frog_r * 1.1 < h, "{w}x{h}: frog under bottom edge");
            for (i, &(fc, fr)) in dl.friends.iter().enumerate() {
                assert!(fc.x - fr > 0.0 && fc.x + fr < w, "{w}x{h}: friend {i} off-screen x");
                assert!(fc.y + fr * 1.1 < h, "{w}x{h}: friend {i} under bottom edge");
            }
        }
    }

    /// The door swing rings fully open then settles fully shut.
    #[test]
    fn door_swing_opens_and_closes() {
        assert_eq!(door_swing(99.0), 0.0);
        assert!(door_swing(0.5) >= 1.0);
        assert!(door_swing(1.44) < 0.2);
        assert_eq!(door_swing(2.0), 0.0);
    }
}
