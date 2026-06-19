//! Sing Back: a Simon-says memory game. Four pads form a choir — each pad binds
//! a pitch ↔ a rainbow color ↔ a critter (frog/duck/cat/owl), low→high in both
//! pitch and color so the row reads as a warm-to-cool scale. A round runs a
//! soft count-in (Ready), plays a growing sequence (Show), the kid taps it back
//! (Input), and every completed round celebrates (Reward) before appending one
//! fresh pad and replaying the whole prefix + the new note. Completing a round
//! of `FINALE_SPAN` triggers the big closing celebration (Finale).
//!
//! No in-play text: the child can't read, so phase/turn are signalled with
//! VISUALS + AUDIO only. The memory task itself stays CALM — ambient motion
//! (blink/bob/breathing) freezes during Show + Input so only the lit pad (Show)
//! or the tapped pad (Input) ever animates; life + joy go to Ready, Reward and
//! the Finale.
//!
//! Errorless + monotonic: a miss never shortens or dead-ends — the correct pad
//! wiggles to teach, then the SAME sequence replays. The only persisted+synced
//! thing is the best span ever reproduced (`core::singback::SingBackState`);
//! the live sequence + the session streak are session-only plain fields.
//! Sync wiring mirrors tracing/phonics (pull+merge on mount, push on a new best,
//! flush on every leave path).
use crate::{
    anim, chrome, draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    store::Db,
};
use fountouki_core::{rng::Mulberry32, settings::load_singback, singback as sb};
use macroquad::prelude::*;
use nanoserde::SerJson;

/// The four choir members, pad index 0..4 (low→high pitch).
const CRITTERS: [draw::Critter; 4] =
    [draw::Critter::Frog, draw::Critter::Duck, draw::Critter::Cat, draw::Critter::Owl];
/// Each pad's color: `palette::RAINBOW[PAD_COLOR_IDX[i]]` — warm (low pitch) to
/// cool (high), so pitch and color climb together (the whole point of the game).
const PAD_COLOR_IDX: [usize; 4] = [0, 2, 3, 6];

/// Per-difficulty beat timing (seconds the pad is lit / the gap between beats)
/// during the Show playback, and the starting sequence length. The Ready
/// count-in now runs at its OWN fixed brisk tempo (`READY_TICK_S`), independent
/// of this difficulty timing, so a slow difficulty can't drag the lead-in.
///
/// Reveal slowed from the first playtest (a 4yo lost the thread): normal now
/// on 0.62s + gap 0.34s (was 0.52 + 0.22); gentle slower still; speedy stays
/// deliberate rather than snappy.
struct Tuning {
    on: f32,
    gap: f32,
    start_len: usize,
}

fn tuning(difficulty: &str) -> Tuning {
    match difficulty {
        "gentle" => Tuning { on: 0.82, gap: 0.40, start_len: 2 },
        "speedy" => Tuning { on: 0.52, gap: 0.26, start_len: 3 },
        _ => Tuning { on: 0.62, gap: 0.34, start_len: 2 }, // normal
    }
}

/// The Ready count-in: `READY_TICKS` soft metronome pulses (a synchronized
/// pulse of all four critters + a soft central ring + an audio tick on each) at
/// a fixed BRISK spacing (`READY_TICK_S`, NOT scaled by difficulty — a slow
/// difficulty mustn't make the lead-in drag), then a short still beat
/// (`READY_STILL_S`), then Show begins on the downbeat. A clear non-text
/// "ready-set-go" of ~1.35s total so a 4yo doesn't lose patience before the
/// sequence even starts — never an instant cold start.
const READY_TICKS: u32 = 3;
/// Fixed brisk spacing between count-in ticks (seconds) — difficulty-independent.
const READY_TICK_S: f32 = 0.40;
/// The still beat after the last tick, before Show begins on the downbeat.
const READY_STILL_S: f32 = 0.55;
/// Total count-in duration: ticks at 0,1,2·`READY_TICK_S` then the still beat —
/// ~1.35s of "ready-set-go".
const READY_TOTAL_S: f32 = (READY_TICKS as f32 - 1.0) * READY_TICK_S + READY_STILL_S;
/// Sequence lengths UNDER which (or whenever `gentle`) a freshly-appended pad
/// is rerolled so it never equals the immediately-preceding pad — same-critter-
/// twice-in-a-row confuses beginners. At/beyond this length, repeats are allowed
/// (the added challenge of a held note). `pub(crate)` so the --playtest asserts
/// against the same threshold instead of a duplicated literal.
pub(crate) const EASY_NO_REPEAT_LEN: usize = 5;
/// Completing a round of this span fires the Finale (the big closing payoff).
const FINALE_SPAN: u32 = 6;

/// How long the Miss teaching beat (the correct pad's wiggle) holds before the
/// same sequence replays.
const MISS_DUR: f32 = 1.3;
/// How long the Reward celebration holds before the next round appends + plays.
const REWARD_DUR: f32 = 1.6;
/// How long the one-shot "your turn" welcome bounce runs when Input opens (after
/// it, every critter sits perfectly still until tapped).
const TURN_BOUNCE_DUR: f32 = 0.55;
/// `flash_t` value parked far past any on-beat to mean "nothing is lit".
const FLASH_IDLE: f32 = 99.0;

/// Distinct tap-debounce target ids. Pads use their index 0..4; the rest start
/// past that so a bounce only collapses a re-fire of the SAME target while a
/// fast tap on a different one always lands (see [`input::TapDebounce`]).
const TGT_REPLAY: u32 = 10;
const TGT_FINALE_REPLAY: u32 = 20;
const TGT_FINALE_HOME: u32 = 21;

/// Confetti-rain pump cadence: one piece every `RAIN_INTERVAL_S` of accumulated
/// time, shared by the Reward (new-best) escalation and the Finale.
const RAIN_INTERVAL_S: f32 = 0.10;

