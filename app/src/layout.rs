//! Layout = the cross-platform cure. We compute every region ourselves from the
//! viewport size + device safe-area insets + form factor, instead of handing
//! layout to a CSS engine. Identical math → identical placement on every device.
//!
//! Sizing mirrors the old CSS `clamp(min, <vw/vh>, max)` ranges from
//! docs/port-spec/visual.md so the feel matches per form factor.
use macroquad::prelude::*;

#[derive(Clone, Copy, Default)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    TabletLandscape, // primary platform
    TabletPortrait,
    PhoneLandscape,
    PhonePortrait, // rotate-gate
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub w: f32,
    pub h: f32,
    pub safe: Insets,
    pub form: Form,
}

impl Frame {
    pub fn new(w: f32, h: f32, safe: Insets) -> Self {
        Frame { w, h, safe, form: detect(w, h) }
    }

    pub fn vw(&self, frac: f32) -> f32 {
        self.w * frac
    }
    pub fn vh(&self, frac: f32) -> f32 {
        self.h * frac
    }
    pub fn vmin(&self, frac: f32) -> f32 {
        self.w.min(self.h) * frac
    }
    /// CSS `clamp(min, w*frac, max)`.
    pub fn clampw(&self, min: f32, frac: f32, max: f32) -> f32 {
        (self.w * frac).clamp(min, max)
    }
    /// CSS `clamp(min, h*frac, max)`.
    pub fn clamph(&self, min: f32, frac: f32, max: f32) -> f32 {
        (self.h * frac).clamp(min, max)
    }

    pub fn is_portrait(&self) -> bool {
        matches!(self.form, Form::TabletPortrait | Form::PhonePortrait)
    }
    pub fn is_phone(&self) -> bool {
        matches!(self.form, Form::PhoneLandscape | Form::PhonePortrait)
    }
    /// Phone held in portrait → show the "turn sideways" wall instead of gameplay.
    pub fn is_rotate_gate(&self) -> bool {
        self.form == Form::PhonePortrait
    }

    pub fn center(&self) -> Vec2 {
        vec2(self.w / 2.0, self.h / 2.0)
    }

    /// Base edge padding, safe-area aware (CSS: max(clamp(14,3vh,32), safe)).
    fn pad(&self) -> Insets {
        let py = self.clamph(if self.is_phone() { 6.0 } else { 14.0 }, 0.03, 32.0);
        let px = self.clampw(14.0, 0.03, 32.0);
        Insets {
            top: py.max(self.safe.top),
            right: px.max(self.safe.right),
            bottom: py.max(self.safe.bottom),
            left: px.max(self.safe.left),
        }
    }

    /// Full content box inside the safe-area padding.
    pub fn content(&self) -> Rect {
        let p = self.pad();
        Rect::new(p.left, p.top, self.w - p.left - p.right, self.h - p.top - p.bottom)
    }

    /// Topbar strip: floats at the top within the safe inset (so the play-area
    /// can center against the FULL viewport, per the visual spec).
    pub fn topbar(&self) -> Rect {
        let p = self.pad();
        let h = self.clamph(44.0, 0.07, 64.0);
        Rect::new(p.left, p.top, self.w - p.left - p.right, h)
    }

    /// Icon-button diameter (home/mute): CSS clamp(44, 5.2vw, 56).
    pub fn icon_btn(&self) -> f32 {
        self.clampw(44.0, 0.052, 56.0)
    }
}

fn detect(w: f32, h: f32) -> Form {
    let portrait = h > w;
    let min_dim = w.min(h);
    let phone = min_dim < 600.0;
    match (portrait, phone) {
        (true, true) => Form::PhonePortrait,
        (true, false) => Form::TabletPortrait,
        (false, true) => Form::PhoneLandscape,
        (false, false) => Form::TabletLandscape,
    }
}
