//! Sing Back: a Simon-says memory game. Four pads form a choir — each pad binds
//! a pitch ↔ a rainbow color ↔ a critter (frog/duck/cat/owl), low→high in both
//! pitch and color so the row reads as a warm-to-cool scale. The game plays a
//! growing sequence (Watch), the kid taps it back (Input), and every completed
//! round celebrates (Reward) before appending one fresh pad and replaying the
//! whole prefix + the new note.
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
    text,
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
/// during the Watch playback, and the starting sequence length.
struct Tuning {
    on: f32,
    gap: f32,
    start_len: usize,
}

fn tuning(difficulty: &str) -> Tuning {
    match difficulty {
        "gentle" => Tuning { on: 0.70, gap: 0.28, start_len: 2 },
        "speedy" => Tuning { on: 0.40, gap: 0.16, start_len: 3 },
        _ => Tuning { on: 0.52, gap: 0.22, start_len: 2 }, // normal
    }
}

/// How long the Miss teaching beat (the correct pad's wiggle) holds before the
/// same sequence replays.
const MISS_DUR: f32 = 1.3;
/// How long the Reward celebration holds before the next round appends + plays.
const REWARD_DUR: f32 = 1.6;
/// `flash_t` value parked far past any on-beat to mean "nothing is lit".
const FLASH_IDLE: f32 = 99.0;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    /// Playing the sequence back to the kid: `idx` = which sequence step is
    /// lighting now, `t` = seconds into that step (on-beat then gap).
    Show { idx: usize, t: f32 },
    /// The kid's turn: `got` correct taps so far. A tap lights its pad + plays
    /// its tone; a right tap advances `got`, a wrong one → Miss.
    Input { got: usize },
    /// A miss: the correct pad (`sequence[got]`) wiggles to teach, `t` seconds
    /// in; at MISS_DUR the same sequence replays (never shortens). `got` is the
    /// step the kid failed (carried from the `Input` it was entered from).
    Miss { got: usize, t: f32 },
    /// A completed round: star pop + confetti + critters hop, `t` seconds in.
    /// At REWARD_DUR a fresh pad appends and Show restarts.
    Reward { t: f32 },
}

pub struct SingbackScene {
    db: Db,
    rng: Mulberry32,
    state: sb::SingBackState,
    tuning: Tuning,
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
    confetti: crate::confetti::Confetti,
    sync: crate::net::SyncClient,
}