/// The reward/finale star-pop curve: `back_out(t / STAR_POP_DUR)` capped at
/// `STAR_POP_CAP` for the springy overshoot that both share.
const STAR_POP_DUR: f32 = 0.45;
const STAR_POP_CAP: f32 = 1.25;
/// The reward (new-best) star is drawn at `star_r * pop * NEW_BEST_SCALE`.
const NEW_BEST_SCALE: f32 = 1.5;
/// The finale star is drawn at `star_r * FINALE_STAR_SCALE * pop * throb`.
const FINALE_STAR_SCALE: f32 = 1.8;
/// The finale throb is `1.0 + FINALE_THROB_AMP * pulse(..).max(0.0)`, peaking at
/// `1.0 + FINALE_THROB_AMP` when the pulse tops out.
const FINALE_THROB_AMP: f32 = 0.06;
/// The LARGEST multiplier the drawn star radius can ever reach over its base
/// `star_r`, across both celebrations at their animated PEAK. Reward new-best
/// peaks at `STAR_POP_CAP * NEW_BEST_SCALE` = 1.875; the finale peaks at
/// `FINALE_STAR_SCALE * STAR_POP_CAP * (1 + FINALE_THROB_AMP)` = 2.385 — the
/// finale is the true max.
///
/// The star is BOTTOM-ANCHORED (grows upward from a fixed lower edge above the
/// heads), so its bottom never covers a face regardless of pop. layout() uses
/// this as the cap that keeps even the PEAK upward growth
/// (`star_r * STAR_PEAK_SCALE` tall) inside the headroom above the anchor — so
/// the celebratory pop is big yet never overruns the topbar.
const STAR_PEAK_SCALE: f32 =
    FINALE_STAR_SCALE * STAR_POP_CAP * (1.0 + FINALE_THROB_AMP);

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    /// The count-in before Show: `t` = seconds in. All four critters pulse +
    /// a soft ring expands on each of `READY_TICKS` beats at the fixed brisk
    /// `READY_TICK_S` spacing, then a `READY_STILL_S` beat, then Show begins on
    /// the downbeat. This is the only place ambient "it's starting" energy lives
    /// before the task.
    Ready { t: f32 },
    /// Playing the sequence back to the kid: `idx` = which sequence step is
    /// lighting now, `t` = seconds into that step (on-beat then gap). Ambient
    /// motion is frozen — ONLY the lit pad animates.
    Show { idx: usize, t: f32 },
    /// The kid's turn: `got` correct taps so far. `t` runs the one-shot welcome
    /// bounce (then everything sits still until tapped). A tap lights its pad +
    /// plays its tone; a right tap advances `got`, a wrong one → Miss.
    Input { got: usize, t: f32 },
    /// A miss: the correct pad (`sequence[got]`) wiggles to teach, `t` seconds
    /// in; at MISS_DUR a fresh count-in (Ready) precedes the same replay (never
    /// shortens). `got` is the step the kid failed.
    Miss { got: usize, t: f32 },
    /// A completed round: star pop + confetti + critters hop, `t` seconds in.
    /// At REWARD_DUR a fresh pad appends and a new count-in begins.
    Reward { t: f32 },
    /// The closing celebration after a `FINALE_SPAN` round: all four critters
    /// dance, confetti rains, a big trophy/star pops. `t` = seconds in. Replay
    /// restarts the session; Home leaves.
    Finale { t: f32 },
}

pub struct SingbackScene {
    db: Db,
    rng: Mulberry32,
    state: sb::SingBackState,
    tuning: Tuning,
    /// True on the `gentle` difficulty: the easy-stage no-repeat rule applies for
    /// the WHOLE session (not just len < EASY_NO_REPEAT_LEN).
    gentle: bool,
    /// The live sequence (pad indices), Simon-style: each round APPENDS one
    /// fresh random pad. Session-only — never persisted.
    sequence: Vec<u8>,
    /// Consecutive completed rounds this session (drives the climbing reward
    /// chime). Session-only.
    streak: u32,
    phase: Phase,
    /// Seconds since the active pad last flashed (drives its pop/glow/sing in
    /// both Show and Input), and which pad is flashing. `flash_pad` is `None`
    /// when nothing is lit.
    flash_t: f32,
    flash_pad: Option<usize>,
    /// Set on the Reward beat that raised `best_span` — escalates the
    /// celebration (rain + level-up chime + a bigger star).
    new_best: bool,
    /// Steady-rain accumulator for the new-best escalation.
    rain_acc: f32,
    /// Gates pad/replay/finale-corner taps PER-TARGET so one physical press
    /// never registers twice on the same target (a bounce) — but a fast tap on a
    /// *different* pad still lands. Driven off `ctx.time`, which in interactive
    /// play is the wall clock (`get_time()`); captures/play-tests inject it.
    tap_debounce: input::TapDebounce,
    confetti: crate::confetti::Confetti,
    sync: crate::net::SyncClient,
}

impl SingbackScene {
    pub fn new(db: Db, seed: u32, now: i64) -> SingbackScene {
        let difficulty = {
            let kv = db.borrow_kv();
            load_singback(&**kv).difficulty
        };
        let gentle = difficulty == "gentle";
        let tuning = tuning(&difficulty);
        let state = {
            let kv = db.borrow_kv();
            sb::load(&**kv, now)
        };
        let mut rng = Mulberry32::new(seed);
        let mut sequence = Vec::new();
        for _ in 0..tuning.start_len {
            sequence.push(rng.below(4) as u8);
        }
        let sync = crate::net::SyncClient::new(db.clone(), "singback");
        let mut sc = SingbackScene {
            db,
            rng,
            state,
            tuning,
            gentle,
            sequence,
            streak: 0,
            phase: Phase::Ready { t: 0.0 },
            flash_t: FLASH_IDLE,
            flash_pad: None,
            new_best: false,
            rain_acc: 0.0,
            tap_debounce: input::TapDebounce::new(),
            // Separate confetti stream, salted distinctly from `seed` so it is
            // genuinely independent (and never perturbs the sequence RNG).
            confetti: crate::confetti::Confetti::new(seed.wrapping_add(0x9E37_79B9)),
            sync,
        };
        // Apply the easy-stage no-repeat rule to the initial sequence too.
        sc.dedupe_initial();
        sc
    }

