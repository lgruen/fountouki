# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Pen routes for the tracing game — stroke ORDER and DIRECTION per glyph.

Authored against `tas-beginners-chart.png` (the official Tasmanian Beginner's
Alphabet chart: numbered arrows, `*` at each start) plus the verbalisation
prompts in the Tasmanian Handwriting Guidelines (2023). Coordinates were read
off the *built* font's skeleton, not guessed: see README.md for the probe
workflow (endpoints and junctions in the same normalized frame as the
waypoints below).

Waypoints are normalized to the glyph's ink bbox: x 0..1 left->right,
y 0..1 bottom->top (descenders included). The string "dot" means "the nearest
small detached component" (the i/j dots).

Lowercase in this style is **one continuous stroke per letter**, with exactly
three exceptions: `f`, `t`, `x` lift once, and `i`, `j` add their dot. The
pen therefore retraces a lot — up a stem, around an oval and back down it —
which the router expresses as the same skeleton pixels appearing in two
consecutive shortest-path segments.

Families (the teaching order in `core::tracing::ORDER`):

* anticlockwise `c o a d g q e s f` — yes, `f` really is grouped here (the
  guidelines' p37/p59 family lists read "c o a d g q, plus e s f" — its top
  curl starts like a c); it sits last in the family because it is the hardest
  of the nine. Every oval starts at **2 o'clock** and
  travels anticlockwise; `a d g q` then run back up the closing side and down
  the stem, `e` starts low on its crossbar, `f` curls back at the top.
* stick `l i t j` — straight down; `l` flicks out, `t` crosses after.
* wave `u y` — down, inverted arch, up, then straight back down (`u`) or on
  into the descender hook (`y`).
