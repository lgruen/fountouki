//! Reusable vector drawing — everything is drawn by us (no platform widgets)
//! so pixels are identical across targets. Split by subject:
//!
//! - [`prim`]: geometric primitives (rects, discs, ellipses, paths, gradients).
//! - [`glyphs`]: stroked UI marks (✓ ✗ →, chevron, speaker, replay, home) —
//!   centered on true geometric center; this deleted the old iOS glyph CSS debt.
//! - [`scenery`]: rainbow, sky, igloo and the phonics garden plants.
//! - [`frog`]: the rigged frog mascot + its pose type.
//! - [`train`]: the Pattern Train engine, cars, flag and bunting.
//!
//! Everything is re-exported flat so call sites stay `draw::frog(..)` etc.
mod frog;
mod glyphs;
mod prim;
mod scenery;
mod train;

pub use frog::{frog, FrogPose};
pub use glyphs::{
    chevron_left, circle_btn, house_icon, mark_arrow, mark_check, mark_cross, replay_icon, speaker,
};
pub use prim::{
    card, disc, fill_ellipse, pop_clip, push_clip, rounded_rect, star, stroke_path, vgradient,
};
pub use scenery::{
    cloud, garden_plant, grass_tuft, igloo, plant, rainbow, rainbow_ghost, sun, Plant,
    GARDEN_SPECIES,
};
pub use train::{
    bunting, checker_flag, engine_funnel_tip, engine_hit_rect, steam_puff, train_car_chassis,
    train_engine, EnginePose,
};
