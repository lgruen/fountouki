//! Tracing: finger-trace VicModernCursive letters with the chart's stroke
//! order. Each letter plays an animated pen demo first (watch), then the kid
//! traces over the faded glyph (trace): a green dot marks the start, a red dot
//! the end, and ink follows the finger along a generous corridor. Errorless —
//! a wandering finger just stops laying ink; progress never goes backwards.
//! After the reward beat the parent grades the trace ✓/✗ (grade) — scheduling
//! only, the star already happened. Letters come from the shared Leitner SRS
//! over the motor-skill order; state syncs cross-device like phonics.
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
/// Reward beat after a finished letter before the next one appears.
const ADVANCE_BEAT: f32 = 1.1;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Watch,
    Trace,
    /// Letter finished + celebrated; the parent grades it ✓/✗ before the next
    /// one. The grade only drives the Leitner schedule — the star is already
    /// the kid's (monotonic, errorless).
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
    /// Letters finished this session, in order (drives the done-scene cards).
    traced: Vec<char>,
    pub stars: u32,
    phase: Phase,
    /// Current stroke being demoed/traced + arc-length progress along it.
    stroke_i: usize,
    progress: f32,
    demo_t: f32,
    advance_in: Option<f32>,
    /// Time since the current letter finished (drives the completed-glyph pop).
    finish_t: f32,
    frog_t: f32,
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
            demo_t: 0.0,
            advance_in: None,
            finish_t: 99.0,
            frog_t: 99.0,
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
    }

    fn start_trace(&mut self) {
        self.phase = Phase::Trace;
        self.stroke_i = 0;
        self.progress = 0.0;
    }

    fn on_letter_done(&mut self, ctx: &Ctx) {
        let p = plan(&ctx.frame, self.current());
        self.stars += 1;
        ctx.audio.correct(self.stars);
        self.finish_t = 0.0;
        self.traced.push(self.current());
        self.confetti
            .burst(vec2(p.card.x + p.card.w / 2.0, p.card.y + p.card.h * 0.3), 60, p.card.w / 3.0);
        // The reward beat, then the parent's ✓/✗ before the next letter.
        self.advance_in = Some(ADVANCE_BEAT);
    }

    fn on_grade(&mut self, ctx: &Ctx, got_it: bool) {
        let c = self.current();
        if got_it {
            srs::grade_got_it(&mut self.state, c, ctx.now);
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
            self.frog_t = 99.0;
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
        // small phone the letter shrinks, a 4yo's finger doesn't.
        let tol = TOL.max(26.0 / p.map.scale);
        let done = if tr::is_dot(stroke) {
            // The dot of i / j: tap it (a small wiggle of the finger is fine).
            let d = vec2(finger.0 - stroke[0].0, finger.1 - stroke[0].1);
            d.length() <= tol * 1.3
        } else {
            self.progress = tr::advance_progress(stroke, self.progress, finger, tol);
            tr::stroke_done(stroke, self.progress)
        };
        if done {
            let end = p.map.to_px(*stroke.last().unwrap());
            self.confetti.burst(end, 12, p.start_r);
            self.stroke_i += 1;
            self.progress = 0.0;
            if self.stroke_i >= g.strokes.len() {
                self.on_letter_done(ctx);
            } else {
                ctx.audio.tap();
            }
        }
    }

    fn update_done(&mut self, ctx: &Ctx) -> Nav {
        let (replay, home_b, br) = chrome::corner_buttons(&ctx.frame);
        let (frog_c, fr) = done_frog(&ctx.frame);
        let pt = ctx.pointer;
        if pt.tapped() {
            if input::hit_circle(pt.pos, replay.x, replay.y, br) {
                self.restart_session(ctx.now);
            } else if input::hit_circle(pt.pos, home_b.x, home_b.y, br) {
                self.sync.flush();
                return Nav::Home;
            } else if input::hit_circle(pt.pos, frog_c.x, frog_c.y, fr * 1.3) && self.frog_t > 0.8 {
                self.frog_t = 0.0;
                ctx.audio.frog();
                self.confetti.burst(vec2(frog_c.x, frog_c.y - fr), 14, fr * 0.5);
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

        // Session stars (monotonic), centered above the card.
        let n = tr::SESSION_GOAL;
        let sr = (p.card.w * 0.045).clamp(10.0, 16.0);
        let sgap = sr * 2.6;
        let sx0 = p.card.x + p.card.w / 2.0 - (n as f32 - 1.0) * sgap / 2.0;
        let sy = p.card.y - sr * 1.9;
        for i in 0..n {
            let c = if (i as u32) < self.stars { palette::GOLD } else { palette::PIP_EMPTY };
            draw::star(sx0 + i as f32 * sgap, sy, sr, c);
        }

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
                    self.draw_ink_full(&p);
                }
            }
            Phase::Done => {}
        }

        // Action row under the card: while tracing, the watch-again button;
        // while grading, the parent's ✓/✗ (phonics' exact pair — scheduling
        // only, the star is already earned).
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

    fn draw_stroke_ink(&self, p: &TLayout, stroke: &[(f32, f32)], upto: f32) {
        if tr::is_dot(stroke) {
            let c = p.map.to_px(stroke[0]);
            draw::disc(c.x, c.y, p.ink_w * 0.62, palette::INK);
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
        draw::stroke_path(&pts, p.ink_w, palette::INK);
    }

    fn draw_ink_full(&self, p: &TLayout) {
        for st in self.glyph().strokes {
            self.draw_stroke_ink(p, st, f32::MAX);
        }
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
                // The pen: a bright dot riding the stroke tip.
                let tip = p.map.to_px(tr::point_at(stroke, s));
                draw::disc(tip.x, tip.y, p.start_r * 0.9, palette::ACCENT);
                draw::disc(tip.x, tip.y, p.start_r * 0.55, palette::WHITE);
            }
            self.draw_stroke_dots(p, i, 0.0);
            return;
        }
        // Demo finished — settle frame(s) before the trace phase begins.
        self.draw_ink_full(p);
    }

    fn draw_trace(&self, p: &TLayout, ctx: &Ctx) {
        let g = self.glyph();
        for i in 0..self.stroke_i.min(g.strokes.len()) {
            self.draw_stroke_ink(p, g.strokes[i], f32::MAX);
        }
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
            self.draw_stroke_ink(p, stroke, self.progress);
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

    fn draw_done(&mut self, ctx: &Ctx) {
        let f = &ctx.frame;
        clear_background(palette::BG);
        draw::vgradient(0.0, 0.0, f.w, f.h * 0.55, palette::SKY_TOP, palette::BG);
        // A steady celebratory drizzle.
        self.confetti.rain(f.w, -10.0, 2);

        text::draw_centered(
            "yay!",
            f.w / 2.0,
            f.h * 0.16,
            (f.vmin(0.09)) as u16,
            &ctx.fonts.cursive,
            palette::OK_STRONG,
        );

        // The letters this session wrote, popping in one by one on mini cards.
        let n = self.traced.len() as f32;
        let cw = (f.w * 0.11).clamp(70.0, 120.0);
        let gap = cw * 0.22;
        let x0 = f.w / 2.0 - (n * cw + (n - 1.0) * gap) / 2.0;
        let cy = f.h * 0.42;
        for (i, ch) in self.traced.iter().enumerate() {
            let t = ((ctx.time - 0.12 * i as f32) / 0.45).clamp(0.0, 1.0);
            if t <= 0.0 {
                continue;
            }
            let s = anim::back_out(t);
            let w = cw * s;
            let x = x0 + i as f32 * (cw + gap) + cw / 2.0;
            draw::card(x - w / 2.0, cy - w * 0.62, w, w * 1.24, palette::CARD);
            text::draw_centered(
                &ch.to_string(),
                x,
                cy,
                (cw * 0.78 * s) as u16,
                &ctx.fonts.cursive,
                palette::INK,
            );
        }

        // The frog celebrates below the letters; tap it for another jump.
        let (frog_c, fr) = done_frog(f);
        let pose = if self.frog_t < 0.8 {
            let pr = (self.frog_t / 0.8).clamp(0.0, 1.0);
            let fly = ((pr - 0.1).max(0.0) / 0.8 * std::f32::consts::PI).sin();
            draw::FrogPose { dy: -fly * fr * 1.2, sy: 1.0 + fly * 0.2, sx: 1.0 - fly * 0.1, ..Default::default() }
        } else {
            let breathe = (ctx.time * 1.85).sin();
            draw::FrogPose { sx: 1.0 - 0.025 * breathe, sy: 1.0 + 0.03 * breathe, ..Default::default() }
        };
        draw::frog(frog_c.x, frog_c.y, fr, palette::RAINBOW[3], pose);

        let (replay, home_b, br) = chrome::corner_buttons(f);
        chrome::draw_corner_buttons(replay, home_b, br);
    }
}

fn done_frog(f: &crate::layout::Frame) -> (Vec2, f32) {
    let fr = f.vmin(0.10).clamp(50.0, 120.0);
    (vec2(f.w / 2.0, f.h * 0.78), fr)
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

    TLayout {
        card,
        font_px,
        map: GlyphMap { pen: vec2(pen_x, baseline), scale },
        ink_w: (64.0 * scale).max(8.0),
        start_r: (90.0 * scale).clamp(12.0, 26.0),
        watch: (vec2(cx, by), btn_r),
        miss: (vec2(gx0 + slot / 2.0, by), btn_r * 0.66),
        got: (vec2(gx0 + slot + ggap + slot / 2.0, by), btn_r),
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
}