* clockwise `r n m h p b k` — down, stop, **trace up the stem**, curve over.
* diagonal `v w x z` — sharp points, no curves.
"""

# Pen routes per letter. Each stroke is a list of (nx, ny) waypoints, or the
# string "dot".
ROUTES = {
    # Oval anticlockwise from 2 o'clock, up the closing side back to the top,
    # then straight down the stem to the baseline (chart: arrows 1 + 2).
    "a": [[(0.99, 0.98), (0.37, 0.88), (0.07, 0.43), (0.29, 0.06),
           (0.79, 0.38), (0.93, 0.91), (0.74, 0.05)]],
    # Tall down to the foot, back up the stem, then the bowl clockwise,
    # closing on the stem just above the baseline.
    "b": [[(0.50, 0.97), (0.05, 0.03), (0.20, 0.31), (0.69, 0.47),
           (0.94, 0.28), (0.64, 0.05), (0.11, 0.08)]],
    # Oval segment only: 2 o'clock, anticlockwise, out to the right.
    "c": [[(0.87, 0.85), (0.06, 0.39), (0.95, 0.32)]],
    # Oval, then on UP the closing side to the ascender, then all the way down
    # and out through the exit flick.
    "d": [[(0.73, 0.45), (0.27, 0.45), (0.05, 0.21), (0.28, 0.03),
           (0.64, 0.24), (0.96, 0.97), (0.96, 0.12)]],
    # Starts low, on the crossbar: up to the right, round the loop
    # anticlockwise, back through the start and out along the bottom.
    "e": [[(0.13, 0.47), (0.66, 0.57), (0.91, 0.92), (0.37, 0.82),
           (0.13, 0.47), (0.08, 0.27), (0.24, 0.08), (0.87, 0.20)]],
    # 1: curl back at the top, then straight down to the baseline (no
    # descender in this style). 2: crossbar left->right.
    "f": [
        [(0.96, 0.88), (0.67, 0.97), (0.33, 0.65), (0.06, 0.03)],
        [(0.04, 0.48), (0.48, 0.49)],
    ],
    # a's oval, then down past the baseline into the left hook.
    "g": [[(0.99, 0.98), (0.45, 0.94), (0.21, 0.72), (0.40, 0.53),
           (0.80, 0.69), (0.94, 0.96), (0.64, 0.26), (0.05, 0.13)]],
    # Tall down, trace up the stem, one arch over to the baseline.
    "h": [[(0.54, 0.97), (0.05, 0.03), (0.22, 0.26), (0.57, 0.43),
           (0.77, 0.02)]],
    # Straight down, then the dot (always last).
    "i": [
        [(0.84, 0.95), (0.18, 0.05)],
        "dot",
    ],
    # Down past the baseline into the left hook, then the dot.
    "j": [
        [(0.95, 0.98), (0.69, 0.41), (0.05, 0.12)],
        "dot",
    ],
    # Tall down, trace up, loop clockwise back onto the stem, then the leg
    # out to the baseline (chart: 3 movements, no pen lift).
    "k": [[(0.51, 0.97), (0.05, 0.02), (0.21, 0.26), (0.53, 0.43),
           (0.91, 0.46), (0.80, 0.29), (0.22, 0.26), (0.70, 0.03)]],
    # Tall down with the exit flick.
    "l": [[(0.83, 0.97), (0.33, 0.41), (0.91, 0.13)]],
    # Down, trace up, arch, down, trace up, arch (n with a second hump).
    "m": [[(0.17, 0.94), (0.03, 0.05), (0.19, 0.62), (0.44, 0.95),
           (0.42, 0.04), (0.54, 0.56), (0.73, 0.89), (0.86, 0.04)]],
    # Down, trace up the stem, one arch over to the baseline.
    "n": [[(0.25, 0.93), (0.05, 0.05), (0.17, 0.46), (0.51, 0.82),
           (0.79, 0.05)]],
    # Closed oval: 2 o'clock, anticlockwise, back to the start.
    "o": [[(0.85, 0.84), (0.55, 0.96), (0.07, 0.49), (0.42, 0.05),
           (0.91, 0.56), (0.85, 0.84)]],
    # Down past the baseline, trace up to just under the top, bowl clockwise,
    # closing on the stem at the baseline.
    "p": [[(0.41, 0.98), (0.04, 0.02), (0.35, 0.82), (0.76, 0.98),
           (0.95, 0.78), (0.71, 0.55), (0.29, 0.58)]],
    # a's oval, then down past the baseline into the exit flick (g's hook
    # mirrored — the pair is taught as opposites).
    "q": [[(0.99, 0.98), (0.34, 0.93), (0.07, 0.71), (0.25, 0.53),
           (0.76, 0.69), (0.93, 0.95), (0.59, 0.30), (0.87, 0.10)]],
    # Down, trace up, small curve over, finishing at ~2 o'clock.
    "r": [[(0.28, 0.94), (0.06, 0.04), (0.23, 0.63), (0.55, 0.90),
           (0.95, 0.82)]],
    # 2 o'clock, curl back anticlockwise, slope down, curl clockwise.
    "s": [[(0.93, 0.86), (0.49, 0.50), (0.07, 0.13)]],
    # 1: down (t is shorter than the other ascenders). 2: crossbar.
    "t": [
        [(0.65, 0.97), (0.12, 0.03)],
        [(0.05, 0.62), (0.94, 0.64)],
    ],
    # Down, inverted arch, up to the top — then stop and come straight back
    # down the right side (one movement, one deliberate reversal).
    "u": [[(0.20, 0.96), (0.06, 0.17), (0.45, 0.11), (0.95, 0.97),
           (0.74, 0.05)]],
    # Diagonal down, sharp point, diagonal up.
    "v": [[(0.05, 0.95), (0.38, 0.08), (0.95, 0.95)]],
    # Four diagonals of equal height (not two v's — the middle peak reaches
    # x-height).
    "w": [[(0.03, 0.95), (0.18, 0.11), (0.49, 0.83), (0.69, 0.11),
           (0.97, 0.95)]],
    # Two separate downstrokes, crossing at half x-height.
    "x": [
        [(0.26, 0.95), (0.70, 0.05)],
        [(0.95, 0.95), (0.06, 0.05)],
    ],
    # u's arch, then on down past the baseline into the left hook.
    "y": [[(0.30, 0.98), (0.14, 0.61), (0.46, 0.54), (0.95, 0.98),
           (0.62, 0.26), (0.04, 0.13)]],
    # Across, sharp stop, back down the diagonal, sharp stop, across. The two
    # corner waypoints are the corner *tips*, so the pen reaches the point and
    # turns there instead of cutting it.
    "z": [[(0.16, 0.95), (0.54, 0.95), (0.98, 0.96), (0.50, 0.50),
           (0.04, 0.04), (0.51, 0.04), (0.91, 0.04)]],
}

# Order the traced glyphs are emitted in (and the only glyphs ROUTES covers).
LETTERS = "abcdefghijklmnopqrstuvwxyz"

# Pen reversals on a degree-2 skeleton pixel with no wedge branch beyond it
# are usually waypoint snap overshoot: the waypoint landed past a junction,
# so the pen ran down the wrong branch and came back (a visible jerk in the
# demo). Any count differing from the table gets a loud `!!` from the trace
# run; only grow it for a reversal you have verified on the debug sheet.
#
# The Tasmanian letterforms retrace *a lot* by design, but nearly all of those
# turns happen on or beside a junction (deg >= 3), which the check ignores.
# What is left here are the genuine mid-line pen touches:
# each verified on debug-sheet-lowercase.png, with the turn's position printed
# out of the router (the letter and the pixel it turns on are in the comment).
EXPECTED_REVERSALS = {
    # Clockwise family: the pen runs down to the foot of the stem and traces
    # straight back up it. The foot is a free tip, but the sharpest pixel of
    # the turn sits a pixel or two short of it, i.e. on plain degree-2 ink.
    "b": 1,  # (0.05,0.03) stem foot
    # h's foot turn grew a detectable wedge branch in the current reference
    # extraction, so the pen excurses into it (not a bare reversal) — 0.
    "n": 1,  # (0.05,0.05) stem foot
    "p": 1,  # (0.04,0.03) foot of the descender
    # k: the loop-closing touch at (0.22,0.26) sits in the stem junction's
    # crotch zone, which the chain-structured emitter treats as a branch
    # departure, not a pen reversal — only the stem foot counts now.
    "k": 1,  # (0.05,0.03) stem foot
    # Anticlockwise family: the closure-top cusp — the pen closes the bowl,
    # touches the top of the closing side and runs straight back down the
    # same ink. On d and q that cusp sits on plain degree-2 ink, so it
    # registers here; on a and g the same cusp lands on the junction knot
    # (degree >= 3) and is not counted.
    "d": 1,  # (0.63,0.51) closure top
    "q": 1,  # (0.92,0.90) closure top
    # Wave family: u rises to x-height on the right and comes straight back
    # down the same ink — the one reversal the chart draws as two arrows.
    "u": 1,  # (0.95,0.96) top right
    # Diagonal family: z's corners are points, and the pen genuinely stops
    # dead and reverses on the top-right one (the bottom-left corner reads as
    # a wedge instead, so it is not counted).
    "z": 1,  # (0.96,0.95) top-right corner
}