impl SingbackScene {
    pub fn new(db: Db, seed: u32, now: i64) -> SingbackScene {
        let difficulty = {
            let kv = db.borrow_kv();
            load_singback(&**kv).difficulty
        };
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
        SingbackScene {
            db,
            rng,
            state,
            tuning,
            sequence,
            streak: 0,
            phase: Phase::Show { idx: 0, t: 0.0 },
            flash_t: FLASH_IDLE,
            flash_pad: None,
            new_best: false,
            rain_acc: 0.0,
            // Separate confetti stream, salted distinctly from `seed` so it is
            // genuinely independent (and never perturbs the sequence RNG).
            confetti: crate::confetti::Confetti::new(seed.wrapping_add(0x9E37_79B9)),
            sync,
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

    /// A correct tap of pad `p` during the kid's turn.
    fn on_tap(&mut self, p: usize, ctx: &Ctx) {
        let Phase::Input { got } = self.phase else { return };
        self.flash(p, ctx);
        if self.sequence.get(got).copied() == Some(p as u8) {
            let got = got + 1;
            if got >= self.len() {
                self.enter_reward(ctx);
            } else {
                self.phase = Phase::Input { got };
            }
        } else {
            // Non-punitive miss: a soft cue + the correct pad teaches by wiggle.
            ctx.audio.incorrect();
            self.phase = Phase::Miss { got, t: 0.0 };
        }
    }

    /// Replay the current sequence from the top (the kid forgot, or a miss).
    fn replay(&mut self) {
        self.phase = Phase::Show { idx: 0, t: 0.0 };
        self.clear_flash();
    }

    /// A round was completed: celebrate, record the span (maybe a new best),
    /// then (after the beat) append a fresh pad and replay the longer sequence.
    fn enter_reward(&mut self, ctx: &Ctx) {
        self.streak += 1;
        let span = self.len() as u32;
        let was_best = self.state.best_span;
        sb::record_span(&mut self.state, span, ctx.now);
        self.new_best = self.state.best_span > was_best;
        self.phase = Phase::Reward { t: 0.0 };
        self.flash_pad = None;
        let f = &ctx.frame;
        let lay = layout(f);
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

    /// Append a fresh random pad and replay the now-longer sequence.
    fn grow_and_replay(&mut self) {
        self.sequence.push(self.rng.below(4) as u8);
        self.new_best = false;
        self.replay();
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
    pub(crate) fn in_input(&self) -> bool {
        matches!(self.phase, Phase::Input { .. })
    }
    pub(crate) fn in_reward(&self) -> bool {
        matches!(self.phase, Phase::Reward { .. })
    }
    pub(crate) fn in_miss(&self) -> bool {
        matches!(self.phase, Phase::Miss { .. })
    }
    pub(crate) fn got(&self) -> usize {
        match self.phase {
            Phase::Input { got } => got,
            _ => 0,
        }
    }
    /// Force the kid's turn now (skip the watch playback) — playtest convenience.
    pub(crate) fn skip_to_input(&mut self) {
        self.phase = Phase::Input { got: 0 };
        self.clear_flash();
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
                        self.phase = Phase::Input { got: 0 };
                        self.clear_flash();
                    } else {
                        self.phase = Phase::Show { idx, t: 0.0 };
                    }
                } else {
                    self.phase = Phase::Show { idx, t };
                }
            }
            Phase::Input { .. } => {
                let pt = ctx.pointer;
                if pt.tapped() {
                    let lay = layout(&ctx.frame);
                    if input::hit_circle(pt.pos, lay.replay.x, lay.replay.y, lay.btn_r) {
                        self.replay();
                    } else {
                        for (i, c) in lay.pads.iter().enumerate() {
                            if input::hit_circle(pt.pos, c.x, c.y, lay.pad * 0.5) {
                                self.on_tap(i, ctx);
                                break;
                            }
                        }
                    }
                }
            }
            Phase::Miss { got, t } => {
                let t = t + ctx.dt;
                if t >= MISS_DUR {
                    self.replay();
                } else {
                    self.phase = Phase::Miss { got, t };
                }
            }
            Phase::Reward { t } => {
                if self.new_best {
                    // Steady rain over the escalated celebration.
                    self.rain_acc += ctx.dt;
                    while self.rain_acc > 0.10 {
                        self.confetti.rain(ctx.frame.w, -10.0, 1);
                        self.rain_acc -= 0.10;
                    }
                }
                let t = t + ctx.dt;
                if t >= REWARD_DUR {
                    self.grow_and_replay();
                } else {
                    self.phase = Phase::Reward { t };
                }
            }
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        clear_background(palette::BG);
        let f = &ctx.frame;
        let lay = layout(f);
        chrome::draw_topbar(&chrome::topbar(f), ctx);

        // The reward star — popped in the empty band ABOVE the choir so it
        // celebrates big without occluding the critters' faces.
        if let Phase::Reward { t } = self.phase {
            let pop = anim::back_out((t / 0.45).clamp(0.0, 1.0)).min(1.25);
            let r = lay.star_r * pop * if self.new_best { 1.5 } else { 1.0 };
            // A soft glow halo behind the star (same gold as the star, just a
            // faint wash so it never tints the choir row).
            let halo = Color { a: 0.12, ..palette::GOLD };
            draw::disc(lay.star.x, lay.star.y, r * 1.5, halo);
            draw::star(lay.star.x, lay.star.y, r, palette::GOLD);
        }

        // The choir: four pads, each a glow ring + the critter in its pad color.
        for i in 0..4 {
            self.draw_pad(ctx, &lay, i);
        }

        // Prompt + progress pips (Input only).
        match self.phase {
            Phase::Show { .. } => {
                text::ui_centered("Watch!", f.w / 2.0, lay.prompt_y, lay.prompt_px, palette::MUTED);
            }
            Phase::Input { got } => {
                text::ui_centered("Your turn!", f.w / 2.0, lay.prompt_y, lay.prompt_px, palette::MUTED);
                self.draw_pips(&lay, got);
                // Replay button — errorless support for forgetting the sequence.
                draw::circle_btn(lay.replay.x, lay.replay.y, lay.btn_r, palette::CARD);
                draw::replay_icon(lay.replay.x, lay.replay.y, lay.btn_r * 0.9, palette::MUTED);
            }
            _ => {}
        }

        self.confetti.draw();
    }
}

impl SingbackScene {
    /// Draw pad `i`: a glow halo behind a colored critter, lit/dim by phase.
    fn draw_pad(&self, ctx: &Ctx, lay: &SLayout, i: usize) {
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
        // Reward; a gentle "tap me" breathe during the kid's Input turn.
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
            Phase::Input { .. } if !lit => {
                // Calm "your turn" breathing on all pads.
                let b = anim::pulse(ctx.time + i as f32 * 0.3, 1.8).max(0.0);
                pose.sy = 1.0 + 0.03 * b;
                pose.sx = 1.0 - 0.02 * b;
                glow = 0.12 * b;
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

        // During playback, make every UNLIT critter clearly RECEDE: composite
        // each channel toward the cream BG (and a touch toward grey to
        // desaturate) so all three muted ones land at a similar low-contrast
        // value and the single lit critter pops. A flat alpha left the warm
        // ones vivid; blending toward BG fixes that.
        let dim = matches!(self.phase, Phase::Show { .. }) && !lit;
        let tint = if dim {
            let grey = (color.r + color.g + color.b) / 3.0;
            // First desaturate slightly, then recede toward the cream stage.
            let dr = anim::lerp(anim::lerp(color.r, grey, 0.35), palette::BG.r, 0.7);
            let dg = anim::lerp(anim::lerp(color.g, grey, 0.35), palette::BG.g, 0.7);
            let db = anim::lerp(anim::lerp(color.b, grey, 0.35), palette::BG.b, 0.7);
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
    /// Reward star center + base radius.
    star: Vec2,
    star_r: f32,
    /// Prompt baseline + font px.
    prompt_y: f32,
    prompt_px: u16,
    /// Progress-pip strip center + per-pip radius.
    pip_c: Vec2,
    pip_r: f32,
    /// Replay button (Input phase) center + radius.
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

    let prompt_px = (f.vmin(0.06)).clamp(22.0, 44.0) as u16;
    let prompt_y = region_top + (region_bot - region_top) * 0.10;

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

    // Pip strip just under the prompt; replay button bottom-center.
    let pip_r = (f.vmin(0.012)).clamp(6.0, 12.0);
    // On short landscape, tuck the pips up close under the prompt so they never
    // crowd the (lowered) critter heads.
    let pip_y = if short_land {
        prompt_y + prompt_px as f32 * 0.7
    } else {
        prompt_y + prompt_px as f32 * 0.9
    };
    let pip_c = vec2(f.w / 2.0, pip_y);
    let btn_r = f.icon_btn() / 2.0;
    let replay = vec2(f.w / 2.0, region_bot - btn_r - f.safe.bottom.max(6.0));

    // The star pops in the empty band ABOVE the choir row (between the pip strip
    // and the top critter heads) so it celebrates without occluding faces — the
    // top heads sit ~pad/2 above each pad center.
    let star_r = pad * 0.45;
    let heads_top = pads[0].y - pad * 0.5;
    let star_y = ((pip_c.y + heads_top) / 2.0).min(heads_top - star_r * 0.6).max(pip_c.y + star_r * 0.4);
    let star = vec2(f.w / 2.0, star_y);

    SLayout { pad, pads, star, star_r, prompt_y, prompt_px, pip_c, pip_r, replay, btn_r }
}

/// Center of pad `i` (PUBLIC indirection lives on the scene; this is the math).
fn pad_center(f: &crate::layout::Frame, i: usize) -> Vec2 {
    layout(f).pads[i.min(3)]
}

// --- capture-state construction (goldens) -----------------------------------

/// The phase a golden wants the scene pinned into. Mirrors patterns' approach of
/// constructing the scene directly into a representative single frame.
pub(crate) enum CaptureState {
    /// Show playback mid-flash (a pad lit + singing).
    Show,
    /// The kid's turn, one pip filled, pads breathing, replay visible.
    Input,
    /// A miss, the correct pad mid head-shake.
    Miss,
    /// Reward mid-celebration on a NEW best (star + confetti + hops).
    Reward,
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
            CaptureState::Show => {
                // Flash the 2nd pad partway through its on-beat.
                sc.phase = Phase::Show { idx: 1, t: 0.18 };
                sc.flash_pad = Some(sc.sequence[1] as usize);
                sc.flash_t = 0.18;
            }
            CaptureState::Input => {
                sc.phase = Phase::Input { got: 1 };
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
                let idle = crate::input::Pointer::default();
                for _ in 0..18 {
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
        }
        sc
    }
}
