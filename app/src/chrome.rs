//! Shared in-game chrome: the topbar (← back / mute speaker) and the finale
//! corner buttons (replay / home). Both games use identical geometry, drawing
//! and input handling for these — keeping them here means a kid sees the same
//! controls in the same places in every game (predictable layout).
use crate::{
    draw, input,
    layout::Frame,
    palette,
    scene::Ctx,
    store::Db,
};
use macroquad::prelude::*;

/// Topbar hit/draw geometry: (center, radius) for the ← home button and the
/// mute speaker, derived from the frame like all layout.
pub struct Topbar {
    pub home: (Vec2, f32),
    pub mute: (Vec2, f32),
}

pub fn topbar(f: &Frame) -> Topbar {
    let tb = f.topbar();
    let ir = f.icon_btn() / 2.0;
    Topbar {
        home: (vec2(tb.x + ir, tb.y + ir), ir),
        mute: (vec2(tb.x + tb.w - ir, tb.y + ir), ir),
    }
}

/// What a topbar interaction asked for this frame.
pub enum TopbarAction {
    /// Tap on ← : leave to the picker (the caller may flush sync first).
    Home,
    /// Long-press on ← : open the parent settings panel.
    OpenParent,
    /// Tap on the speaker: mute was toggled (and persisted) — consume the tap.
    MuteToggled,
}

/// Handle topbar input: long-press ← opens the parent panel, tap ← goes home,
/// tap on the speaker toggles + persists the shared mute. Returns `None` when
/// the pointer didn't interact with the topbar this frame.
pub fn handle_topbar(tb: &Topbar, ctx: &Ctx, db: &Db) -> Option<TopbarAction> {
    let pt = ctx.pointer;
    if pt.long_fired && input::hit_circle(pt.pos, tb.home.0.x, tb.home.0.y, tb.home.1) {
        return Some(TopbarAction::OpenParent);
    }
    if !pt.tapped() {
        return None;
    }
    if input::hit_circle(pt.pos, tb.home.0.x, tb.home.0.y, tb.home.1) {
        return Some(TopbarAction::Home);
    }
    if input::hit_circle(pt.pos, tb.mute.0.x, tb.mute.0.y, tb.mute.1) {
        let muted = !ctx.audio.muted();
        ctx.audio.set_muted(muted);
        crate::store::persist_mute(db, muted);
        return Some(TopbarAction::MuteToggled);
    }
    None
}

/// Draw the topbar chrome (← back chevron + mute speaker).
pub fn draw_topbar(tb: &Topbar, ctx: &Ctx) {
    draw::circle_btn(tb.home.0.x, tb.home.0.y, tb.home.1, palette::CARD);
    draw::chevron_left(tb.home.0.x, tb.home.0.y, tb.home.1 * 0.9, palette::INK);
    draw::circle_btn(tb.mute.0.x, tb.mute.0.y, tb.mute.1, palette::CARD);
    draw::speaker(tb.mute.0.x, tb.mute.0.y, tb.mute.1 * 0.9, palette::INK, ctx.audio.muted());
}

/// Finale corner buttons: (replay_center, home_center, radius). Identical in
/// both games' celebration scenes so the kid always finds them in the corners.
pub fn corner_buttons(f: &Frame) -> (Vec2, Vec2, f32) {
    let br = f.icon_btn() / 2.0 * 1.2;
    let m = 30.0 + f.safe.bottom.max(0.0);
    let replay = vec2(f.safe.left + 30.0 + br, f.h - m - br);
    let home = vec2(f.w - (f.safe.right + 30.0 + br), f.h - m - br);
    (replay, home, br)
}

/// Draw the finale corner buttons (replay ↻ + home ⌂ on white discs).
pub fn draw_corner_buttons(replay: Vec2, home: Vec2, r: f32) {
    let white = Color::new(1.0, 1.0, 1.0, 0.94);
    draw::circle_btn(replay.x, replay.y, r, white);
    draw::replay_icon(replay.x, replay.y, r, palette::INK);
    draw::circle_btn(home.x, home.y, r, white);
    draw::house_icon(home.x, home.y, r, palette::INK);
}
