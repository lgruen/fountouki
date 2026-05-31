//! Scene framework. A `Scene` updates (with dt + pointer) and draws; it returns
//! a `Nav` to drive the router. `Ctx` carries everything a scene needs per
//! frame. Time/dt are explicit so golden frames are deterministic.
use crate::{input::Pointer, layout::Frame, sound::Audio, text::Fonts};

pub struct Ctx<'a> {
    pub dt: f32,
    /// Seconds since the scene was mounted (deterministic in capture).
    pub time: f32,
    /// Wall-clock epoch milliseconds (for SRS due/last-seen). Fixed in capture.
    pub now: i64,
    pub pointer: &'a Pointer,
    pub frame: Frame,
    pub fonts: &'a Fonts,
    pub audio: &'a Audio,
}

/// What a scene asks the router to do after an update.
pub enum Nav {
    Stay,
    Home,
    Game(String),
    /// Long-press on ← : open the parent settings panel for the current scene.
    OpenParent,
}

pub trait Scene {
    fn update(&mut self, ctx: &Ctx) -> Nav;
    fn draw(&mut self, ctx: &Ctx);
}