    /// Reroll any adjacent duplicate in the freshly-built starting sequence, so
    /// the easy-stage no-repeat invariant holds from the very first round.
    fn dedupe_initial(&mut self) {
        for i in 1..self.sequence.len() {
            while self.sequence[i] == self.sequence[i - 1] {
                self.sequence[i] = self.rng.below(4) as u8;
            }
        }
    }

    fn len(&self) -> usize {
        self.sequence.len()
    }

    fn save(&self) {
        let mut kv = self.db.borrow_kv_mut();
        sb::save(&mut **kv, &self.state);
    }

    /// Light pad `i`: pop/glow/sing animation + its tone at onset.
    fn flash(&mut self, i: usize, ctx: &Ctx) {
        self.flash_pad = Some(i);
        self.flash_t = 0.0;
        ctx.audio.memory_tone(i as u32);
    }

    /// Reset to "nothing lit" — used everywhere a phase change clears the flash.
    fn clear_flash(&mut self) {
        self.flash_pad = None;
        self.flash_t = FLASH_IDLE;
    }

    /// Pump the steady confetti rain: drop one piece per `RAIN_INTERVAL_S` of
    /// accumulated time. Shared by the Reward (new-best) escalation + the Finale.
    fn pump_rain(&mut self, dt: f32, w: f32) {
        self.rain_acc += dt;
        while self.rain_acc > RAIN_INTERVAL_S {
            self.confetti.rain(w, -10.0, 1);
            self.rain_acc -= RAIN_INTERVAL_S;
        }
    }

    /// A tap of pad `p` during the kid's turn.
    fn on_tap(&mut self, p: usize, ctx: &Ctx) {
        let Phase::Input { got, .. } = self.phase else { return };
        self.flash(p, ctx);
        if self.sequence.get(got).copied() == Some(p as u8) {
            let got = got + 1;
            if got >= self.len() {
                self.enter_reward(ctx);
            } else {
                // Keep the turn open; the bounce stays finished (t past dur) so
                // only the just-tapped pad animates.
                self.phase = Phase::Input { got, t: TURN_BOUNCE_DUR };
            }
        } else {
            // Non-punitive miss: a soft cue + the correct pad teaches by wiggle.
            ctx.audio.incorrect();
            self.phase = Phase::Miss { got, t: 0.0 };
        }
    }

    /// Begin a count-in lead-in, then Show. Entered on game start, after a Miss,
    /// and at the start of every new (grown) round — the sequence NEVER starts
    /// cold. Reseeds the tap debounce so the first turn tap always lands.
    fn enter_ready(&mut self) {
        self.phase = Phase::Ready { t: 0.0 };
        self.clear_flash();
        self.tap_debounce = input::TapDebounce::new();
    }

    /// Open the kid's turn: a one-shot welcome bounce, then still. Reseeds the
    /// debounce so the first tap of the turn can't be swallowed.
    fn enter_input(&mut self) {
        self.phase = Phase::Input { got: 0, t: 0.0 };
        self.clear_flash();
        self.tap_debounce = input::TapDebounce::new();
    }

    /// A round was completed: celebrate (Reward), or — at `FINALE_SPAN` — fire
    /// the big Finale. Records the span (maybe a new best) either way.
    fn enter_reward(&mut self, ctx: &Ctx) {
        self.streak += 1;
        let span = self.len() as u32;
        let was_best = self.state.best_span;
        sb::record_span(&mut self.state, span, ctx.now);
        self.new_best = self.state.best_span > was_best;
        self.flash_pad = None;
        if span >= FINALE_SPAN {
            self.enter_finale(ctx);
            return;
        }
        // Layout only matters on the reward path (the finale recomputes its own).
        let lay = layout(&ctx.frame);
        self.phase = Phase::Reward { t: 0.0 };
        // A fuller burst: more pieces over a wider radius so the reward fills
        // more of the frame (the star sits high now, so spread it generously).
        self.confetti.burst(vec2(lay.star.x, lay.star.y), 140, lay.pad * 1.1);
        if self.new_best {
            // Escalate: bigger fanfare + persist & push the new best.
            ctx.audio.level_up();
            self.save();
            self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        } else {
            ctx.audio.correct(self.streak);
        }
    }

    /// The closing payoff: a `FINALE_SPAN` round completed (always a new best).
    /// Persists + pushes, then dances. Reseeds the debounce for the corner taps.
    fn enter_finale(&mut self, ctx: &Ctx) {
        self.phase = Phase::Finale { t: 0.0 };
        self.flash_pad = None;
        self.clear_flash();
        self.tap_debounce = input::TapDebounce::new();
        ctx.audio.finale();
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        let f = &ctx.frame;
        let lay = layout(f);
        self.confetti.burst(vec2(lay.star.x, lay.star.y), 180, lay.pad * 1.4);
    }

    /// Restart the session from the Finale: sequence back to the difficulty's
    /// start length, streak cleared, count-in begins. `best_span` is untouched
    /// (monotonic / persisted).
    fn restart(&mut self) {
        self.sequence.clear();
        for _ in 0..self.tuning.start_len {
            self.sequence.push(self.rng.below(4) as u8);
        }
        self.dedupe_initial();
        self.streak = 0;
        self.new_best = false;
        self.rain_acc = 0.0;
        // Fresh, independent confetti stream for the new session.
        let cseed = (self.rng.next_f64() * u32::MAX as f64) as u32;
        self.confetti = crate::confetti::Confetti::new(cseed);
        self.enter_ready();
    }

