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
/// get-ready cue runs at its OWN fixed duration (`READY_TOTAL_S`), independent of
/// this difficulty timing, so a slow difficulty can't drag the lead-in.
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

/// The Ready get-ready cue: a purely ENVIRONMENTAL "dim-and-bloom" fade over the
/// still choir — NO character motion. A fullscreen overlay dims the stage (lights
/// down → "settle"), holds briefly, then blooms back to full reveal (lights up →
/// "now"), resolving to alpha 0 EXACTLY at `READY_TOTAL_S` so the first critter
/// sings the instant the scene is fully revealed. Replaces the old metronome
/// count-in (which jumped all four critters in unison and mis-read as part of the
/// sequence). The critters stay perfectly still throughout.
///
/// Curve, in fractions of `READY_TOTAL_S`: alpha eases 0 → `READY_DIM_PEAK` over
/// `[0, READY_DIM_IN]` (dim-in), holds across `[READY_DIM_IN, READY_HOLD_END]`,
/// then eases back to 0 over `[READY_HOLD_END, 1]` (bloom-out / reveal).
///
/// Lengthened from 1.15 — the old fade resolved a touch too quickly to read as a
/// deliberate "settle → now" (err on the slow side for the reveal, per the
/// pedagogy notes).
const READY_TOTAL_S: f32 = 1.55;
/// Peak overlay alpha at the dim — "lights down to settle". Tuned by eye (A/B'd
/// against 0.5): 0.65 + a deep cool indigo tint reads as a deliberate theatre
/// "lights down → settle", where 0.5 navy-over-cream read as a flat beige-grey
/// "disabled" wash. High enough to dim clearly without fully hiding the choir.
const READY_DIM_PEAK: f32 = 0.65;
/// End of the dim-in ramp (fraction of `READY_TOTAL_S`).
const READY_DIM_IN: f32 = 0.42;
/// End of the dim hold, where the bloom-out (reveal) ramp begins (fraction of
/// `READY_TOTAL_S`).
const READY_HOLD_END: f32 = 0.58;
/// Overlay color: a deep, warm theatre plum-violet. Chosen over a cream/white
/// bloom (white-on-cream is near-invisible), over the old near-INK navy 0x2b2c34
/// (which flattened to a muddy beige-grey "disabled" wash), and over the prior
/// cool indigo 0x1e1b3a (which half-opaque over cream read a little cold + dull).
/// A deep plum-violet ties the get-ready dim to the finale's party violet and
/// reads as a richer, warmer "lights down → settle".
const READY_OVERLAY: u32 = 0x2a1b47;
/// The one-time scene-entry lead-in (FIRST play only — retries go straight to the
/// `Ready` cue): the choir eases in from the dim `READY_OVERLAY` wash over
/// `INTRO_FADE_S` (a soft non-abrupt entrance), then holds fully revealed + still
/// for `INTRO_HOLD_S` so the child can ORIENT in the scene before the get-ready
/// cue and the first note. Calm throughout — no character motion (the choir is a
/// neutral still pose), matching the "freeze ambient motion / lead into the
/// sequence" rules. After it, the normal `Ready` dim-and-bloom + `Show` run.
const INTRO_FADE_S: f32 = 0.65;
const INTRO_HOLD_S: f32 = 0.7;
const INTRO_TOTAL_S: f32 = INTRO_FADE_S + INTRO_HOLD_S;
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
/// Finale dance-party tap targets: one per dancing critter (0..4 shifted past
/// the pad ids), the frog DJ, and the balloons. Distinct so a fast tap on a
/// neighbour always lands (the per-target debounce only swallows a same-target
/// re-fire).
const TGT_DANCER_BASE: u32 = 30; // dancer i → TGT_DANCER_BASE + i
const TGT_FINALE_FROG: u32 = 40;
const TGT_BALLOON_BASE: u32 = 50; // balloon i → TGT_BALLOON_BASE + i

// --- finale dance party -----------------------------------------------------

/// The looping happy TUNE the choir sings during the Finale: pentatonic steps
/// restricted to 0..=3 so every note maps to a visible critter (frog/duck/cat/
/// owl, low→high). A short cheerful motif (8 beats) then a rest, on a loop, so
/// the party has a voice without being relentless. Driven off the finale clock:
/// each time the melody clock crosses a beat we play `memory_tone(step)` and
/// make that critter sing+dance.
const MELODY: [u8; 8] = [0, 2, 1, 3, 2, 3, 1, 0];
/// Seconds per melody beat (the on-beat cadence of the tune).
const BEAT_S: f32 = 0.36;
/// A trailing rest after the 8-beat motif before it loops, so the tune breathes.
const MELODY_REST_S: f32 = 0.8;
/// Total melody loop period: the 8 beats + the trailing rest.
const MELODY_LOOP_S: f32 = MELODY.len() as f32 * BEAT_S + MELODY_REST_S;
/// How long a critter's auto-sing (driven by the melody) lights it for — a touch
/// under one beat so each note reads as a distinct pop.
const DANCER_SING_S: f32 = 0.30;
/// How long a tapped critter's DANCE MOVE animates. A tap is ignored while a
/// move is mid-flight (the per-critter timer < this); `DANCE_IDLE` (99) is idle.
const DANCE_MOVE_S: f32 = 0.6;
/// Parked dance-timer value meaning "this critter is idle" (no move in flight).
const DANCE_IDLE: f32 = 99.0;
/// How long the frog DJ's special flourish (hop + spin + ribbit) runs.
const FROG_FLOURISH_S: f32 = 0.7;
/// The non-escalating set of tapped-dance moves, cycled `mod` its length per
/// critter so a tap is always errorless and never harder. Each is a small,
/// distinct (dy, rot, squash) recipe driven by a half-sine impulse.
const DANCE_MOVES: usize = 4;

/// The party backdrop wash — a warm festive gradient distinct from the plain
/// cream BG, so the finale unmistakably reads as a different (party) screen: a
/// deep grape/violet up top fading to a warm amber glow near the dance floor.
const PARTY_TOP: Color = palette::hex(0x4a2d6b); // deep party violet
const PARTY_BOT: Color = palette::hex(0xffc97a); // warm amber floor glow
/// Festive balloon colours: one RAINBOW index per balloon (spread across the
/// ramp for a bright party spread).
const BALLOON_COLOR_IDX: [usize; FINALE_BALLOONS] = [0, 1, 3, 5, 6];
/// Dance-floor tile colours (two warm party hues, alternated checker-style).
const FLOOR_TILE_A: Color = palette::hexa(0xff8fb3, 0.55); // pink
const FLOOR_TILE_B: Color = palette::hexa(0x8fd0ff, 0.55); // sky-blue
/// How many triangular pennants the bunting swag strings across the top.
const BUNTING_FLAGS: usize = 12;
/// A warm-amber spotlight glow for the DJ's spot. Pulled off the old near-white
/// 0xfff0b8: that pale fill, drawn as one hard-edged translucent disc, read as a
/// bright "splotch" on the floor — and blew out further under a device's
/// gamma-correct blending (vs. the softer software renders). A warmer amber, drawn
/// through `soft_glow` (a radial falloff, lower alpha), reads as a glow not a blob.
const SPOTLIGHT_AMBER: u32 = 0xffd98a;
/// Per-dancer spotlight radius as a multiple of the dancer's body radius (idle).
/// Kept tight so adjacent dancers' halos don't merge into one centre-stage blob;
/// it widens a touch on the beat (see the draw).
const DANCER_GLOW_SCALE: f32 = 0.92;
/// The DJ's spotlight radius as a multiple of `dj_r` — tightened from 1.4 so the
/// host's spot reads discretely from the dancers' (no central washed-out blob).
const DJ_GLOW_SCALE: f32 = 1.1;

