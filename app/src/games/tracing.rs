//! Tracing: finger-trace VicModernCursive letters with the chart's stroke
//! order. Each letter plays an animated pen demo first (watch), then the kid
//! traces freely over the high-contrast glyph (trace): the laid ink is the
//! finger's *actual* path (so a wobbly trace looks wobbly — the parent judges
//! it), with no corridor, no progress tracking and no moving guide point —
//! just the kid drawing over the letter. Errorless: there's no fail state and
//! the letter is never auto-judged "done".
//! The redo / ✗ / ✓ row is always offered: redo replays the demo, ✓ installs
//! the next house part (the progress meter advances only on a ✓, like phonics'
//! rainbow) + celebrates with confetti and promotes the letter, ✗ just
//! reschedules it and moves on (no confetti).
//! Letters come from the shared Leitner SRS over the motor-skill order; state
//! syncs cross-device like phonics.
//! Stroke geometry lives in `fountouki_core::tracing`.
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

/// Demo pen speed (font units / s) and the pause between demo strokes.
const DEMO_SPEED: f32 = 620.0;
const DEMO_PAUSE: f32 = 0.4;
const DEMO_DOT_POP: f32 = 0.45;
/// House-part install timeline, relative to the parent's ✓ grade
/// (`install_t`): the build stage starts at `INSTALL_START` and runs
/// `draw::install_dur(stage)` seconds — stage-specific, since the foundation's
/// dig + pour tells a longer story than a single crane lift. Sound cues come
/// from `draw::install_cues(stage)`. The next letter's demo holds off
/// (`demo_delay`) until this stage plays out, so the build and the demo don't
/// compete for the kid's attention.
const INSTALL_START: f32 = 0.35;
/// Breath between a finished house stage and the next letter's demo.
const INSTALL_BREAK: f32 = 0.7;
/// Glyph-outline alpha (over `palette::INK`): a strong, high-contrast guide
/// that stays clearly visible even with the kid's ink laid over it.
const OUTLINE_ALPHA: f32 = 0.5;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Watch,
    /// The kid draws freely over the glyph while the redo / ✗ / ✓ row stays
    /// offered. The house only gains its part on ✓ — the grade drives both the
    /// Leitner schedule and the progress meter, like phonics.
    Trace,
    /// The session's last ✓ landed: the card empties and the crane hangs the
    /// door — the build's final beat plays out (doorbell and all) before the
    /// house-warming, instead of hard-cutting past it.
    Topping,
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
    /// The finger's actual laid ink for this letter, in font units: one
    /// polyline per drag (broken when the finger lifts or leaves the card).
    /// This is the free-drawn trace — the parent judges it against the glyph.
    laid: Vec<Vec<(f32, f32)>>,
    /// The last `laid` polyline is still being extended.
    laid_open: bool,
    demo_t: f32,
    /// After a ✓-built stage, the next letter's demo waits this many seconds so
    /// the house install (and its breath) finishes first instead of playing on
    /// top of the demo. Zero on a ✗ (no build) — the demo starts at once.
    demo_delay: f32,
    /// Time since the last ✓ grade (drives the house-part install).
    install_t: f32,
    frog_t: f32,
    // --- finale (the house-warming) ---
    /// Seconds since the done scene was entered (entry pops, smoke, flags).
    done_t: f32,
    /// Seconds since the door was last tapped (drives the swing); 99 = idle.
    door_t: f32,
    /// Window lamps are switches, not stars: each tap flips the switch
    /// (`lit_on`) and `lit_warm` eases toward it, so a lamp turns OFF as readily
    /// as on — a light you can actually play with.
    lit_on: [bool; 2],
    lit_warm: [f32; 2],
    /// Door taps this finale (playtest hook).
    door_taps: u32,
    /// The party guests: seconds since each friend frog was tapped (99 = idle).
    friend_t: [f32; 3],
    friend_taps: u32,
    /// Seconds since the sun was tapped (rays burst + spin); 99 = idle.
    sun_t: f32,
    sun_taps: u32,
    /// Seconds since the chimney was tapped (a cough of smoke); 99 = idle.
    chimney_t: f32,
    chimney_taps: u32,
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
            laid: Vec::new(),
            laid_open: false,
            demo_t: 0.0,
            demo_delay: 0.0,
            install_t: 99.0,
            frog_t: 99.0,
            done_t: 0.0,
            door_t: 99.0,
            lit_on: [false; 2],
            lit_warm: [0.0; 2],
            door_taps: 0,
            friend_t: [99.0; 3],
            friend_taps: 0,
            sun_t: 99.0,
            sun_taps: 0,
            chimney_t: 99.0,
            chimney_taps: 0,
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
        self.laid.clear();
        self.laid_open = false;
        self.demo_t = 0.0;
        self.demo_delay = 0.0;
        self.install_t = 99.0;
        self.done_t = 0.0;
        self.door_t = 99.0;
        self.lit_on = [false; 2];
        self.lit_warm = [0.0; 2];
        self.door_taps = 0;
        self.friend_t = [99.0; 3];
        self.friend_taps = 0;
        self.sun_t = 99.0;
        self.sun_taps = 0;
        self.chimney_t = 99.0;
        self.chimney_taps = 0;
    }

    fn start_trace(&mut self) {
        self.phase = Phase::Trace;
        self.laid.clear();
        self.laid_open = false;
    }

    fn on_grade(&mut self, ctx: &Ctx, got_it: bool) {
        let c = self.current();
        if got_it {
            srs::grade_got_it(&mut self.state, c, ctx.now);
            // ✓ builds: the next house part rides the crane down while the
            // next letter's demo plays. The celebration (confetti + the
            // climbing chime) lands only here — a ✗ just moves on.
            self.stars += 1;
            self.traced.push(c);
            self.install_t = 0.0;
            let p = plan(&ctx.frame, c);
            ctx.audio.correct(self.stars);
            self.confetti
                .burst(vec2(p.card.x + p.card.w / 2.0, p.card.y + p.card.h * 0.3), 60, p.card.w / 3.0);
        } else {
            srs::grade_missed(&mut self.state, c, ctx.now);
            ctx.audio.tap();
        }
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        self.advance_letter(ctx);
        // On a ✓ the next letter's demo holds off until the earned house stage
        // has installed (with its sound cues) plus a short breath — so the
        // build plays out fully before the demo, rather than overlapping it.
        if got_it && self.phase == Phase::Watch {
            let stage = (self.stars as usize).saturating_sub(1).min(draw::HOUSE_PARTS - 1);
            self.demo_delay = INSTALL_START + draw::install_dur(stage) + INSTALL_BREAK;
        }
    }

    fn advance_letter(&mut self, ctx: &Ctx) {
        self.last = Some(self.current());
        self.qi += 1;
        if self.stars >= tr::SESSION_GOAL as u32 {
            // The last stage (the door!) installs on-site first; the
            // house-warming follows once it lands (see Phase::Topping).
            self.phase = Phase::Topping;
            return;
        }
        if self.qi >= self.queue.len() {
            self.queue = tr::build_queue(&self.state, ctx.now, &mut self.rng);
            srs::avoid_repeat(&mut self.queue, self.last);
            self.qi = 0;
        }
        self.phase = Phase::Watch;
        self.demo_t = 0.0;
        self.demo_delay = 0.0;
        self.laid.clear();
        self.laid_open = false;
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

    /// Free drawing: lay the finger's actual path as ink while it's down on
    /// the card. No corridor, no green-dot arming, no progress tracking and no
    /// auto-completion — whatever the kid draws shows up (wobbles and all), so
    /// the parent judges the real trace. Lifting (or leaving the card) just
    /// breaks the polyline; nothing is ever undone (errorless).
    fn update_trace(&mut self, ctx: &Ctx) {
        let pt = ctx.pointer;
        let p = plan(&ctx.frame, self.current());
        if !pt.down || !p.card.contains(pt.pos) {
            self.laid_open = false;
            return;
        }
        let finger = p.map.px_to_units(pt.pos);
        if !self.laid_open {
            self.laid.push(Vec::new());
            self.laid_open = true;
        }
        let seg = self.laid.last_mut().unwrap();
        if seg.last().is_none_or(|&l| units_dist(l, finger) >= 4.0) {
            seg.push(finger);
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
            let chim = draw::house_chimney_center(hc.x, hc.y, hs);
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
                // The window lamp is a switch: each tap flips it. Turning it on
                // sparkles; turning it off just clicks (the warmth eases away).
                self.lit_on[i] = !self.lit_on[i];
                ctx.audio.twinkle();
                if self.lit_on[i] {
                    self.confetti.burst(wins[i], 8, hs * 0.12);
                }
            } else if input::hit_circle(pt.pos, chim.x, chim.y, hs * 0.16) {
                // Poke the chimney → it coughs out a puff of smoke.
                self.chimney_t = 0.0;
                self.chimney_taps += 1;
                ctx.audio.train_whistle();
            } else if input::hit_circle(pt.pos, dl.sun_c.x, dl.sun_c.y, dl.sun_r * 1.5) {
                // Tap the sun → rays burst out and the sky sparkles.
                self.sun_t = 0.0;
                self.sun_taps += 1;
                ctx.audio.twinkle();
                self.confetti.burst(dl.sun_c, 12, dl.sun_r * 0.8);
            } else if input::hit_circle(pt.pos, dl.frog_c.x, dl.frog_c.y, dl.frog_r * 1.3)
                && self.frog_t > 0.8
            {
                self.frog_t = 0.0;
                ctx.audio.frog();
                self.confetti.burst(vec2(dl.frog_c.x, dl.frog_c.y - dl.frog_r), 14, dl.frog_r * 0.5);
            } else if let Some(i) = (0..3).find(|&i| {
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
    pub(crate) fn in_trace(&self) -> bool {
        self.phase == Phase::Trace
    }
    /// Whether the kid has laid any free-drawn ink for the current letter.
    pub(crate) fn has_ink(&self) -> bool {
        self.laid.iter().any(|s| !s.is_empty())
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
    /// Pin the build mid-session for captures: `stars` parts earned, with the
    /// newest one's install clock at `install_t` seconds.
    pub(crate) fn debug_set_build(&mut self, stars: u32, install_t: f32) {
        self.stars = stars;
        self.install_t = install_t;
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
        done_layout(f).friends[i.min(2)].0
    }
    pub(crate) fn friend_taps(&self) -> u32 {
        self.friend_taps
    }
    pub(crate) fn sun_center(&self, f: &crate::layout::Frame) -> Vec2 {
        done_layout(f).sun_c
    }
    pub(crate) fn sun_taps(&self) -> u32 {
        self.sun_taps
    }
    pub(crate) fn chimney_center(&self, f: &crate::layout::Frame) -> Vec2 {
        let dl = done_layout(f);
        draw::house_chimney_center(dl.house_c.x, dl.house_c.y, dl.house_s)
    }
    pub(crate) fn chimney_taps(&self) -> u32 {
        self.chimney_taps
    }
    pub(crate) fn stroke_count(&self) -> usize {
        self.glyph().strokes.len()
    }
    /// Screen point at fraction `t` (0..1) along stroke `si` — playtest +
    /// captures feed these as drag positions, exactly like a finger tracing
    /// over the glyph.
    pub(crate) fn stroke_point_px(&self, f: &crate::layout::Frame, si: usize, t: f32) -> Vec2 {
        let p = plan(f, self.current());
        let strokes = self.glyph().strokes;
        let stroke = strokes[si.min(strokes.len() - 1)];
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
        let install_prev = self.install_t;
        self.install_t += ctx.dt;
        self.frog_t += ctx.dt;
        self.done_t += ctx.dt;
        self.door_t += ctx.dt;
        self.sun_t += ctx.dt;
        self.chimney_t += ctx.dt;
        // Each lamp's warmth eases toward its switch (on = 1, off = 0), so it
        // glows up and dims down instead of snapping.
        for i in 0..2 {
            let target = if self.lit_on[i] { 1.0 } else { 0.0 };
            let step = ctx.dt * 4.0;
            self.lit_warm[i] += (target - self.lit_warm[i]).clamp(-step, step);
        }
        for t in &mut self.friend_t {
            *t += ctx.dt;
        }
        // The build stage's sound cues (digger scoops, truck beeps, brick
        // taps, the lands-home thunk + confetti…) fire as install_t crosses
        // them — while the next letter's demo waits on `demo_delay`.
        if self.phase != Phase::Done && self.stars > 0 {
            let stage = (self.stars as usize - 1).min(draw::HOUSE_PARTS - 1);
            let dur = draw::install_dur(stage);
            for &(frac, cue) in draw::install_cues(stage) {
                let at = INSTALL_START + frac * dur;
                if install_prev < at && self.install_t >= at {
                    match cue {
                        draw::BuildCue::Thunk => {
                            ctx.audio.hammer();
                            let p = plan(&ctx.frame, self.current());
                            let a = draw::house_part_anchor(p.house.0.x, p.house.0.y, p.house.1, stage);
                            self.confetti.burst(a, 10, p.house.1 * 0.25);
                        }
                        draw::BuildCue::Tap => ctx.audio.tap(),
                        draw::BuildCue::Digger => ctx.audio.digger(),
                        draw::BuildCue::TruckBeep => ctx.audio.truck_beep(),
                        draw::BuildCue::Twinkle => ctx.audio.twinkle(),
                        draw::BuildCue::Doorbell => ctx.audio.doorbell(),
                    }
                }
            }
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
                // An impatient tap skips straight to tracing — even past the
                // post-build hold.
                if ctx.pointer.tapped() {
                    self.start_trace();
                } else if self.demo_delay > 0.0 {
                    // The earned house stage is still installing: the card waits
                    // (faded letter only) so the build has the kid's attention.
                    self.demo_delay -= ctx.dt;
                } else {
                    self.demo_t += ctx.dt;
                    if self.demo_t >= self.demo_total() {
                        self.start_trace();
                    }
                }
            }
            Phase::Trace => {
                // The redo / ✗ / ✓ row is always live; anything else on the
                // card is free drawing.
                let p = plan(&ctx.frame, self.current());
                let pt = ctx.pointer;
                let hit = |c: (Vec2, f32)| pt.tapped() && input::hit_circle(pt.pos, c.0.x, c.0.y, c.1);
                if hit(p.watch) {
                    // Redo: replay the demo from the top and clear the trace.
                    self.phase = Phase::Watch;
                    self.demo_t = 0.0;
                    self.demo_delay = 0.0;
                    self.laid.clear();
                    self.laid_open = false;
                } else if hit(p.got) {
                    self.on_grade(ctx, true);
                } else if hit(p.miss) {
                    self.on_grade(ctx, false);
                } else {
                    self.update_trace(ctx);
                }
            }
            Phase::Topping => {
                // The door lands (install + a settle beat) → house-warming.
                let dur = draw::install_dur(draw::HOUSE_PARTS - 1);
                if self.install_t >= INSTALL_START + dur + 0.6 {
                    self.phase = Phase::Done;
                    // The frog enters the house-warming mid-hop, celebrating.
                    self.frog_t = 0.0;
                    self.done_t = 0.0;
                    self.sync.flush();
                    ctx.audio.finale();
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

        // The letter, rendered by the real font — a strong, high-contrast guide
        // (it stays clearly visible even with the kid's ink over it). The
        // topping-out beat leaves the card empty: all eyes on the door going in.
        if self.phase != Phase::Topping {
            let glyph = self.current().to_string();
            draw_text_ex(
                &glyph,
                p.map.pen.x,
                p.map.pen.y,
                TextParams {
                    font: Some(&ctx.fonts.cursive),
                    font_size: p.font_px,
                    color: palette::hexa(0x2b2c34, OUTLINE_ALPHA),
                    ..Default::default()
                },
            );
        }

        match self.phase {
            // While the post-build hold runs, the card shows only the guide
            // letter waiting — the demo pen starts once the house is in.
            Phase::Watch if self.demo_delay <= 0.0 => self.draw_demo(&p),
            Phase::Watch => {}
            // The kid's free-drawn ink over the guide — the parent judges this.
            Phase::Trace => self.draw_trace(&p),
            Phase::Topping | Phase::Done => {}
        }

        // The always-offered action row under the card: redo (replay the demo),
        // ✗ (move on, no build) and ✓ (phonics' green check — celebrates AND
        // installs the next house part).
        if self.phase == Phase::Trace {
            draw::circle_btn(p.watch.0.x, p.watch.0.y, p.watch.1, palette::CARD);
            draw::replay_icon(p.watch.0.x, p.watch.0.y, p.watch.1 * 0.9, palette::MUTED);
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

    /// The build state shown beside the card: installed parts = stars (one per
    /// ✓-graded letter), with the newest stage animating in on the `install_t`
    /// timeline that starts at the parent's ✓.
    fn house_pose(&self, time: f32) -> draw::HousePose {
        let stage = (self.stars as usize).saturating_sub(1).min(draw::HOUSE_PARTS - 1);
        let install_end = INSTALL_START + draw::install_dur(stage);
        let (parts, installing) = if self.stars == 0 || self.install_t >= install_end {
            (self.stars as usize, None)
        } else if self.install_t < INSTALL_START {
            // Pre-roll: hold the install at t=0 (crane stages show the part
            // hooked at the park — anticipation) so the blueprint ghost
            // doesn't flash back between the ✓ and the stage start.
            (self.stars as usize - 1, Some(0.0))
        } else {
            (self.stars as usize - 1, Some((self.install_t - INSTALL_START) / draw::install_dur(stage)))
        };
        let smoke_t = if parts >= draw::HOUSE_PARTS { self.install_t - install_end } else { -1.0 };
        draw::HousePose { parts, installing, site: true, smoke_t, time, ..Default::default() }
    }

    /// Small start dot (green) and end dot (red) for stroke `i` — the chart's
    /// convention, kept tiny so they cue start/end without burying the glyph.
    /// Multi-stroke letters get a legible order number beside the start dot.
    fn draw_stroke_dots(&self, p: &TLayout, i: usize) {
        let g = self.glyph();
        let stroke = g.strokes[i];
        let start = p.map.to_px(stroke[0]);
        if !tr::is_dot(stroke) {
            let end = p.map.to_px(*stroke.last().unwrap());
            draw::disc(end.x, end.y, p.dot_r * 0.62, palette::RAINBOW[0]);
        }
        draw::disc(start.x, start.y, p.dot_r, palette::OK_STRONG);
        draw::disc(start.x, start.y, p.dot_r * 0.78, palette::OK);
        if g.strokes.len() > 1 {
            // The dot is too small to hold the number — set it just above-left,
            // in start-green, with a readable floor.
            let fs = (p.dot_r * 2.4).max(14.0);
            text::ui_centered(
                &format!("{}", i + 1),
                start.x - p.dot_r * 1.6,
                start.y - p.dot_r * 1.6,
                fs as u16,
                palette::OK_STRONG,
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
            }
            self.draw_stroke_dots(p, i);
            return;
        }
        // Demo finished — settle frame(s) before the trace phase begins.
        self.draw_ink_full(p);
    }

    /// The trace screen: the kid's free-drawn ink (whatever they drew, wobbles
    /// and all) over the high-contrast glyph, plus small start/end dots for
    /// each stroke. No path tracking, no breadcrumbs, no moving guide point.
    fn draw_trace(&self, p: &TLayout) {
        self.draw_laid_ink(p, 1.0);
        for i in 0..self.glyph().strokes.len() {
            self.draw_stroke_dots(p, i);
        }
    }

    /// The house-warming: the finished house front and center under a sunny
    /// sky, the session's letters strung up as bunting flags, the frog in its
    /// builder's hard hat out front. The door rings + swings, the window lamps
    /// toggle on AND off, the chimney puffs when poked, the sun bursts into
    /// rays, the frogs jump — everything the kid can reach does something.
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
        // The sun sits mid-sky on the right, clear of the letter bunting — tap
        // it and rays burst out, spinning, before settling back to a calm disc.
        let sun_pop = if self.sun_t < 0.6 {
            1.0 + (self.sun_t / 0.6 * std::f32::consts::PI).sin() * 0.18
        } else {
            1.0
        };
        draw::sun_rays(dl.sun_c.x, dl.sun_c.y, dl.sun_r, (1.0 - self.sun_t).max(0.0), ctx.time * 1.5);
        draw::sun(dl.sun_c.x, dl.sun_c.y, dl.sun_r * sun_pop);

        // Ground.
        draw::vgradient(0.0, dl.ground_y, f.w, f.h - dl.ground_y, palette::GROUND_TOP, palette::GROUND_BOT);
        draw_line(0.0, dl.ground_y, f.w, dl.ground_y, 3.0, palette::hex(0x2f7d2f));

        // The finished house — a springy entrance pop, then smoke + lights.
        let pop = anim::back_out(((self.done_t) / 0.5).clamp(0.0, 1.0));
        let hs = dl.house_s * pop.max(0.05);
        let door_open = door_swing(self.door_t);
        let lit = [self.lit_warm[0].clamp(0.0, 1.0), self.lit_warm[1].clamp(0.0, 1.0)];
        let pose = draw::HousePose {
            parts: draw::HOUSE_PARTS,
            installing: None,
            // The site is cleared for the house-warming: crane gone, job done.
            site: false,
            door_open,
            lit,
            smoke_t: (self.done_t - 0.45).max(-1.0),
            puff_t: if self.chimney_t < 0.9 { self.chimney_t } else { -1.0 },
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

        // The party guests: three friend frogs in cone hats, back-left, in the
        // front yard, and a small one front-left. They hop on a lazy ambient
        // cadence and ribbit-jump when tapped (the front ones poke a tongue out).
        for (i, &((fc, fr), (body, hat), phase)) in [
            (dl.friends[0], (palette::RAINBOW[6], palette::GOLD), 2.0f32),
            (dl.friends[1], (palette::RAINBOW[1], palette::RAINBOW[4]), 4.2),
            (dl.friends[2], (palette::RAINBOW[4], palette::RAINBOW[0]), 0.9),
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
                    tongue: if i >= 1 { fly } else { 0.0 },
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
            // Hang each flag perpendicular to the string: rotate by the swag's
            // local tangent (downhill on the left, uphill on the right, upright
            // at the dip) so the bunting reads as one strung line, not a row of
            // upright cards.
            let rot = (sag * 4.0 * (1.0 - 2.0 * t)).atan2(x1 - x0);
            let pivot = vec2(x, top);
            draw::rounded_rect_rot(Rect::new(x - fs / 2.0, top, fs, fs * 1.22), fs * 0.12, pivot, rot, palette::CARD);
            draw::rounded_rect_rot(Rect::new(x - fs / 2.0, top, fs, fs * 0.18), fs * 0.10, pivot, rot, palette::RAINBOW[i % 7]);
            let (sr, cr) = rot.sin_cos();
            text::draw_centered_rot(
                &ch.to_string(),
                x - fs * 0.74 * sr,
                top + fs * 0.74 * cr,
                (fs * 0.62).max(1.0) as u16,
                &ctx.fonts.cursive,
                palette::INK,
                rot,
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
    /// the front yard, and a small one front-left.
    friends: [(Vec2, f32); 3],
    /// The sun (center, radius) — a tappable sky element.
    sun_c: Vec2,
    sun_r: f32,
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
    let fr2 = fr * 0.5;
    DoneLayout {
        ground_y,
        house_c: vec2(cx, base_y),
        house_s: s,
        frog_c: vec2(cx + s * 0.66, frog_base - 0.92 * fr),
        frog_r: fr,
        friends: [
            (vec2(cx - s * 0.72, band(0.40) - 0.92 * fr0), fr0),
            (vec2(cx + s * 0.30, band(0.78) - 0.92 * fr1), fr1),
            (vec2(cx - s * 0.34, band(0.82) - 0.92 * fr2), fr2),
        ],
        sun_c: vec2(f.w * 0.84, ground_y * 0.50),
        sun_r: f.vmin(0.06).max(32.0),
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
    /// Start/end target-dot radius (px) — tiny, so the green/red dots cue
    /// start/end without burying the glyph.
    dot_r: f32,
    /// Redo button (center, radius) — replays the demo.
    watch: (Vec2, f32),
    /// Parent grade buttons: ✗ (smaller) + ✓ — phonics' pair.
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
        // A big writing surface — grow the card (≈2× the old area) while still
        // leaving a margin for the build-a-house meter and a row for the
        // redo / ✗ / ✓ buttons below.
        let card_w = (f.w * 0.46).clamp(320.0, 620.0);
        let card_h = (f.h * 0.72).clamp(280.0, 620.0);
        let card_y = f.h * 0.49 - card_h / 2.0;
        let btn_r = (f.w * 0.038).clamp(30.0, 46.0);
        let by = card_y + card_h + (f.h - (card_y + card_h)) * 0.45;
        (card_w, card_h, card_y, btn_r, by)
    };
    let card = Rect::new(cx - card_w / 2.0, card_y, card_w, card_h);

    // Scale: the full ascender→descender band fits the card height (the same
    // letter proportions across the alphabet), and a wide glyph (m, w)
    // additionally caps on the card width.
    let bb = tr::ink_bbox(tr::glyph(ch).expect("glyph data"));
    let band = tr::ASCENT - tr::DESCENT;
    let scale_h = card_h * 0.92 / band;
    let scale_w = card_w * 0.80 / (bb.2 - bb.0).max(1.0);
    let font_px = ((scale_h.min(scale_w)) * tr::UPEM) as u16;
    let scale = font_px as f32 / tr::UPEM;

    // Pen so this letter's ink is centered in the card both ways (an ascender
    // letter otherwise crams against the top edge while its unused descender
    // zone leaves the bottom third empty); the baseline + x-height guides
    // shift with the letter.
    let baseline = card.y + card_h / 2.0 + (bb.1 + bb.3) / 2.0 * scale;
    let pen_x = cx - (bb.0 + bb.2) / 2.0 * scale;

    // Always-offered action row, evenly spaced around center: redo (replay the
    // demo) on the left, ✗ (smaller, phonics' miss) in the middle, ✓ on the
    // right — ✗/✓ keep phonics' colours so the parent's hand already knows them.
    let slot = btn_r * 2.0;
    let ggap = if phone { 22.0 } else { 34.0 };
    let step = slot + ggap;

    // The build-a-house meter (with its construction site) lives in the right
    // margin beside the card, grounded on the card's bottom edge — same spot
    // every session. The crane's tower head must clear the topbar, so the
    // available headroom also caps the scale.
    let margin = f.w - (card.x + card.w);
    let house_base = card.y + card_h - 2.0;
    let headroom = house_base - (tb.y + tb.h) - 6.0;
    let house_s = (margin * 0.62)
        .min(card_h * 0.66)
        .min(headroom / draw::site_height(1.0))
        .clamp(60.0, 150.0);
    // Center the whole site silhouette in the margin — the crane stands left
    // of the house, so the footprint shifts right of the margin's middle.
    let (site_l, site_r) = draw::site_extents();
    let house_cx = card.x + card.w + margin / 2.0 - (site_l + site_r) / 2.0 * house_s;

    TLayout {
        card,
        font_px,
        map: GlyphMap { pen: vec2(pen_x, baseline), scale },
        ink_w: (64.0 * scale).max(8.0),
        dot_r: (58.0 * scale / 4.0).clamp(2.5, 4.5),
        watch: (vec2(cx - step, by), btn_r),
        miss: (vec2(cx, by), btn_r * 0.66),
        got: (vec2(cx + step, by), btn_r),
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

    /// Each letter's ink is centered in the card — ascender letters (l) must
    /// not cram against the top edge, descender letters (g) not the bottom.
    #[test]
    fn glyph_ink_is_centered_in_the_card() {
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)] {
            for g in fountouki_core::tracing::GLYPHS.iter() {
                let p = plan(&frame(w, h), g.ch);
                let bb = fountouki_core::tracing::ink_bbox(g);
                let top = p.map.to_px((bb.0, bb.3));
                let bot = p.map.to_px((bb.2, bb.1));
                let cx = p.card.x + p.card.w / 2.0;
                let cy = p.card.y + p.card.h / 2.0;
                assert!(
                    ((top.x + bot.x) / 2.0 - cx).abs() < 1.0
                        && ((top.y + bot.y) / 2.0 - cy).abs() < 1.0,
                    "{w}x{h} '{}': ink center ({}, {}) vs card center ({cx}, {cy})",
                    g.ch,
                    (top.x + bot.x) / 2.0,
                    (top.y + bot.y) / 2.0,
                );
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

    /// The in-play construction site must fit its margin on the whole golden
    /// matrix: fully right of the card (counter-jib included), inside the
    /// screen, the crane's tower head below the topbar.
    #[test]
    fn site_fits_beside_the_card() {
        let (sl, sr) = crate::draw::site_extents();
        for (w, h) in [(1194.0, 834.0), (834.0, 1194.0), (844.0, 390.0)] {
            let f = frame(w, h);
            let p = plan(&f, 'm');
            let (c, s) = (p.house.0, p.house.1);
            assert!(c.x + sl * s > p.card.x + p.card.w, "{w}x{h}: site overlaps card");
            assert!(c.x + sr * s < w, "{w}x{h}: site off-screen right");
            let top = c.y - crate::draw::site_height(s);
            let tb = f.topbar();
            assert!(top > tb.y + tb.h, "{w}x{h}: crane head {top} under topbar");
            assert!(c.y <= p.card.y + p.card.h, "{w}x{h}: house floats below card");
        }
    }

    /// One ✓-graded letter per construction stage: a finished session must
    /// finish exactly one house.
    #[test]
    fn house_parts_match_session_goal() {
        assert_eq!(crate::draw::HOUSE_PARTS, fountouki_core::tracing::SESSION_GOAL);
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