    /// Append a fresh random pad, then run a fresh count-in + replay. In the easy
    /// stage (len < EASY_NO_REPEAT_LEN, or `gentle`) the new pad is rerolled so
    /// it never equals the immediately-preceding one.
    fn grow_and_replay(&mut self) {
        self.sequence.push(self.rng.below(4) as u8);
        self.dedupe_easy_tail();
        self.new_best = false;
        self.enter_ready();
    }

    /// In the easy stage, reroll the LAST pad until it differs from the one
    /// before it (no same-critter-twice-in-a-row for beginners). A no-op past
    /// the easy stage, where repeats are allowed challenge.
    fn dedupe_easy_tail(&mut self) {
        let easy = self.sequence.len() < EASY_NO_REPEAT_LEN || self.gentle;
        if !easy || self.sequence.len() < 2 {
            return;
        }
        let prev = self.sequence[self.sequence.len() - 2];
        let last = self.sequence.last_mut().unwrap();
        while *last == prev {
            *last = self.rng.below(4) as u8;
        }
    }

    // Test hooks (used by --playtest / --capture).
    /// Center of pad `i` for the current frame (only main.rs's --playtest uses it).
    pub(crate) fn pad_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        pad_center(f, i)
    }
    pub(crate) fn sequence(&self) -> &[u8] {
        &self.sequence
    }
    pub(crate) fn best_span(&self) -> u32 {
        self.state.best_span
    }
    pub(crate) fn in_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready { .. })
    }
    pub(crate) fn in_input(&self) -> bool {
        matches!(self.phase, Phase::Input { .. })
    }
    pub(crate) fn in_reward(&self) -> bool {
        matches!(self.phase, Phase::Reward { .. })
    }
    pub(crate) fn in_miss(&self) -> bool {
        matches!(self.phase, Phase::Miss { .. })
    }
    pub(crate) fn in_finale(&self) -> bool {
        matches!(self.phase, Phase::Finale { .. })
    }
    pub(crate) fn got(&self) -> usize {
        match self.phase {
            Phase::Input { got, .. } => got,
            _ => 0,
        }
    }
    /// Force the kid's turn now (skip the count-in + watch playback) — playtest
    /// convenience. Opens Input through the real path (reseeds the debounce).
    pub(crate) fn skip_to_input(&mut self) {
        self.enter_input();
    }
    pub(crate) fn replay_center(&self, f: &crate::layout::Frame) -> Vec2 {
        layout(f).replay
    }
}