// Confetti burst recipes (piece count + horizontal spread), named for the file's
// "every magic number named" style. Spreads are expressed in the local geometry
// of each burst's source (pad / dance-unit / body radius) at the call site.
/// The Reward (round-complete) star burst: a full spray to fill the frame.
const REWARD_BURST_N: usize = 140;
const REWARD_BURST_SPREAD_PAD: f32 = 1.1; // × `lay.pad`
/// The Finale opening burst centred on the trophy.
const FINALE_OPEN_BURST_N: usize = 180;
const FINALE_OPEN_BURST_SPREAD_UNIT: f32 = 3.0; // × `fl.unit`
/// The frog DJ's tap-flourish burst.
const DJ_TAP_BURST_N: usize = 22;
const DJ_TAP_BURST_SPREAD: f32 = 0.6; // × `dj_r`
/// A dancer's tap burst.
const DANCER_TAP_BURST_N: usize = 16;
const DANCER_TAP_BURST_SPREAD: f32 = 0.5; // × the dancer's body radius
/// How long a tapped balloon's swing-and-bob wobble animates. Balloons never pop
/// (errorless + endlessly re-tappable) — a tap just nudges them around.
const BALLOON_BUMP_S: f32 = 0.7;

/// Confetti-rain pump cadence: one piece every `RAIN_INTERVAL_S` of accumulated
/// time, shared by the Reward (new-best) escalation and the Finale.
const RAIN_INTERVAL_S: f32 = 0.10;

/// Salt added to the base `seed` when the constructor seeds the confetti stream,
/// so confetti is independent of the sequence RNG (golden-ratio mix constant).
const CONFETTI_SEED_SALT: u32 = 0x9E37_79B9;
/// A DISTINCT salt for the restart() confetti reseed, so a restarted session's
/// confetti differs from the first session's but is STILL derived from the base
/// seed (never drawn from the sequence RNG, which would couple the two streams).
const CONFETTI_RESTART_SALT: u32 = 0x85EB_CA6B;

/// The reward/finale star-pop curve: `back_out(t / STAR_POP_DUR)` capped at
/// `STAR_POP_CAP` for the springy overshoot that both share.
const STAR_POP_DUR: f32 = 0.45;
const STAR_POP_CAP: f32 = 1.25;
/// The reward (new-best) star is drawn at `star_r * pop * NEW_BEST_SCALE`.
const NEW_BEST_SCALE: f32 = 1.5;
/// The finale throb is `1.0 + FINALE_THROB_AMP * pulse(..).max(0.0)`, peaking at
/// `1.0 + FINALE_THROB_AMP` when the pulse tops out. (Applied only in the finale
/// draw; the finale star is sized by `finale_layout`, not the gameplay `layout`.)
const FINALE_THROB_AMP: f32 = 0.06;
/// The LARGEST multiplier the gameplay reward star's drawn radius can reach over
/// its base `star_r`, at its animated PEAK: the new-best reward draws at
/// `star_r * pop * NEW_BEST_SCALE`, so the peak is `STAR_POP_CAP * NEW_BEST_SCALE`
/// = 1.875. (The FINALE star is sized independently by `finale_layout` and is NOT
/// drawn through the gameplay `layout`, so it does not enter this cap.)
///
/// The star is BOTTOM-ANCHORED (grows upward from a fixed lower edge above the
/// heads), so its bottom never covers a face regardless of pop. `layout()` uses
/// this as the cap that keeps even the PEAK upward growth
/// (`star_r * REWARD_STAR_PEAK_SCALE` tall) inside the headroom above the anchor
/// — so the celebratory pop is big yet never overruns the topbar.
const REWARD_STAR_PEAK_SCALE: f32 = STAR_POP_CAP * NEW_BEST_SCALE;

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    /// The one-time scene-entry lead-in (FIRST play only): `t` = seconds in. The
    /// choir eases in from the dim wash then holds fully revealed + still so the
    /// child can orient before the get-ready cue. At `INTRO_TOTAL_S` the normal
    /// `Ready` cue begins. Never re-entered (a retry/restart goes to `Ready`).
    Intro { t: f32 },
    /// The get-ready cue before Show: `t` = seconds in. A fullscreen overlay
    /// dims the still choir (lights down → settle), holds, then blooms back to a
    /// full reveal (lights up → now), reaching alpha 0 at `READY_TOTAL_S` — when
    /// Show begins so the first critter sings on the reveal. The critters stay
    /// STILL: this cue is purely environmental, never character motion.
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
    /// The scene's base seed (from construction). Confetti is reseeded from this
    /// (salted) on `restart()` so its stream stays independent of the sequence RNG.
    seed: u32,
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

    // --- finale dance-party state (only meaningful in Phase::Finale) ---
    /// Looping melody clock: seconds into the current `MELODY_LOOP_S` cycle. The
    /// previous frame's value too, so a beat that the clock CROSSES this frame
    /// fires exactly once (deterministic off `ctx.dt`).
    melody_t: f32,
    /// The last melody beat index we fired (`-1` until the first beat / after a
    /// loop wrap), so each of the 8 beats triggers its note exactly once a loop.
    melody_beat: i32,
    /// Per-critter dance-move timer: seconds into the current tapped move, or
    /// `DANCE_IDLE` when idle. A tap is ignored while this is < `DANCE_MOVE_S`.
    dance_t: [f32; 4],
    /// Per-critter dance-move kind (cycled `mod DANCE_MOVES` on each tap), and
    /// the auto-sing timer (seconds since the melody last lit this critter, or
    /// `DANCE_IDLE`), which drives the looping-tune sing pose independent of taps.
    dance_kind: [usize; 4],
    sing_t: [f32; 4],
    /// Count of dancer taps accepted this finale (a --playtest reaction hook).
    dancer_taps: u32,
    /// Count of balloon nudges accepted this finale (a --playtest reaction hook;
    /// balloons never pop, so the count can grow without bound on re-taps).
    balloon_bumps: u32,
    /// The frog DJ's flourish timer (seconds in, or `DANCE_IDLE` when idle).
    frog_t: f32,
    /// Per-balloon bump timer: seconds into the current tapped wobble, or
    /// `DANCE_IDLE` when idle. Balloons NEVER pop — a tap just makes them swing +
    /// bob (errorless, infinitely re-tappable). `balloon_kick` is the swing
    /// direction of the current bump (set from which side of the balloon was hit).
    balloon_t: [f32; FINALE_BALLOONS],
    balloon_kick: [f32; FINALE_BALLOONS],
}

