# Port spec: audio + confetti FX

Exact reimplementation reference for the Web Audio sounds
(`src/shared/sounds.ts`) and canvas confetti (`src/shared/confetti.ts`).
Goal: synthesize byte-identical-feeling PCM tones and a matching particle
burst in Rust. All numeric constants below are authoritative — copy them.

---

## 1. Audio

### 1.1 Synthesis model (must match for parity)

Every sound is a sum of independent oscillator "notes" mixed through a
master gain of `1.0` into the output. Each note is one oscillator +
its own gain (envelope) node.

Per-note parameters (`NoteSpec`):
- `freq` — oscillator frequency, Hz (constant for the note's life; no glide).
- `start` — onset offset in seconds, relative to the call's `t0`.
- `dur` — duration in seconds.
- `gain` — envelope peak amplitude, default `0.18`.
- `type` — oscillator waveform, default `sine`.

Oscillator waveforms used: `sine` and `triangle` only. Match the Web
Audio band-limited shapes closely enough that the ear can't tell; a naive
non-band-limited triangle is acceptable for these short, low-partial
tones, but a band-limited triangle is preferred for the low-frequency
frog notes to avoid aliasing buzz.

Mathematical definitions (phase `φ` in turns, `φ = (freq * t) mod 1`):
- sine: `sin(2π·φ)`
- triangle: `4·|φ − 0.5| − 1` (range −1..1, peaks at φ=0, troughs at φ=0.5)

### 1.2 Envelope (applied to every note)

Timeline, with `start = t0 + note.start`, `end = start + note.dur`:

1. At `start`: gain = `0`.
2. Linear ramp to `peak` over `0.01 s` (attack), i.e. gain hits `peak`
   at `start + 0.01`.
3. Exponential ramp from `peak` down to `0.0001` ending exactly at `end`.
   - Web Audio `exponentialRampToValueAtTime` interpolates geometrically
     between the value at the previous event time (`peak` at `start+0.01`)
     and the target `0.0001` at `end`. Replicate as:
     `g(t) = peak · (0.0001/peak)^((t − (start+0.01)) / (end − (start+0.01)))`
     for `t` in `[start+0.01, end]`.
   - Exponential ramps cannot reach 0, hence the `0.0001` floor.
4. Oscillator physically stops at `end + 0.02` (gain is already ~0; this
   is just tail slack). For PCM you can stop rendering the note at `end`;
   the extra 20 ms is inaudible.

Attack note: if `dur < 0.01` the attack ramp would overrun `end`; in
practice the shortest note is `playTap` at `dur = 0.05`, so attack always
completes well before the decay. Keep the `start+0.01` knee fixed.

No master envelope, no compression, no reverb, no filtering. Notes simply
sum; brief overlaps (the chords below) can exceed the per-note peak — that
is intended and not clamped in the source. If your Rust mixer hard-clips,
either allow headroom or soft-clip; the JS path does neither.

### 1.3 Sounds

All times in seconds, freqs in Hz. `t0` = moment of the call.

#### playCorrect(streak = 0) — ascending major triad C5–E5–G5
Streak pitch-shift rule: `shift = 2^(min(streak, 5) / 12)`.
- Multiply **every** note freq by `shift`.
- `streak` is clamped to `5`, so max shift is `2^(5/12) ≈ 1.33484` (+5
  semitones). streak 0 → shift 1.0.
- Default waveform (sine), default gain (0.18).

| note | base freq | start | dur  |
|------|-----------|-------|------|
| C5   | 523.25    | 0.00  | 0.18 |
| E5   | 659.25    | 0.09  | 0.18 |
| G5   | 783.99    | 0.18  | 0.28 |

(Effective freq = base × shift.) Notes overlap, forming a chord by the end.

#### playIncorrect() — gentle two-note descent G4→E4
Waveform `triangle`, gain `0.14` for both. Never harsh.

| note | freq   | start | dur  | type     | gain |
|------|--------|-------|------|----------|------|
| G4   | 392.0  | 0.00  | 0.16 | triangle | 0.14 |
| E4   | 329.63 | 0.12  | 0.22 | triangle | 0.14 |

#### playLevelUp() — four-note rising fanfare C5–E5–G5–C6
Default waveform (sine), default gain (0.18). No streak shift.

| note | freq    | start | dur  |
|------|---------|-------|------|
| C5   | 523.25  | 0.00  | 0.14 |
| E5   | 659.25  | 0.10  | 0.14 |
| G5   | 783.99  | 0.20  | 0.14 |
| C6   | 1046.5  | 0.32  | 0.32 |

#### playTap() — single soft tick
One sine note. Quiet (gain 0.08), very short.

| freq | start | dur  | type | gain |
|------|-------|------|------|------|
| 660  | 0.00  | 0.05 | sine | 0.08 |

#### playFrog() — two-syllable "ri-bbit"
Four triangle notes in two syllable pairs; each syllable is a low note
immediately followed by a higher note ~50 ms later (the rapid up-bend is
*not* a glide — it's two discrete overlapping notes). Kept at/under the
level-up loudness so the modal doesn't outshine in-game wins.

| note | freq | start | dur  | type     | gain |
|------|------|-------|------|----------|------|
| 1    | 220  | 0.00  | 0.09 | triangle | 0.16 |
| 2    | 300  | 0.05  | 0.08 | triangle | 0.14 |
| 3    | 200  | 0.18  | 0.10 | triangle | 0.16 |
| 4    | 280  | 0.22  | 0.09 | triangle | 0.14 |

Syllable 1 = notes 1+2 (~0.00–0.13 s), syllable 2 = notes 3+4
(~0.18–0.31 s). Total ~0.31 s.

### 1.4 Mute + context lifecycle (parity behavior)

- Module-level `muted` flag, default `false`. `setMuted(bool)` /
  `isMuted()`.
- **Muted short-circuits before any context work**: `ensureCtx()` returns
  null immediately if `muted`, so no audio object is created and no sound
  plays. Rust equivalent: gate at the top of every play call; do not even
  start synthesis when muted.
- Single lazily-created `AudioContext`, reused for the process lifetime.
  Created on first non-muted play call.
- **iOS gesture nuance (for parity awareness):** on iOS Safari the
  `AudioContext` is created in `suspended` state and stays silent until a
  user gesture resumes it. The code calls `ctx.resume()` on every
  `ensureCtx()` when `state === 'suspended'` (cheap no-op once running).
  In a native Rust audio backend there is usually no equivalent suspend
  state, but if the port ever runs in a browser/WASM context it must
  preserve "first sound after a tap" semantics: lazily open the output
  stream on a user gesture, not at startup, or the first cue is dropped.
- `currentTime` base: each call snapshots the context's current time as
  `t0` and schedules all its notes relative to it. Concurrent calls
  (e.g. tap then correct) layer independently; there is no voice
  stealing or polyphony cap. Rust port should likewise allow overlapping
  one-shot voices.

---

## 2. Confetti

Canvas 2D particle burst. One short burst per `burst()` call; particles
fade out over ~1.0–1.6 s. Self-contained, no deps.

### 2.1 Canvas + DPR handling

- Target element: `<canvas id="confetti">` (full-viewport overlay).
- On first use and on every window `resize`:
  - `dpr = window.devicePixelRatio || 1`.
  - Backing store: `canvas.width = floor(innerWidth · dpr)`,
    `canvas.height = floor(innerHeight · dpr)`.
  - CSS size: `style.width = innerWidth px`, `style.height = innerHeight px`.
  - Context transform reset to `setTransform(dpr, 0, 0, dpr, 0, 0)` so
    **all drawing/physics is done in CSS (logical) pixels** and the
    transform scales to device pixels. Physics constants below are in
    logical px.
  - On resize, **all in-flight particles are dropped** (`particles = []`)
    so an orientation flip doesn't strand confetti at stale coordinates.
- Rust port: render in logical pixels, multiply by `dpr` (or your render
  scale) at blit time; clear the whole backing store each frame.

### 2.2 Emit anchor (reads the #sequence rect)

Defaults if no sequence card is found:
- `cx = innerWidth / 2`
- `emitY = min(innerHeight · 0.55, 380)`
- `spreadX = 60`

If `<#sequence>` exists and its bounding rect has `width>0 && height>0`,
override with the live card geometry:
- `cx = rect.left + rect.width / 2` (horizontal center of the card)
- `emitY = rect.bottom` (just below the card, so particles rise up
  *through* it past the just-placed answer)
- `spreadX = min(rect.width / 3, 140)`

Rust port: feed in the target widget's bounding box (left, bottom, width)
in logical px; apply the same formulas. Fall back to the defaults when no
anchor is available.

### 2.3 Particle initialization

Default `count = 80` per burst. For each particle, with `rand(a,b)` =
uniform in `[a,b)` (`a + random()·(b−a)`):

| field | init                              | meaning                    |
|-------|-----------------------------------|----------------------------|
| x     | `cx + rand(-spreadX, spreadX)`    | logical px                 |
| y     | `emitY + rand(-10, 10)`           | logical px                 |
| vx    | `rand(-220, 220)`                 | px/s                       |
| vy    | `rand(-360, -180)`                | px/s (negative = upward)   |
| size  | `rand(6, 10)`                     | px (square width)          |
| color | uniform pick from COLORS          | see palette                |
| rot   | `rand(0, 2π)`                     | radians                    |
| vr    | `rand(-6, 6)`                     | rad/s (rotation velocity)  |
| life  | `rand(1.0, 1.6)`                  | seconds remaining          |

COLORS palette (pick index = `floor(random()·6)`):
`#f582ae` (pink), `#ffd166` (yellow), `#06d6a0` (green), `#118ab2`
(blue), `#9b5de5` (purple), `#ef476f` (red).

Particles from new bursts append to the existing array (bursts stack).

### 2.4 Per-frame update (physics + draw)

Driven by an animation loop. `dt` (seconds) = time since last frame,
**clamped to a 0.05 s max** (`dt = min(0.05, (ts − lastTs)/1000)`), so a
long frame gap doesn't teleport particles.

Constants:
- Gravity `g = 600` px/s². **No horizontal drag, no air resistance** —
  `vx` is constant for a particle's whole life; only `vy` accelerates.

For each particle each frame, in this order:
1. `life -= dt`; if `life <= 0`, remove it (skip).
2. `vy += g · dt`
3. `x += vx · dt`
4. `y += vy · dt`
5. `rot += vr · dt`
6. `alpha = clamp(life, 0, 1)` — i.e. fully opaque while `life > 1 s`,
   then linearly fades to 0 over the final second. (Note: particles with
   `life` initialized above 1.0 spend their first `life−1` seconds at full
   opacity, then fade.)
7. Draw: save state, `globalAlpha = alpha`, translate to `(x, y)`, rotate
   by `rot`, fill the particle color, draw a rectangle centered at origin:
   `fillRect(-size/2, -size/2, size, size·0.6)` — width `size`, height
   `size·0.6` (a flat 5:3 chip, not a square), restore.

Loop continues while any particles remain; stops (idles) when the array
empties, restarts on the next `burst()`.

### 2.5 Rust particle-system summary

- Spawn 80 chips at the anchor; uniform random init per table 2.3.
- Integrate with semi-implicit Euler: apply gravity to `vy`, then advance
  position; no drag; constant `vx`. Clamp `dt ≤ 0.05`.
- Spin each chip at constant angular velocity.
- Fade: opacity = `clamp(life, 0, 1)`, decremented by real `dt`.
- Draw rotated rect `size × (size·0.6)` centered on the chip, premultiplied
  by alpha; clear and redraw the full surface each frame; account for DPR.
- The "feel": chips shoot mostly upward (vy −180..−360) with sideways
  spread (vx ±220), arc over under gravity 600, tumble (±6 rad/s), and
  fade out in the last second of a 1.0–1.6 s life.