impl Scene for SingbackScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        self.flash_t += ctx.dt;
        self.confetti.update(ctx.dt);

        // Drive cross-device sync: send debounced pushes, merge the remote blob
        // once the initial pull lands (non-yanking — just updates the best span).
        self.sync.drive(ctx.now);
        if let Some(remote) = self.sync.poll_pull() {
            if let Some(rstate) = sb::validate(&remote) {
                self.state = sb::merge(&self.state, &rstate, ctx.now);
                self.save();
                // If the merged state carries info the server lacks (a higher
                // best or a newer reset), push it so a dumb last-write server
                // converges and a parent reset propagates. Only fires when
                // strictly ahead of the pulled blob, so it can't loop: once the
                // server echoes our state back, merged == rstate and we stop.
                if self.state != rstate {
                    self.sync.queue_push(&self.state.serialize_json(), ctx.now);
                }
            }
        }

        // The Finale is a full-screen celebration that draws NO topbar — handle
        // it (its own corner replay/home) FIRST and return, so the invisible
        // topbar's top-corner hit targets are never consulted during it (a
        // top-left tap must NOT silently go Home / open parent over the dance).
        if let Phase::Finale { t } = self.phase {
            // A steady, gentle confetti rain over the dance.
            self.pump_rain(ctx.dt, ctx.frame.w);
            self.phase = Phase::Finale { t: t + ctx.dt };
            let pt = ctx.pointer;
            if pt.tapped() {
                let (replay, home, br) = chrome::corner_buttons(&ctx.frame);
                if input::hit_circle(pt.pos, replay.x, replay.y, br) {
                    if self.tap_debounce.accept(TGT_FINALE_REPLAY, ctx.time) {
                        self.restart();
                    }
                } else if input::hit_circle(pt.pos, home.x, home.y, br)
                    && self.tap_debounce.accept(TGT_FINALE_HOME, ctx.time)
                {
                    self.sync.flush();
                    return Nav::Home;
                }
            }
            return Nav::Stay;
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
            Phase::Ready { t } => {
                // Count-in: a soft audio tick on each of READY_TICKS beats at the
                // fixed brisk READY_TICK_S spacing (a clear "ready-set-go", NOT
                // scaled by difficulty), then a short still beat, then Show on the
                // downbeat. The all-critter pulse + central ring are drawn off `t`.
                let prev = t;
                let t = t + ctx.dt;
                for k in 0..READY_TICKS {
                    let beat = k as f32 * READY_TICK_S;
                    if prev <= beat && t > beat {
                        ctx.audio.tap();
                    }
                }
                if t >= READY_TOTAL_S {
                    self.phase = Phase::Show { idx: 0, t: 0.0 };
                } else {
                    self.phase = Phase::Ready { t };
                }
            }
            Phase::Show { idx, t } => {
                let on = self.tuning.on;
                let gap = self.tuning.gap;
                // Flash the step at its onset (t crosses 0), then hold + gap.
                if t == 0.0 {
                    let pad = self.sequence[idx] as usize;
                    self.flash(pad, ctx);
                }
                let t = t + ctx.dt;
                if t >= on + gap {
                    let idx = idx + 1;
                    if idx >= self.len() {
                        // Whole sequence shown → the kid's turn.
                        self.enter_input();
                    } else {
                        self.phase = Phase::Show { idx, t: 0.0 };
                    }
                } else {
                    self.phase = Phase::Show { idx, t };
                }
            }
            Phase::Input { got, t } => {
                // Run the one-shot welcome-bounce timer (drawn off `t`).
                self.phase = Phase::Input { got, t: t + ctx.dt };
                let pt = ctx.pointer;
                // Gate through the debounce so one physical press never fires two
                // taps (consume the clock only when a tap actually lands on a
                // target, so a tap into dead space doesn't burn the window).
                if pt.tapped() {
                    let lay = layout(&ctx.frame);
                    if input::hit_circle(pt.pos, lay.replay.x, lay.replay.y, lay.btn_r) {
                        if self.tap_debounce.accept(TGT_REPLAY, ctx.time) {
                            // Re-show from the top (with a fresh count-in).
                            self.enter_ready();
                        }
                    } else {
                        for (i, c) in lay.pads.iter().enumerate() {
                            if input::hit_circle(pt.pos, c.x, c.y, lay.pad * 0.5) {
                                // Per-target id = pad index, so a fast tap on a
                                // DIFFERENT pad lands while a bounce on the same
                                // pad (which would mis-fire a Miss) is dropped.
                                if self.tap_debounce.accept(i as u32, ctx.time) {
                                    self.on_tap(i, ctx);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            Phase::Miss { got, t } => {
                let t = t + ctx.dt;
                if t >= MISS_DUR {
                    // Re-show the same sequence through a fresh count-in.
                    self.enter_ready();
                } else {
                    self.phase = Phase::Miss { got, t };
                }
            }
            Phase::Reward { t } => {
                if self.new_best {
                    // Steady rain over the escalated celebration.
                    self.pump_rain(ctx.dt, ctx.frame.w);
                }
                let t = t + ctx.dt;
                if t >= REWARD_DUR {
                    self.grow_and_replay();
                } else {
                    self.phase = Phase::Reward { t };
                }
            }
            // Finale is handled before the topbar above (and returns there).
            Phase::Finale { .. } => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        clear_background(palette::BG);
        let f = &ctx.frame;
        let lay = layout(f);

        // The Finale is its own full-screen celebration scene.
        if let Phase::Finale { t } = self.phase {
            self.draw_finale(ctx, &lay, t);
            return;
        }

        chrome::draw_topbar(&chrome::topbar(f), ctx);

        // The reward star — popped in the empty band ABOVE the choir so it
        // celebrates big without occluding the critters' faces.
        if let Phase::Reward { t } = self.phase {
            let pop = anim::back_out((t / STAR_POP_DUR).clamp(0.0, 1.0)).min(STAR_POP_CAP);
            let r = lay.star_r * pop * if self.new_best { NEW_BEST_SCALE } else { 1.0 };
            // Bottom-anchored: grow UPWARD from the fixed lower edge so the pop
            // never dips onto the faces. A soft glow halo behind the star (same
            // gold, a faint wash so it never tints the choir row).
            let sy = lay.star_bottom - r;
            draw_star_halo(lay.star.x, sy, r, 0.12, 1.5);
        }

        // The count-in ring: a soft circle expanding+fading on each Ready beat,
        // a non-text "it's starting now" pulse centered on the choir.
        if let Phase::Ready { t } = self.phase {
            self.draw_ready_ring(&lay, t);
        }

        // The choir: four pads, each a glow ring + the critter in its pad color.
        for i in 0..4 {
            self.draw_pad(&lay, i);
        }

        // Progress pips + the replay affordance (Input only). No text anywhere —
        // the child can't read; phase/turn read purely from motion + audio.
        if let Phase::Input { got, .. } = self.phase {
            self.draw_pips(&lay, got);
            // A large, clearly-tappable replay button — a primary affordance for
            // a 4yo who forgot the tune. Shown ONLY on the kid's turn.
            draw::circle_btn(lay.replay.x, lay.replay.y, lay.btn_r, palette::CARD);
            draw::replay_icon(lay.replay.x, lay.replay.y, lay.btn_r * 0.84, palette::MUTED);
        }

        self.confetti.draw();
    }
}

impl SingbackScene {
    /// Draw pad `i`: a glow halo behind a colored critter, lit/dim by phase.
    fn draw_pad(&self, lay: &SLayout, i: usize) {
        let c = lay.pads[i];
        let color = palette::RAINBOW[PAD_COLOR_IDX[i]];
        let r = lay.pad * 0.42;

        // Is this pad lit right now (flashing in Show/Input), and how strongly?
        let lit = self.flash_pad == Some(i);
        let flash = if lit { (1.0 - self.flash_t / self.tuning.on).clamp(0.0, 1.0) } else { 0.0 };

        // The correct pad teaches during a Miss with a head-shake wiggle.
        let teaching = matches!(self.phase, Phase::Miss { got, .. }
            if self.sequence.get(got).copied() == Some(i as u8));

        // Pose: pop + sing when flashing; wiggle when teaching; staggered hop in
        // Reward; a synchronized count-in pulse in Ready; a ONE-SHOT welcome
        // bounce the instant Input opens — then everything sits perfectly still
        // (no ambient breathing/blink during the memory task: only the lit pad
        // in Show, or the just-tapped pad in Input, ever moves).
        let mut pose = draw::CritterPose::default();
        let mut glow = 0.0_f32;
        match self.phase {
            Phase::Reward { t } => {
                // All four hop, staggered by index.
                let ph = (t * 3.0 - i as f32 * 0.18).max(0.0);
                let hop = (ph * std::f32::consts::PI).sin().max(0.0) * (1.0 - (t / REWARD_DUR)).max(0.0);
                pose.dy = -hop * r * 0.5;
                pose.sing = hop;
                pose.sy = 1.0 + hop * 0.12;
                pose.sx = 1.0 - hop * 0.06;
            }
            Phase::Ready { t } => {
                // Synchronized soft pulse on each count-in beat (all four in
                // unison — "we're about to start together"), on the fixed brisk
                // tick spacing. Pulses only while the ticks are still firing.
                let phase_t = (t % READY_TICK_S) / READY_TICK_S; // 0..1 in the beat
                let env = (1.0 - phase_t).powi(2); // sharp attack, soft decay
                let active = t < READY_TICKS as f32 * READY_TICK_S;
                let p = if active { env } else { 0.0 };
                pose.sy = 1.0 + 0.10 * p;
                pose.sx = 1.0 - 0.05 * p;
                glow = 0.16 * p;
            }
            Phase::Input { t, .. } if !lit && t < TURN_BOUNCE_DUR => {
                // One-shot welcome bounce: all four hop once, in unison, then
                // freeze. A single decaying half-sine — no looping.
                let env = (t / TURN_BOUNCE_DUR * std::f32::consts::PI).sin().max(0.0);
                pose.dy = -env * r * 0.16;
                pose.sy = 1.0 + 0.10 * env;
                pose.sx = 1.0 - 0.05 * env;
                glow = 0.14 * env;
            }
            _ => {}
        }
        if teaching {
            // Head-shake: a quick rotation oscillation that decays.
            if let Phase::Miss { t, .. } = self.phase {
                let decay = (1.0 - t / MISS_DUR).max(0.0);
                pose.rot = (t * 16.0).sin() * 0.18 * decay;
                glow = 0.5 * decay;
            }
        }
        if lit {
            // Pop + sing + a bright glow at the beat.
            pose.sing = flash;
            pose.dy = -flash * r * 0.18;
            pose.sy = 1.0 + flash * 0.14;
            pose.sx = 1.0 - flash * 0.07;
            glow = (glow).max(0.35 + 0.55 * flash);
        }

        // Glow ring behind the pad (a larger tinted disc), brighter when lit.
        if glow > 0.001 {
            let halo = Color::new(color.r, color.g, color.b, (glow * 0.6).min(0.6));
            draw::disc(c.x, c.y + pose.dy, r * (1.35 + 0.25 * glow), halo);
        }

        // During playback, make every UNLIT critter clearly RECEDE while still
        // reading as ITS rainbow color (the old 0.35 desaturate + 0.7 toward BG
        // washed them to a lifeless grey). A lighter touch — slight desaturate,
        // partway toward the cream stage — keeps the hue alive yet low-contrast,
        // so the single lit critter still clearly dominates.
        let dim = matches!(self.phase, Phase::Show { .. }) && !lit;
        let tint = if dim {
            let grey = (color.r + color.g + color.b) / 3.0;
            let dr = anim::lerp(anim::lerp(color.r, grey, 0.20), palette::BG.r, 0.55);
            let dg = anim::lerp(anim::lerp(color.g, grey, 0.20), palette::BG.g, 0.55);
            let db = anim::lerp(anim::lerp(color.b, grey, 0.20), palette::BG.b, 0.55);
            Color::new(dr, dg, db, 1.0)
        } else {
            color
        };
        draw::critter(CRITTERS[i], c.x, c.y, r, tint, &pose);
    }

    /// Progress pips: one per sequence step, filled up to `got`.
    fn draw_pips(&self, lay: &SLayout, got: usize) {
        let n = self.len();
        if n == 0 {
            return;
        }
        let gap = lay.pip_r * 2.8;
        let total = (n as f32 - 1.0) * gap;
        let x0 = lay.pip_c.x - total / 2.0;
        for i in 0..n {
            let on = i < got;
            let col = if on { palette::RAINBOW[3] } else { palette::PIP_EMPTY };
            draw::disc(x0 + i as f32 * gap, lay.pip_c.y, lay.pip_r, col);
        }
    }

    /// The count-in ring: on each Ready beat a soft circle expands from the
    /// choir center and fades — a non-text "ready… ready… NOW" metronome that
    /// matches the audio tick + the synchronized critter pulse.
    fn draw_ready_ring(&self, lay: &SLayout, t: f32) {
        if t >= READY_TICKS as f32 * READY_TICK_S {
            return; // the still beat before the downbeat: no ring
        }
        let phase_t = (t % READY_TICK_S) / READY_TICK_S; // 0..1 within the beat
        let cx = lay.star.x;
        let cy = (lay.pads[0].y + lay.pads.last().unwrap().y) / 2.0;
        let base = lay.pad * 0.6;
        let radius = base + phase_t * base * 1.4;
        let alpha = (1.0 - phase_t) * 0.45;
        let col = Color { a: alpha, ..palette::RAINBOW[3] };
        draw::arc(cx, cy, radius, 0.0, std::f32::consts::TAU, lay.pad * 0.06, col);
    }

    /// The Finale: a full-screen celebration — confetti rain, a big trophy/star
    /// pop, all four critters dancing on a loop, with corner replay + home.
    fn draw_finale(&self, ctx: &Ctx, lay: &SLayout, t: f32) {
        let f = &ctx.frame;

        // A big gold star/trophy pop high in the frame, with a soft halo and a
        // gentle ongoing throb so it never goes static.
        let pop = anim::back_out((t / STAR_POP_DUR).clamp(0.0, 1.0)).min(STAR_POP_CAP);
        let throb = 1.0 + FINALE_THROB_AMP * anim::pulse(t, 0.9).max(0.0);
        let r = lay.star_r * FINALE_STAR_SCALE * pop * throb;
        // Bottom-anchored: grow UPWARD from the fixed lower edge so even the big
        // finale pop never drops onto the dancers' faces.
        let sy = lay.star_bottom - r;
        draw_star_halo(lay.star.x, sy, r, 0.14, 1.6);

        // The dancing choir: all four hop on a continuous staggered loop.
        for (i, c) in lay.pads.iter().enumerate() {
            let rr = lay.pad * 0.42;
            let color = palette::RAINBOW[PAD_COLOR_IDX[i]];
            let ph = t * 5.0 - i as f32 * 0.5;
            let hop = ph.sin().max(0.0);
            let pose = draw::CritterPose {
                dy: -hop * rr * 0.45,
                sing: hop,
                sy: 1.0 + hop * 0.12,
                sx: 1.0 - hop * 0.06,
                rot: (t * 3.0 + i as f32).sin() * 0.06,
                ..Default::default()
            };
            // A warm glow under each dancer.
            let glow = Color::new(color.r, color.g, color.b, 0.30 + 0.25 * hop);
            draw::disc(c.x, c.y + pose.dy, rr * 1.4, glow);
            draw::critter(CRITTERS[i], c.x, c.y, rr, color, &pose);
        }

        self.confetti.draw();

        // Corner buttons (replay + home), placed identically to the other
        // finale scenes so the kid always finds them.
        let (replay, home, br) = chrome::corner_buttons(f);
        chrome::draw_corner_buttons(replay, home, br);
    }
}

// --- layout -----------------------------------------------------------------

/// Choir + chrome geometry, derived (like every layout) from viewport size +
/// safe insets + form factor: a 1×4 row in landscape, a 2×2 grid in portrait.
struct SLayout {
    /// Per-pad slot size (the pad disc radius is `pad * 0.5`).
    pad: f32,
    /// The four pad centers, indexed 0..4 (1×4 row in landscape, 2×2 portrait).
    /// Computed ONCE here so `draw_pad`/hit-testing reuse them (no re-layout).
    pads: [Vec2; 4],
    /// Reward/finale star RESTING center (pop = 1) + base radius. The star is
    /// bottom-anchored: its lower edge is pinned at `star_bottom` and the pop
    /// grows it UPWARD, so the bottom never dips onto the critter faces.
    star: Vec2,
    star_r: f32,
    /// The fixed lower edge of the star (just above the critter heads). The
    /// Reward/Finale draws place the star at `y = star_bottom - r` so the pop
    /// grows upward from here and the bottom stays clear of faces at any size.
    star_bottom: f32,
    /// Progress-pip strip center + per-pip radius.
    pip_c: Vec2,
    pip_r: f32,
    /// Replay button (Input phase) center + radius (a big primary affordance).
    replay: Vec2,
    btn_r: f32,
}

fn layout(f: &crate::layout::Frame) -> SLayout {
    let tb = f.topbar();
    let content = f.content();
    let region_top = tb.y + tb.h;
    let region_bot = content.y + content.h;

    // The four pads sit in a band centered in the play region below the prompt.
    // Pad slot scales with the viewport but clamps to a big tap target.
    let pad = if f.is_portrait() {
        (f.w * 0.40).clamp(120.0, 280.0)
    } else {
        (f.w * 0.20).clamp(120.0, 240.0)
    };

    // The four pad centers (1×4 row in landscape, 2×2 grid in portrait), biased
    // a touch below the prompt. On short landscape viewports nudge the choir
    // band down a little so the prompt + pip strip clear the critter heads.
    let cx = f.w / 2.0;
    let short_land = !f.is_portrait() && f.h < 480.0;
    let cy_bias = if short_land { 0.12 } else { 0.04 };
    let cy = (region_top + region_bot) / 2.0 + (region_bot - region_top) * cy_bias;
    let s = pad;
    let mut pads = [vec2(cx, cy); 4];
    if f.is_portrait() {
        // 2×2 grid.
        let gap = s * 0.22;
        let half = (s + gap) / 2.0;
        let offs = [(-half, -half), (half, -half), (-half, half), (half, half)];
        for (i, &(dx, dy)) in offs.iter().enumerate() {
            pads[i] = vec2(cx + dx, cy + dy);
        }
    } else {
        // 1×4 row.
        let gap = (s * 0.22).max(12.0);
        let total = 4.0 * s + 3.0 * gap;
        let x0 = cx - total / 2.0 + s / 2.0;
        for (i, p) in pads.iter_mut().enumerate() {
            *p = vec2(x0 + i as f32 * (s + gap), cy);
        }
    }

    // Pip strip: a stable strip near the top of the play region (where the
    // now-removed prompt used to sit), tucked a touch higher on short landscape
    // so it clears the lowered critter heads. Derived directly (the old text
    // band scaffolding collapsed into this one y).
    let pip_r = (f.vmin(0.012)).clamp(6.0, 12.0);
    let region_h = region_bot - region_top;
    let pip_inset = (f.vmin(0.06)).clamp(22.0, 44.0) * if short_land { 0.7 } else { 0.9 };
    let pip_c = vec2(f.w / 2.0, region_top + region_h * 0.10 + pip_inset);
    // A big replay button: it's a primary affordance for a 4yo who lost the tune.
    // ~1.5× the standard icon button, clamped so it never crowds the choir or
    // runs off a short viewport.
    let btn_r = (f.icon_btn() / 2.0 * 1.5).min(region_h).max(36.0);
    let replay = vec2(f.w / 2.0, region_bot - btn_r - f.safe.bottom.max(6.0));

    // The reward/finale star lives in the empty band ABOVE the choir row
    // (between the pip strip and the top critter heads) so it celebrates WITHOUT
    // ever occluding faces — the top heads sit ~pad/2 above each pad center.
    //
    // The star is drawn at `star_r * pop * scale`, peaking at
    // `star_r * STAR_PEAK_SCALE` (the finale's springy throb is the worst case).
    // Rather than shrink the RESTING star so its PEAK fits (which left the star
    // tiny on short bands — phone landscape, iPad portrait), the star is
    // BOTTOM-ANCHORED: its lower edge is pinned at `star_bottom` (just above the
    // heads) and the pop grows it UPWARD. So a resting star fills the band big,
    // and at ANY pop magnitude its bottom never dips onto a face. `star_r` is
    // then sized so even the PEAK upward growth (`star_r * STAR_PEAK_SCALE` tall)
    // clears the pips above — the band above the anchor bounds the peak radius.
    let heads_top = pads[0].y - pad * 0.5;
    // Pin the star's bottom just above the heads; the pop grows it upward only.
    let star_bottom = heads_top - pad * 0.06; // a small margin above the heads
    // The headroom above the anchor, up to the top of the play region (the pips
    // don't draw during Reward/Finale, so the star may rise through that strip,
    // but it must NOT overrun the topbar). The PEAK star (diameter
    // `2 * star_r * STAR_PEAK_SCALE`) must fit that headroom; that bound, the
    // preferred `pad`-relative size, and a floor pick the resting radius.
    let headroom = (star_bottom - region_top).max(0.0);
    let star_r = (pad * 0.5)
        .min(headroom / (2.0 * STAR_PEAK_SCALE))
        .max(pad * 0.12);
    // Resting center: bottom-anchored at rest (pop = 1). Confetti bursts from
    // here; the Reward/Finale draws recompute the center from `star_bottom` as
    // the radius grows, so the bottom edge stays pinned above the heads.
    let star = vec2(f.w / 2.0, star_bottom - star_r);

    SLayout { pad, pads, star, star_r, star_bottom, pip_c, pip_r, replay, btn_r }
}

/// Center of pad `i` (PUBLIC indirection lives on the scene; this is the math).
fn pad_center(f: &crate::layout::Frame, i: usize) -> Vec2 {
    layout(f).pads[i.min(3)]
}

/// The reward/finale gold star with its soft halo: a faint gold disc behind a
/// solid gold star of radius `r`. Shared by Reward + Finale so the two never
/// drift. `halo_alpha`/`halo_scale` tune the wash (Finale runs a touch bigger).
fn draw_star_halo(x: f32, y: f32, r: f32, halo_alpha: f32, halo_scale: f32) {
    let halo = Color { a: halo_alpha, ..palette::GOLD };
    draw::disc(x, y, r * halo_scale, halo);
    draw::star(x, y, r, palette::GOLD);
}

// --- capture-state construction (goldens) -----------------------------------

/// The phase a golden wants the scene pinned into. Mirrors patterns' approach of
/// constructing the scene directly into a representative single frame.
pub(crate) enum CaptureState {
    /// The count-in mid-tick (all four critters pulse + the central ring
    /// expands) — the "ready-set-go" lead-in, made reviewable.
    Ready,
    /// Show playback mid-flash (a pad lit + singing; the rest calm).
    Show,
    /// The kid's turn, one pip filled, the big replay button visible.
    Input,
    /// A miss, the correct pad mid head-shake.
    Miss,
    /// Reward mid-celebration on a NEW best (star + confetti + hops).
    Reward,
    /// The closing Finale mid-celebration (dancing choir + trophy + confetti +
    /// corner buttons).
    Finale,
}

impl SingbackScene {
    /// Build the scene pinned into `cap` for a golden capture, at a fixed seed so
    /// the sequence/colors are deterministic. Drives the chosen phase a few
    /// frames so the single captured frame looks representative (mid-animation).
    pub(crate) fn capture(db: Db, seed: u32, now: i64, cap: CaptureState, ctx0: &Ctx) -> SingbackScene {
        let mut sc = SingbackScene::new(db, seed, now);
        // A fixed 3-pad sequence for all captures (warm→cool legibility).
        sc.sequence = vec![0, 2, 3];
        match cap {
            CaptureState::Ready => {
                // Mid count-in, just after the 2nd brisk tick: all four critters
                // pulse in unison + the central ring is expanding (phase_t small,
                // so the ring reads bright). A representative "ready-set-go" frame.
                sc.phase = Phase::Ready { t: READY_TICK_S + 0.06 };
                sc.clear_flash();
            }
            CaptureState::Show => {
                // Flash the 2nd pad partway through its on-beat.
                sc.phase = Phase::Show { idx: 1, t: 0.18 };
                sc.flash_pad = Some(sc.sequence[1] as usize);
                sc.flash_t = 0.18;
            }
            CaptureState::Input => {
                // The kid's turn, one pip filled, mid welcome-bounce so the choir
                // shows life + the big replay button is up.
                sc.phase = Phase::Input { got: 1, t: 0.22 };
                sc.clear_flash();
            }
            CaptureState::Miss => {
                // Failed at step 1 (the 2nd pad) — it head-shakes mid-wiggle.
                sc.phase = Phase::Miss { got: 1, t: 0.25 };
                sc.clear_flash();
            }
            CaptureState::Reward => {
                // A completed round on a FRESH best: best_span starts at 0, so the
                // span-3 round enter_reward records is strictly higher → new_best
                // is true off the seed (no need to force the escalation look).
                sc.enter_reward(ctx0);
                drive(&mut sc, ctx0, 18);
            }
            CaptureState::Finale => {
                // A full FINALE_SPAN sequence completed → the big closing dance.
                sc.sequence = (0..FINALE_SPAN).map(|i| (i % 4) as u8).collect();
                sc.enter_finale(ctx0);
                // Drive ~0.4s in so the dancers are mid-hop + confetti is falling.
                drive(&mut sc, ctx0, 14);
            }
        }
        sc
    }
}

/// Step `sc` `n` frames at a fixed dt with no input — used by captures to land a
/// representative mid-animation frame.
fn drive(sc: &mut SingbackScene, ctx0: &Ctx, n: usize) {
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