/// How many festive balloons bob over the dance floor (tappable → pop).
const FINALE_BALLOONS: usize = 5;

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
            seed,
            rng,
            state,
            tuning,
            gentle,
            sequence,
            streak: 0,
            phase: Phase::Intro { t: 0.0 },
            flash_t: FLASH_IDLE,
            flash_pad: None,
            new_best: false,
            rain_acc: 0.0,
            tap_debounce: input::TapDebounce::new(),
            // Separate confetti stream, salted distinctly from `seed` so it is
            // genuinely independent (and never perturbs the sequence RNG).
            confetti: crate::confetti::Confetti::new(seed.wrapping_add(CONFETTI_SEED_SALT)),
            sync,
            melody_t: 0.0,
            melody_beat: -1,
            dance_t: [DANCE_IDLE; 4],
            dance_kind: [0; 4],
            sing_t: [DANCE_IDLE; 4],
            dancer_taps: 0,
            balloon_bumps: 0,
            frog_t: DANCE_IDLE,
            balloon_t: [DANCE_IDLE; FINALE_BALLOONS],
            balloon_kick: [0.0; FINALE_BALLOONS],
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
        self.confetti.burst(
            vec2(lay.star.x, lay.star.y),
            REWARD_BURST_N,
            lay.pad * REWARD_BURST_SPREAD_PAD,
        );
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
    /// Persists + pushes, then throws a dance party. Reseeds the debounce for the
    /// dancer / frog / balloon / corner taps; resets the melody + dance clocks so
    /// the tune starts from the top and every dancer pops in fresh.
    fn enter_finale(&mut self, ctx: &Ctx) {
        self.phase = Phase::Finale { t: 0.0 };
        self.flash_pad = None;
        self.clear_flash();
        self.tap_debounce = input::TapDebounce::new();
        // Reset the dance-party clocks so a restart-then-replay never inherits a
        // stale beat / mid-move pose.
        self.melody_t = 0.0;
        self.melody_beat = -1;
        self.dance_t = [DANCE_IDLE; 4];
        self.dance_kind = [0; 4];
        self.sing_t = [DANCE_IDLE; 4];
        self.dancer_taps = 0;
        self.balloon_bumps = 0;
        self.frog_t = DANCE_IDLE;
        self.balloon_t = [DANCE_IDLE; FINALE_BALLOONS];
        self.balloon_kick = [0.0; FINALE_BALLOONS];
        self.rain_acc = 0.0;
        ctx.audio.finale();
        self.save();
        self.sync.queue_push(&self.state.serialize_json(), ctx.now);
        // The opening confetti burst centred on the dance floor (its OWN geometry,
        // not the gameplay pads).
        let fl = finale_layout(&ctx.frame);
        self.confetti.burst(fl.trophy, FINALE_OPEN_BURST_N, fl.unit * FINALE_OPEN_BURST_SPREAD_UNIT);
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
        // Fresh confetti stream for the new session, reseeded from the base seed
        // (salted) — NOT from `self.rng`, so the sequence stream is untouched and
        // confetti stays independent of gameplay (mirrors the constructor's salt).
        self.confetti =
            crate::confetti::Confetti::new(self.seed.wrapping_add(CONFETTI_RESTART_SALT));
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
    pub(crate) fn in_intro(&self) -> bool {
        matches!(self.phase, Phase::Intro { .. })
    }
    pub(crate) fn in_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready { .. })
    }
    pub(crate) fn in_show(&self) -> bool {
        matches!(self.phase, Phase::Show { .. })
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
    /// Finale dance-party hooks (used by --playtest to drive + assert taps).
    /// The on-floor center of dancer `i` (its tap target).
    pub(crate) fn finale_dancer_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        finale_layout(f).dancers[i.min(3)].0
    }
    /// How many dancer taps have been accepted this finale (a reaction proof).
    pub(crate) fn dancer_taps(&self) -> u32 {
        self.dancer_taps
    }
    /// The center of balloon `i` (its tap target) for the current frame.
    pub(crate) fn finale_balloon_center(&self, f: &crate::layout::Frame, i: usize) -> Vec2 {
        finale_layout(f).balloons[i.min(FINALE_BALLOONS - 1)].0
    }
    /// How many balloon nudges have been accepted this finale (a reaction proof).
    pub(crate) fn balloon_bumps(&self) -> u32 {
        self.balloon_bumps
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

        // The Finale is a full-screen dance party that draws NO topbar — handle
        // it (its own corner replay/home + the interactive dancers) FIRST and
        // return, so the invisible topbar's top-corner hit targets are never
        // consulted during it (a top-left tap must NOT silently go Home / open
        // parent over the dance).
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
            Phase::Intro { t } => {
                // The one-time scene-entry settle: ease in, then a still hold, then
                // hand off to the normal get-ready cue. No motion, no taps consumed.
                let t = t + ctx.dt;
                if t >= INTRO_TOTAL_S {
                    self.enter_ready();
                } else {
                    self.phase = Phase::Intro { t };
                }
            }
            Phase::Ready { t } => {
                // The dim-and-bloom get-ready cue: the choir stays still while a
                // fullscreen overlay dims (settle) then blooms back (reveal). One
                // subtle twinkle marks the start of the bloom (the reveal), so the
                // cue has a single gentle voice — no rhythmic count. Show begins
                // when the bloom resolves to full reveal, so the first note lands
                // on the "now".
                let prev = t;
                let t = t + ctx.dt;
                let bloom = READY_HOLD_END * READY_TOTAL_S;
                if prev <= bloom && t > bloom {
                    ctx.audio.twinkle();
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

        // The Finale is its own full-screen dance-party scene (its OWN geometry
        // + backdrop — never the gameplay layout).
        if let Phase::Finale { t } = self.phase {
            self.draw_finale(ctx, t);
            return;
        }

        let lay = layout(f);
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

        // The choir: four pads, each a glow ring + the critter in its pad color.
        for i in 0..4 {
            self.draw_pad(&lay, i);
        }

        // The one-time scene-entry settle: the choir eases in from the dim wash,
        // then the overlay holds clear (a still, fully-revealed beat to orient)
        // before the get-ready cue takes over. Same wash as Ready so the entrance
        // and the get-ready dim read as one continuous environment.
        if let Phase::Intro { t } = self.phase {
            let a = intro_overlay_alpha(t);
            if a > 0.001 {
                draw_rectangle(0.0, 0.0, f.w, f.h, palette::hexa(READY_OVERLAY, a));
            }
        }

        // The get-ready cue: a fullscreen dim-and-bloom overlay OVER the still
        // choir. Dims to settle, holds, then blooms back to a full reveal exactly
        // as Show begins — a purely environmental "get ready → now" (no character
        // motion). Drawn over the choir + topbar so the whole stage dims as one.
        if let Phase::Ready { t } = self.phase {
            let a = ready_overlay_alpha(t);
            if a > 0.001 {
                draw_rectangle(0.0, 0.0, f.w, f.h, palette::hexa(READY_OVERLAY, a));
            }
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
        // Reward; a ONE-SHOT welcome bounce the instant Input opens — then
        // everything sits perfectly still (no ambient breathing/blink during the
        // memory task: only the lit pad in Show, or the just-tapped pad in Input,
        // ever moves). Ready has NO pose: the critters stay neutral/still while
        // the environmental dim-and-bloom overlay (drawn in draw()) does the cue,
        // so the get-ready beat never reads as part of the sequence.
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

    /// Drive the Finale dance party for one frame: advance the looping melody
    /// (firing each beat's note + sing once), tick every per-critter dance + the
    /// frog flourish, rain confetti, and handle taps — dancers sing+dance, the
    /// frog DJ flourishes, balloons pop, corners replay/home. Errorless +
    /// non-escalating: every tap reacts, none is ever "wrong". Handled FIRST in
    /// `update()` (before the topbar) so the invisible topbar corners stay dead.
    fn update_finale(&mut self, ctx: &Ctx) -> Nav {
        let Phase::Finale { t } = self.phase else { return Nav::Stay };
        let dt = ctx.dt;

        // A steady, gentle confetti rain over the dance.
        self.pump_rain(dt, ctx.frame.w);

        // Tick the per-critter dance moves + auto-sing + the frog flourish (all
        // dt-driven; `DANCE_IDLE` parks a timer when idle so it never wraps).
        for i in 0..4 {
            if self.dance_t[i] < DANCE_MOVE_S {
                self.dance_t[i] += dt;
            } else {
                self.dance_t[i] = DANCE_IDLE;
            }
            if self.sing_t[i] < DANCER_SING_S {
                self.sing_t[i] += dt;
            } else {
                self.sing_t[i] = DANCE_IDLE;
            }
        }
        if self.frog_t < FROG_FLOURISH_S {
            self.frog_t += dt;
        } else {
            self.frog_t = DANCE_IDLE;
        }
        for i in 0..FINALE_BALLOONS {
            if self.balloon_t[i] < BALLOON_BUMP_S {
                self.balloon_t[i] += dt;
            } else {
                self.balloon_t[i] = DANCE_IDLE;
            }
        }

        // Advance the looping melody clock. Each beat the clock CROSSES fires its
        // note + lights that critter exactly once; the trailing rest gives the
        // tune room to breathe before it loops.
        let prev = self.melody_t;
        let mut mt = self.melody_t + dt;
        if mt >= MELODY_LOOP_S {
            mt -= MELODY_LOOP_S;
            self.melody_beat = -1; // a new loop: re-arm every beat
        }
        self.melody_t = mt;
        // Fire any motif beat whose onset lies in the half-open window [prev, mt)
        // this frame (in-loop only — never during the rest tail). Half-open at the
        // FRONT (`prev <= onset`) so beat 0's onset of 0.0 fires on the very first
        // frame of a loop (prev == 0.0) — a strict `prev < onset` would drop the
        // opening downbeat on loop 1. The `melody_beat < beat` guard below stops a
        // beat re-firing within the loop, so the inclusive front never double-fires.
        // On a wrap, `prev > mt`, so only the post-wrap window is checked (the
        // pre-wrap tail was the rest).
        for (beat, &step_u8) in MELODY.iter().enumerate() {
            let onset = beat as f32 * BEAT_S;
            let crossed = if prev <= mt {
                prev <= onset && onset < mt
            } else {
                onset < mt // after a wrap
            };
            if crossed && self.melody_beat < beat as i32 {
                self.melody_beat = beat as i32;
                let step = step_u8 as usize;
                ctx.audio.memory_tone(step as u32);
                // The auto-sing pose for that critter (independent of taps).
                self.sing_t[step] = 0.0;
            }
        }

        self.phase = Phase::Finale { t: t + dt };

        // Taps: dancers / frog DJ / balloons / corners. Gated per-target through
        // the debounce so one press never double-fires the same thing.
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
            // Frog DJ: a special flourish (ribbit + hop/spin). The frog isn't one
            // of the four dancers — it's the host, on its own spot + timer.
            if input::hit_circle(pt.pos, fl.dj.x, fl.dj.y, fl.dj_r * 1.2)
                && self.frog_t >= FROG_FLOURISH_S
                && self.tap_debounce.accept(TGT_FINALE_FROG, ctx.time)
            {
                self.frog_t = 0.0;
                ctx.audio.frog();
                self.confetti.burst(
                    vec2(fl.dj.x, fl.dj.y - fl.dj_r),
                    DJ_TAP_BURST_N,
                    fl.dj_r * DJ_TAP_BURST_SPREAD,
                );
                return Nav::Stay;
            }
            // A dancer: sing its note + a fresh DANCE MOVE (cycled, non-escalating)
            // + a small burst. Ignored while that critter's move is mid-flight.
            for i in 0..4 {
                let (c, rr) = fl.dancers[i];
                if input::hit_circle(pt.pos, c.x, c.y, rr * 1.15)
                    && self.dance_t[i] >= DANCE_MOVE_S
                    && self.tap_debounce.accept(TGT_DANCER_BASE + i as u32, ctx.time)
                {
                    self.dance_t[i] = 0.0;
                    self.dance_kind[i] = (self.dance_kind[i] + 1) % DANCE_MOVES;
                    self.sing_t[i] = 0.0;
                    self.dancer_taps += 1;
                    ctx.audio.memory_tone(i as u32);
                    self.confetti.burst(
                        vec2(c.x, c.y - rr),
                        DANCER_TAP_BURST_N,
                        rr * DANCER_TAP_BURST_SPREAD,
                    );
                    return Nav::Stay;
                }
            }
            // A balloon: nudge it (swing + bob + twinkle). It NEVER pops — a tap
            // just bumps it so it bobs away from the finger; endlessly re-tappable.
            for i in 0..FINALE_BALLOONS {
                let (bc, brad) = fl.balloons[i];
                if input::hit_circle(pt.pos, bc.x, bc.y, brad * 1.2)
                    && self.tap_debounce.accept(TGT_BALLOON_BASE + i as u32, ctx.time)
                {
                    self.balloon_t[i] = 0.0;
                    // Swing away from where it was hit (right tap → swing left).
                    self.balloon_kick[i] = if pt.pos.x >= bc.x { -1.0 } else { 1.0 };
                    self.balloon_bumps += 1;
                    ctx.audio.twinkle();
                    return Nav::Stay;
                }
            }
        }
        Nav::Stay
    }

    /// The Finale: a full-screen DANCE PARTY — a festive backdrop wash, a glowing
    /// dance floor of tiles, bobbing balloons, the trophy star, the four critters
    /// scattered + dancing across the floor, the frog DJ as host, confetti rain,
    /// and corner replay/home. A DISTINCT arrangement from the gameplay row/grid
    /// (its own [`FinaleLayout`] geometry, never `lay.pads`).
    fn draw_finale(&self, ctx: &Ctx, t: f32) {
        let f = &ctx.frame;
        let fl = finale_layout(f);

        // Festive backdrop wash (distinct from the plain cream BG): a warm party
        // gradient, deep at the top fading to a glow near the floor.
        draw::vgradient(0.0, 0.0, f.w, f.h, PARTY_TOP, PARTY_BOT);

        // Bunting strung high across the top (a row of little triangular flags on
        // a gentle catenary), gently swaying off the finale clock. Reuses the
        // shared train bunting so the festive dressing never drifts between games.
        draw::bunting(0.0, f.w, fl.bunting_y, fl.bunting_drop, BUNTING_FLAGS, t);

        // The dance floor: a glowing ellipse pool under a band of alternating
        // rounded tiles, so the critters clearly stand ON a floor (not a row).
        // A soft warm pool under the floor — kept gentle (was 0.35) so it lifts the
        // tiles without piling onto the amber base into a bright bottom band.
        let floor_glow = palette::hexa(0xffe9a8, 0.22);
        draw::fill_ellipse(f.w / 2.0, fl.floor_y, fl.floor_rx, fl.floor_ry, 0.0, floor_glow);
        draw_dance_floor(&fl, t);

        // The trophy star, bottom-anchored above the dancers, with a tight bright
        // core + a few radiating sparkle points + a gentle throb so it never goes
        // static. Over the dark party backdrop the old stacked faint-gold discs
        // browned out to a dull tan ring, so the halo is now a single tight bright
        // core plus radiating twinkles — the star reads as radiant GOLD, not muddy.
        let pop = anim::back_out((t / STAR_POP_DUR).clamp(0.0, 1.0)).min(STAR_POP_CAP);
        let throb = 1.0 + FINALE_THROB_AMP * anim::pulse(t, 0.9).max(0.0);
        let r = fl.star_r * pop * throb;
        draw_star_spotlight(fl.trophy.x, fl.trophy.y, r, t);

        // Bobbing balloons (festive RAINBOW colours), behind the dancers. Each
        // bobs on its own phase off the party clock; a tap adds a decaying swing +
        // bob (`balloon_bump`) so a touched balloon swings around rather than pops.
        for (i, (&(bc, brad), &ci)) in fl.balloons.iter().zip(BALLOON_COLOR_IDX.iter()).enumerate() {
            let bob = (t * 1.3 + i as f32 * 1.7).sin() * brad * 0.18;
            let (kx, ky) = self.balloon_bump(i, brad);
            draw_balloon(bc.x + kx, bc.y + bob + ky, brad, fl.floor_y, palette::RAINBOW[ci], i, t);
        }

        // The dancers: the four critters scattered across the floor at varied
        // positions + sizes, each on an idle sway PLUS its tapped move + its
        // melody-driven sing pop. The frog is NOT here — it's the DJ host.
        for i in 0..4 {
            let (c, rr) = fl.dancers[i];
            let color = palette::RAINBOW[PAD_COLOR_IDX[i]];
            let pose = self.dancer_pose(i, rr, t);
            // A tight per-dancer spotlight under each, brightening on the beat. Kept
            // SMALL (was rr*1.2 — adjacent halos merged into one washed-out blob at
            // centre stage) so each dancer reads as separately spotlit, not a wash.
            // Drawn through `soft_glow` (radial falloff, lower peak) so it never hard-
            // edges into a bright splotch on the floor.
            let lift = (-pose.dy / (rr * 0.5)).clamp(0.0, 1.0);
            soft_glow(
                c.x,
                c.y + rr * 0.55,
                rr * (DANCER_GLOW_SCALE + 0.10 * lift),
                color,
                0.20 + 0.12 * lift,
            );
            draw::critter(CRITTERS[i], c.x, c.y, rr, color, &pose);
        }

        // The frog DJ (the host) — prominent on its spot, with a TIGHT warm-amber
        // spotlight (was dj_r*1.4 near-white → it merged with the dancer halos into
        // one pale centre blob; now dj_r*1.1 + a touch fainter so it reads as the
        // host's own discrete spot). Headphones + a party hat make it visibly RUN
        // THE MUSIC (a green frog with a hat alone read as a 5th choir frog).
        let djp = self.frog_dj_pose(fl.dj_r, t);
        soft_glow(
            fl.dj.x,
            fl.dj.y + fl.dj_r * 0.55,
            fl.dj_r * DJ_GLOW_SCALE,
            palette::hex(SPOTLIGHT_AMBER),
            0.22 + 0.10 * (t * 2.0).sin().abs(),
        );
        draw::frog(fl.dj.x, fl.dj.y, fl.dj_r, palette::RAINBOW[3], djp);
        draw::frog_party_hat(fl.dj.x, fl.dj.y, fl.dj_r, djp, palette::RAINBOW[0]);
        draw::frog_headphones(fl.dj.x, fl.dj.y, fl.dj_r, djp, palette::RAINBOW[5]);

        self.confetti.draw();

        // Corner buttons (replay + home), placed identically to the other finale
        // scenes so the kid always finds them.
        let (replay, home, br) = chrome::corner_buttons(f);
        chrome::draw_corner_buttons(replay, home, br);
    }

    /// The pose for dancer `i`: a continuous BEAT-SYNCED groove so the whole floor
    /// dances in time with the tune (not just the one critter singing its note),
    /// its melody-driven SING pop, and — when tapped — a `dance_kind` move layered
    /// on top via a half-sine impulse.
    fn dancer_pose(&self, i: usize, rr: f32, t: f32) -> draw::CritterPose {
        let pi = std::f32::consts::PI;
        // The groove rides the shared melody clock so every dancer bounces in TIME
        // with the music (the old idle sway half-rectified a slow sine, so each
        // dancer sat flat at rest half its cycle and the floor read as static).
        // A small per-dancer phase offset staggers the four into a little wave
        // rather than a robotic unison. `bob` is one smooth hop per beat: 0 = knees
        // bent on the downbeat, 1 = airborne between beats.
        let beat = self.melody_t / BEAT_S - i as f32 * 0.16;
        let bob = 0.5 - 0.5 * (beat * 2.0 * pi).cos();
        // A slow side-to-side lean (staggered) so they sway as they bounce.
        let sway = (t * 1.8 - i as f32 * 0.6).sin();
        // A recurring happy squint (narrow spikes, staggered) so the dancers look
        // gleeful rather than blank while they groove.
        let squint = (t * 1.6 + i as f32 * 1.3).sin().max(0.0).powi(4) * 0.5;
        let mut pose = draw::CritterPose {
            dy: -bob * rr * 0.14,
            rot: sway * 0.12,
            // Anticipation squash-and-stretch: stretch tall when airborne, squash
            // wide when grounded, so the hop reads springy instead of a flat slide.
            sy: 1.0 + 0.08 * bob - 0.04 * (1.0 - bob),
            sx: 1.0 - 0.05 * bob + 0.03 * (1.0 - bob),
            blink: squint,
            ..Default::default()
        };
        // Melody / tap sing pop (mouth open + a little lift).
        if self.sing_t[i] < DANCER_SING_S {
            let s = 1.0 - self.sing_t[i] / DANCER_SING_S;
            pose.sing = s;
            pose.dy -= s * rr * 0.16;
            pose.sy += s * 0.12;
            pose.sx -= s * 0.06;
        }
        // A tapped DANCE MOVE: a distinct (hop / spin / shimmy / squash) recipe.
        if self.dance_t[i] < DANCE_MOVE_S {
            let p = self.dance_t[i] / DANCE_MOVE_S;
            let imp = (p * std::f32::consts::PI).sin();
            match self.dance_kind[i] {
                0 => {
                    // Big hop.
                    pose.dy -= imp * rr * 0.5;
                    pose.sy += imp * 0.12;
                }
                1 => {
                    // Twist/spin (a quick rotation that returns to upright).
                    pose.rot += imp * 0.5;
                    pose.dy -= imp * rr * 0.18;
                }
                2 => {
                    // Side shimmy (two little lateral leans) + small lift.
                    pose.rot += (p * std::f32::consts::PI * 4.0).sin() * 0.18 * imp;
                    pose.dy -= imp * rr * 0.12;
                }
                _ => {
                    // Squash-and-stretch bounce.
                    pose.sy += imp * 0.28;
                    pose.sx -= imp * 0.16;
                    pose.dy -= imp * rr * 0.10;
                }
            }
        }
        pose
    }

    /// The frog DJ's pose: a BEAT-SYNCED host bob (grooving in time with the
    /// dancers) + a tap-triggered hop/spin flourish.
    fn frog_dj_pose(&self, r: f32, t: f32) -> draw::FrogPose {
        let pi = std::f32::consts::PI;
        // One smooth hop per beat, on the shared melody clock, so the host nods in
        // time with the floor instead of on its own loose timer.
        let bob = 0.5 - 0.5 * (self.melody_t / BEAT_S * 2.0 * pi).cos();
        let mut pose = draw::FrogPose {
            dy: -bob * r * 0.10,
            rot: (t * 1.4).sin() * 0.06,
            sy: 1.0 + 0.05 * bob,
            sx: 1.0 - 0.03 * bob,
            ..Default::default()
        };
        if self.frog_t < FROG_FLOURISH_S {
            let p = self.frog_t / FROG_FLOURISH_S;
            let imp = (p * std::f32::consts::PI).sin();
            pose.dy -= imp * r * 0.55; // a big hop
            pose.rot += imp * 0.6; // with a spin
            pose.tongue = imp; // tongue-out ribbit
            pose.blink = (imp * 0.5).min(0.5);
        }
        pose
    }

    /// The tapped-balloon nudge offset `(dx, dy)` for balloon `i`: a decaying
    /// swing (in `balloon_kick[i]`'s direction) with a little upward bob, so a
    /// touched balloon swings around and settles — it never pops. `(0, 0)` idle.
    fn balloon_bump(&self, i: usize, brad: f32) -> (f32, f32) {
        if self.balloon_t[i] >= BALLOON_BUMP_S {
            return (0.0, 0.0);
        }
        let p = self.balloon_t[i] / BALLOON_BUMP_S;
        let env = 1.0 - p; // linear decay to rest
        // A couple of side swings that damp out, plus a single soft upward kick.
        let swing = (p * std::f32::consts::PI * 3.0).sin() * env;
        let dx = self.balloon_kick[i] * brad * 0.55 * swing;
        let dy = -brad * 0.3 * (p * std::f32::consts::PI).sin() * env;
        (dx, dy)
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

    // The REWARD star lives in the empty band ABOVE the choir row (between the pip
    // strip and the top critter heads) so it celebrates WITHOUT ever occluding
    // faces — the top heads sit ~pad/2 above each pad center. (The finale star is
    // sized by `finale_layout`, not here — this layout serves only gameplay.)
    //
    // The reward star is drawn at `star_r * pop * scale`, peaking at
    // `star_r * REWARD_STAR_PEAK_SCALE` (the new-best pop is the worst case).
    // Rather than shrink the RESTING star so its PEAK fits (which left the star
    // tiny on short bands — phone landscape, iPad portrait), the star is
    // BOTTOM-ANCHORED: its lower edge is pinned at `star_bottom` (just above the
    // heads) and the pop grows it UPWARD. So a resting star fills the band big,
    // and at ANY pop magnitude its bottom never dips onto a face. `star_r` is then
    // sized so even the PEAK upward growth (`star_r * REWARD_STAR_PEAK_SCALE` tall)
    // clears the pips above — the band above the anchor bounds the peak radius.
    let heads_top = pads[0].y - pad * 0.5;
    // Pin the star's bottom just above the heads; the pop grows it upward only.
    let star_bottom = heads_top - pad * 0.06; // a small margin above the heads
    // The headroom above the anchor, up to the top of the play region (the pips
    // don't draw during Reward, so the star may rise through that strip, but it
    // must NOT overrun the topbar). The PEAK star (diameter
    // `2 * star_r * REWARD_STAR_PEAK_SCALE`) must fit that headroom; that bound,
    // the preferred `pad`-relative size, and a floor pick the resting radius.
    let headroom = (star_bottom - region_top).max(0.0);
    let star_r = (pad * 0.5)
        .min(headroom / (2.0 * REWARD_STAR_PEAK_SCALE))
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

// --- finale dance-party layout ----------------------------------------------

/// The Finale dance-party geometry — a DISTINCT arrangement from the gameplay
/// row/grid (the achievement made a hero): the four critters SCATTERED at varied
/// positions + sizes across a dance-floor band in a loose arc, the frog DJ host
/// front-and-centre + larger, balloons bobbing above, bunting up top, and a
/// trophy star high. All viewport-derived (vw/vh/vmin + safe area) so it fits
/// every form factor (phone-landscape 844×390 is the tight case).
struct FinaleLayout {
    /// A base unit (≈ a dancer's body radius) the whole scene scales from.
    unit: f32,
    /// The four dancers (center, body radius), scattered across the floor in a
    /// loose arc at VARIED sizes — index 0..4 maps frog→duck→cat→owl colours, but
    /// the FROG critter art is reused as the DJ; the four dancers are the choir.
    dancers: [(Vec2, f32); 4],
    /// The frog DJ host (center + larger body radius) — the hero of the party.
    dj: Vec2,
    dj_r: f32,
    /// The festive balloons (center + radius), bobbing above the floor.
    balloons: [(Vec2, f32); FINALE_BALLOONS],
    /// The dance-floor ellipse: vertical center + radii (the band the dancers
    /// stand on), plus the tile band's row count for the checker tiles.
    floor_y: f32,
    floor_rx: f32,
    floor_ry: f32,
    /// The trophy star center (bottom-anchored above the dancers) + base radius.
    trophy: Vec2,
    star_r: f32,
    /// Bunting line: the y the flags hang from + how far the lowest dips.
    bunting_y: f32,
    bunting_drop: f32,
}

fn finale_layout(f: &crate::layout::Frame) -> FinaleLayout {
    let (w, h) = (f.w, f.h);
    let vmin = f.vmin(1.0);
    // Base dancer radius: a generous-but-clamped fraction of the smaller dim, so
    // the dancers are big on tablets yet still fit the short phone-landscape band.
    let unit = (vmin * 0.085).clamp(34.0, 96.0);

    // The dance floor sits in the lower-middle band, lifted off the very bottom so
    // the corner buttons + safe inset stay clear.
    let floor_y = h * 0.72 - f.safe.bottom * 0.5;
    let floor_rx = (w * 0.46).min(w / 2.0 - 12.0);
    let floor_ry = (h * 0.12).clamp(unit * 0.7, unit * 1.6);

    // The frog DJ: front-and-centre on the floor, the largest character (the
    // hero), standing a touch ahead (lower) so it reads clearly in FRONT of the
    // back dancers rather than overlapping them.
    let dj_r = unit * 1.3;
    let dj = vec2(w * 0.5, floor_y + dj_r * 0.18);

    // The four dancers scattered in a loose arc ACROSS the floor at VARIED
    // positions + sizes (clearly not the even gameplay row/grid). The DJ owns the
    // centre-front, so the back pair (1,2) sit HIGHER and further out and the
    // flanks (0,3) sit low + wide — a fanned cluster, never an even row.
    // (fx as a fraction of width, fy lift above floor_y in units, size multiplier)
    let specs = [
        (0.13_f32, 0.20_f32, 1.05_f32), // far left, low + big
        (0.32, 1.35, 0.78),             // left-back, high + small
        (0.68, 1.35, 0.82),             // right-back, high
        (0.87, 0.20, 1.08),             // far right, low + big
    ];
    // On short landscape (phone) the top band is doing star + balloons + bunting
    // at once; pull the high back pair (1,2) DOWN so the back dancers don't crowd
    // under the bunting/balloon stack. A no-op on tall viewports.
    let short_land = !f.is_portrait() && h < 480.0;
    let back_lift_scale = if short_land { 0.72 } else { 1.0 };
    let mut dancers = [(vec2(0.0, 0.0), unit); 4];
    for (i, &(fx, lift, sz)) in specs.iter().enumerate() {
        let rr = unit * sz;
        let cx = w * fx;
        // Only the high back pair gets pulled down; the low flanks stay put.
        let lift = if lift > 1.0 { lift * back_lift_scale } else { lift };
        let cy = floor_y - lift * unit;
        dancers[i] = (vec2(cx, cy), rr);
    }

    // Balloons bob in the upper band, spread across the width on a gentle arc, at
    // slightly varied sizes for a festive cluster (kept below the bunting).
    let brad = (unit * 0.55).clamp(20.0, 64.0);
    let bal_band = (h * 0.30).max(brad * 2.0 + f.safe.top);
    let bspecs = [
        (0.12_f32, 0.55_f32, 1.0_f32),
        (0.30, 0.20, 0.85),
        (0.50, 0.62, 1.1),
        (0.70, 0.18, 0.9),
        (0.88, 0.52, 1.0),
    ];
    let mut balloons = [(vec2(0.0, 0.0), brad); FINALE_BALLOONS];
    for (i, &(fx, fy, sz)) in bspecs.iter().enumerate() {
        balloons[i] = (vec2(w * fx, bal_band * (0.45 + fy * 0.45)), brad * sz);
    }

    // The trophy star, bottom-anchored above the dancers (in the gap below the
    // balloon cluster), sized so its throbbing pop never overruns the bunting.
    let dancer_top = dancers
        .iter()
        .map(|(c, r)| c.y - r * 1.6)
        .fold(f32::INFINITY, f32::min);
    let star_bottom = dancer_top - unit * 0.2;
    // Hang the bunting a touch higher + shallower on short landscape so the top
    // band isn't bunting + balloons + star all at once.
    let bunting_y = (f.safe.top + h * if short_land { 0.04 } else { 0.06 }).max(brad * 1.2);
    let bunting_drop = h * if short_land { 0.04 } else { 0.05 };
    let headroom = (star_bottom - (bunting_y + bunting_drop + unit * 0.4)).max(unit * 0.6);
    let star_r = (unit * 1.1).min(headroom / (2.0 * STAR_POP_CAP)).max(unit * 0.45);
    let trophy = vec2(w * 0.5, star_bottom - star_r);

    FinaleLayout {
        unit,
        dancers,
        dj,
        dj_r,
        balloons,
        floor_y,
        floor_rx,
        floor_ry,
        trophy,
        star_r,
        bunting_y,
        bunting_drop,
    }
}

/// A festive balloon: a teardrop body + a tiny knot + a thin curving string down
/// toward the floor. Local to the finale (not reused elsewhere). RAINBOW-coloured.
/// `i` (the balloon index) + `t` (the party clock) give each string a DISTINCT
/// curve that gently SWAYS as the balloon floats — no two hang alike, and none
/// reads as the old straight two-segment line.
fn draw_balloon(cx: f32, cy: f32, r: f32, floor_y: f32, color: Color, i: usize, t: f32) {
    use std::f32::consts::PI;
    // The string: a smooth curved ribbon from the knot down toward the floor,
    // sampled finely (a sine bow, not two straight segments). Each balloon bows a
    // different way + amount (off `i`) and the whole curve sways slowly (off `t`),
    // with the free tip drifting most, so the strings drift like real ribbons.
    let knot_y = cy + r * 1.08;
    let end_y = (cy + r * 4.0).min(floor_y);
    let span = (end_y - knot_y).max(1.0);
    let fi = i as f32;
    // Per-balloon curve: alternating bow direction + a size that varies per index,
    // so the five strings clearly differ in shape (not one repeated curve).
    let dir = if i.is_multiple_of(2) { 1.0 } else { -1.0 };
    let bow_amp = r * (0.42 + 0.22 * (fi * 1.7).sin().abs());
    // A slow sway of the whole curve + a larger drift of the free tip.
    let sway = (t * 1.05 + fi * 1.7).sin();
    let twist = (t * 0.8 + fi * 2.3).cos();
    const N: usize = 18;
    let mut pts = Vec::with_capacity(N + 1);
    for k in 0..=N {
        let u = k as f32 / N as f32; // 0 at knot → 1 at tip
        let y = knot_y + span * u;
        // A single sine bow (peaks mid-string) that breathes with `twist`, plus a
        // tip drift that grows toward the free end so the bottom swings most.
        let bow = (u * PI).sin() * bow_amp * dir * (0.7 + 0.3 * twist);
        let drift = sway * r * 0.22 * u * u;
        pts.push(vec2(cx + bow + drift, y));
    }
    draw::stroke_path(&pts, (r * 0.06).max(1.5), palette::hexa(0x6f6e77, 0.7));
    // The body: a slightly tall ellipse, with a soft highlight + a darker base.
    draw::fill_ellipse(cx, cy, r * 0.86, r * 1.04, 0.0, color);
    let hi = palette::hexa(0xffffff, 0.35);
    draw::fill_ellipse(cx - r * 0.28, cy - r * 0.34, r * 0.24, r * 0.34, 0.0, hi);
    // The knot: a tiny triangle/disc at the base.
    draw::disc(cx, knot_y, r * 0.12, color);
}

/// A soft radial glow with NO hard disc edge: a few concentric discs from `r`
/// down, each at a fraction of `peak` alpha, so opacity builds smoothly toward the
/// centre and fades to nothing at the rim. Replaces a single translucent disc,
/// whose hard edge + pale fill read as a bright "splotch" on the floor (and blew
/// out further under a device's gamma-correct blending vs. the software renders).
fn soft_glow(x: f32, y: f32, r: f32, color: Color, peak: f32) {
    const RINGS: usize = 5;
    let per = peak / RINGS as f32;
    for k in 0..RINGS {
        // Outer (largest) first, smaller discs stacked on top → the centre
        // accumulates `per` RINGS times (≈ `peak`) while the rim keeps just one.
        let f = k as f32 / RINGS as f32;
        draw::disc(x, y, r * (1.0 - 0.78 * f), Color { a: per, ..color });
    }
}

/// The dance floor's checker tiles: a band of alternating rounded-rect tiles
/// across the floor ellipse, with a subtle parallax shimmer off the party clock.
/// Local to the finale.
fn draw_dance_floor(fl: &FinaleLayout, t: f32) {
    let cols = 7usize;
    let rows = 3usize;
    let band_w = fl.floor_rx * 1.7;
    let band_h = fl.floor_ry * 1.5;
    let x0 = fl.dj.x - band_w / 2.0;
    let y0 = fl.floor_y - band_h * 0.35;
    let tw = band_w / cols as f32;
    let th = band_h / rows as f32;
    let gap = tw * 0.12;
    for row in 0..rows {
        for col in 0..cols {
            // Tiles narrow toward the back (a cheap perspective) — back rows inset.
            let inset = row as f32 * tw * 0.18;
            let x = x0 + inset + col as f32 * tw;
            let bw = tw - inset * 2.0 / cols as f32;
            if bw <= gap {
                continue;
            }
            let y = y0 + row as f32 * th;
            // The checker, with a slow shimmer that flips the parity over time.
            let lit = (row + col + (t * 1.5) as usize).is_multiple_of(2);
            let base = if lit { FLOOR_TILE_A } else { FLOOR_TILE_B };
            // Back rows fade a touch (depth).
            let a = base.a * (1.0 - row as f32 * 0.18);
            let col_c = Color { a, ..base };
            draw::rounded_rect(x + gap / 2.0, y + gap / 2.0, bw - gap, th - gap, th * 0.18, col_c);
        }
    }
}

/// The reward/finale gold star with its soft halo: a faint gold disc behind a
/// solid gold star of radius `r`. Shared by Reward + Finale so the two never
/// drift. `halo_alpha`/`halo_scale` tune the wash (Finale runs a touch bigger).
fn draw_star_halo(x: f32, y: f32, r: f32, halo_alpha: f32, halo_scale: f32) {
    let halo = Color { a: halo_alpha, ..palette::GOLD };
    draw::disc(x, y, r * halo_scale, halo);
    draw::star(x, y, r, palette::GOLD);
}

/// The finale trophy star, drawn to read as RADIANT GOLD over the dark violet
/// party backdrop. The old stacked faint-gold discs browned out to a dull tan
/// ring there, so this draws a single TIGHT bright core plus a few radiating
/// sparkle points (a slow twinkle on `t`) behind the solid gold star — light
/// that shines outward, not a flat wash that muddies. `t` only spins/twinkles
/// the sparkles; the star size is the caller's `r`.
fn draw_star_spotlight(x: f32, y: f32, r: f32, t: f32) {
    // Radiating sparkle points: small bright twinkles spoked around the star,
    // slowly rotating + breathing so it never goes static. NO faint backing disc:
    // a half-opaque gold disc over the violet→amber backdrop browned out to a dull
    // tan ring, so the radiance is carried by bright opaque sparkles instead.
    const SPARKS: usize = 8;
    let pi = std::f32::consts::PI;
    for i in 0..SPARKS {
        let a = t * 0.6 + i as f32 * (2.0 * pi / SPARKS as f32);
        // Alternate near/far spokes and breathe each on its own phase.
        let far = if i % 2 == 0 { 1.55 } else { 1.95 };
        let breathe = 0.5 + 0.5 * (t * 2.4 + i as f32 * 0.8).sin();
        let sr = r * far;
        let sx = x + a.cos() * sr;
        let sy = y + a.sin() * sr;
        // A bright opaque warm-gold sparkle with a hot white centre (twinkle).
        let pr = r * (0.11 + 0.07 * breathe);
        draw::disc(sx, sy, pr, palette::hexa(0xffe066, 0.95));
        draw::disc(sx, sy, pr * 0.45, palette::hexa(0xfffcea, 0.95));
    }
    // The solid gold star on top, with a thin brighter inner star so it reads as
    // LIT gold rather than a flat shape over the dark backdrop.
    draw::star(x, y, r, palette::GOLD);
    draw::star(x, y, r * 0.62, palette::hexa(0xffe066, 0.9));
}

/// The get-ready overlay alpha at `t` seconds into Ready: dim-in → hold →
/// bloom-out over `READY_TOTAL_S`. Eases 0 → `READY_DIM_PEAK` (lights down to
/// settle), holds at the peak, then eases back to 0 (lights up to reveal),
/// reaching exactly 0 at `READY_TOTAL_S` so the first note lands on the full
/// reveal ("now"). Cubic ease both directions for a soft, non-mechanical feel.
/// The scene-entry overlay alpha at `t` seconds into Intro: the dim wash eases
/// from `READY_DIM_PEAK` back to 0 over `INTRO_FADE_S` (the soft entrance), then
/// stays clear through the still orientation hold. Cubic ease for a soft feel.
fn intro_overlay_alpha(t: f32) -> f32 {
    if t >= INTRO_FADE_S {
        return 0.0;
    }
    let u = anim::clamp01(t / INTRO_FADE_S);
    READY_DIM_PEAK * (1.0 - anim::ease_out_cubic(u))
}

fn ready_overlay_alpha(t: f32) -> f32 {
    let f = anim::clamp01(t / READY_TOTAL_S); // fraction of the cue
    if f < READY_DIM_IN {
        // Dim-in: ease 0 → peak.
        let u = f / READY_DIM_IN;
        READY_DIM_PEAK * anim::ease_out_cubic(u)
    } else if f < READY_HOLD_END {
        // Hold at the dim.
        READY_DIM_PEAK
    } else {
        // Bloom-out: ease peak → 0 across the remaining span.
        let u = (f - READY_HOLD_END) / (1.0 - READY_HOLD_END);
        READY_DIM_PEAK * (1.0 - anim::ease_out_cubic(u))
    }
}

// --- capture-state construction (goldens) -----------------------------------

/// The phase a golden wants the scene pinned into. Mirrors patterns' approach of
/// constructing the scene directly into a representative single frame.
pub(crate) enum CaptureState {
    /// The get-ready cue at the dim peak (the still choir under the fullscreen
    /// dim-and-bloom overlay) — the environmental "settle → now", made reviewable.
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
                // The dim peak (mid-hold): the still choir under the fullscreen
                // dim-and-bloom overlay at full READY_DIM_PEAK alpha — a
                // representative "settle" frame of the environmental get-ready cue.
                // Critters stay neutral (no Ready pose) for a deterministic golden.
                sc.phase = Phase::Ready { t: 0.5 * READY_TOTAL_S };
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
                // A full FINALE_SPAN sequence completed → the closing dance party.
                sc.sequence = (0..FINALE_SPAN).map(|i| (i % 4) as u8).collect();
                sc.enter_finale(ctx0);
                // Settle the entrance, then pin a representative frame: a couple of
                // dancers mid-move, the frog DJ mid-flourish, balloons up, confetti
                // falling. Driving ~0.5s lands the entrance pops + some rain.
                drive(&mut sc, ctx0, 16);
                // Pin a representative groove phase so the still frame clearly shows
                // the beat-synced bounce mid-hop (the per-dancer stagger spans the
                // four across the bounce, reading as a little wave).
                sc.melody_t = BEAT_S * 0.5;
                // Force two distinct dance moves + the DJ flourish at fixed phases so
                // the single captured frame clearly shows the party in motion.
                sc.dance_t[0] = DANCE_MOVE_S * 0.4;
                sc.dance_kind[0] = 0; // a big hop
                sc.dance_t[3] = DANCE_MOVE_S * 0.5;
                sc.dance_kind[3] = 3; // a squash bounce
                sc.sing_t[1] = DANCER_SING_S * 0.3; // duck mid-sing
                sc.sing_t[2] = DANCER_SING_S * 0.5; // cat mid-sing
                sc.frog_t = FROG_FLOURISH_S * 0.18; // DJ early in its hop (clean still)
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
