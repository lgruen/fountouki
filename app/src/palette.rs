//! Color palette — transcribed from docs/port-spec/visual.md (CSS :root tokens).
//! The rainbow ramp is unified into ONE canonical 7-stop set (the visual spec's
//! "could refresh" win); pips reuse a 6-subset of it.
use macroquad::prelude::Color;

/// Build an opaque color from a 0xRRGGBB literal.
pub const fn hex(rgb: u32) -> Color {
    Color::new(
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        1.0,
    )
}
/// 0xRRGGBB + explicit alpha 0..1.
pub const fn hexa(rgb: u32, a: f32) -> Color {
    Color::new(
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        a,
    )
}

// Core tokens
pub const BG: Color = hex(0xfef6e4); // warm cream stage
pub const CARD: Color = hex(0xfffdf6); // off-white surface
pub const WHITE: Color = hex(0xffffff); // cell/choice shape fills
pub const INK: Color = hex(0x2b2c34); // primary text + glyphs
pub const MUTED: Color = hex(0x6f6e77); // secondary text, miss glyph
pub const ACCENT: Color = hex(0xf582ae); // brand pink "tap here"
pub const ACCENT_SOFT: Color = hex(0xffd6e6); // pale pink slot fill
pub const OK: Color = hex(0x8bd3a6); // success green button fill
pub const OK_STRONG: Color = hex(0x2b9d5f); // check glyph / star-pop flash
pub const BAD: Color = hex(0xf6b3a2); // wrong-answer salmon (never harsh red)
pub const GOLD: Color = hex(0xf6b800); // star glyph

// Soft drop shadow (CSS: 0 6px 16px rgba(43,44,52,0.10))
pub const SHADOW: Color = Color::new(43.0 / 255.0, 44.0 / 255.0, 52.0 / 255.0, 0.10);
pub const RADIUS: f32 = 18.0; // card/button corner radius (design units)

/// Canonical 7-stop rainbow (outer→inner = stop 0→6). Unifies the old
/// arc / pip / picker ramps into one ROYGBIV set.
pub const RAINBOW: [Color; 7] = [
    hex(0xff4d6d), // 0 red (rose-red)
    hex(0xff8c42), // 1 orange
    hex(0xffd166), // 2 yellow
    hex(0x2bd5a0), // 3 green
    hex(0x38b3e2), // 4 blue
    hex(0x6e72e7), // 5 indigo
    hex(0xb364e5), // 6 violet
];

/// Patterns level pips: 6-subset of the canonical ramp (drop indigo).
pub const PIPS: [Color; 6] = [
    RAINBOW[0], RAINBOW[1], RAINBOW[2], RAINBOW[3], RAINBOW[4], RAINBOW[6],
];
pub const PIP_EMPTY: Color = Color::new(0.0, 0.0, 0.0, 0.10);

// Mastery dots (parent panel)
pub const MASTERY: [Color; 5] = [
    Color::new(43.0 / 255.0, 44.0 / 255.0, 52.0 / 255.0, 0.10), // box-0 gray
    hex(0xffd6a8), // box-1
    hex(0xffb56e), // box-2
    hex(0x4adf99), // box-3
    hex(0xffd84f), // box-4 (gold; CSS uses a gradient, we use the bright stop)
];

// Done-scene gradient stops (sky / sun / ground / rain)
pub const SKY_TOP: Color = hex(0xcdefff);
pub const SKY_MID: Color = hex(0xe6f6ff);
pub const SKY_BOT: Color = hex(0xfff7d6);
pub const SUN_CORE: Color = hex(0xfff3a8);
pub const SUN_MID: Color = hex(0xffd76b);
pub const SUN_EDGE: Color = hex(0xffb347);
pub const GROUND_TOP: Color = hex(0x6fcf6f);
pub const GROUND_MID: Color = hex(0x4caf4c);
pub const GROUND_BOT: Color = hex(0x3a8c3a);
pub const RAIN: Color = hex(0x3aa8ee);

// Patterns finale — the "Pattern Train" dusk arrival (a deliberate golden-hour
// contrast to the phonics garden's cool high-noon).
pub const SKY_DUSK_TOP: Color = hex(0xff9d7e); // dusky rose top
pub const SKY_DUSK_MID: Color = hex(0xffd9a0); // amber middle (also cab glass)
pub const SKY_DUSK_BOT: Color = hex(0xfff2cf); // pale gold at the horizon
pub const HILL_FAR: Color = hex(0xb98ad0); // hazy violet far hills
pub const HILL_NEAR: Color = hex(0x7fae6e); // green near hill / ground band
pub const RAIL: Color = hex(0x8a7a6a); // track + sleepers + station posts
pub const LIGHT_GLOW: Color = hex(0xfff0b8); // warm string-light / headlamp glow
pub const ENGINE_RED: Color = hex(0xe85c6b); // cheerful kid-train red (boiler)
pub const ENGINE_RED_DARK: Color = hex(0xc24856); // boiler front face / shade
pub const STEAM: Color = hex(0xfff6ea); // warm-white steam puff
pub const CAR_BODY: Color = CARD; // cars are uniform cream — the items carry the pattern
// The cab driver is the shared frog mascot (see draw::frog), tinted RAINBOW[3];
// no separate critter palette.

// Form chrome (parent settings)
pub const FIELD_BORDER: Color = hex(0xe6e1d3);
pub const SCRIM: Color = Color::new(43.0 / 255.0, 44.0 / 255.0, 52.0 / 255.0, 0.72);

// Cell group tints (patterns unit grouping)
pub const GROUP_A_BG: Color = hex(0xfff4e6);
pub const GROUP_A_BORDER: Color = hex(0xffd9a8);
pub const GROUP_B_BG: Color = hex(0xe9f4ff);
pub const GROUP_B_BORDER: Color = hex(0xb4d6ff);
