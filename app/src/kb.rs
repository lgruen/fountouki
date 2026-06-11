//! Soft-keyboard bridge for the parent-menu text fields.
//!
//! macroquad renders to a `<canvas>`, which can't raise a mobile on-screen
//! keyboard and (on Android) delivers no character events at all — so the
//! token/endpoint fields were unusable on touch devices. `web/text_input.js`
//! overlays a hidden, focusable `<input>`; while a field is focused the wasm
//! reads [`value`] back each frame.
//!
//! Raising the keyboard is the subtle part: **iOS only shows the soft keyboard
//! when `input.focus()` runs synchronously inside a user-gesture handler**.
//! macroquad processes a tap one frame *after* the touch, so [`focus`] (called
//! from the wasm frame) is too late on iOS — it works on Android/desktop but
//! the keyboard never appears on iPad. The cure: each frame the panel publishes
//! its focusable field rects via [`set_fields`]; `text_input.js` hit-tests the
//! touch inside its *own* DOM listener and focuses the input there, in-gesture.
//!
//! On native desktop the physical keyboard reaches macroquad directly, so these
//! are no-ops there and the caller keeps using `get_char_pressed`.

/// Which on-screen keyboard layout to request for a field.
#[derive(Clone, Copy)]
pub enum Mode {
    /// Plain text (the alphanumeric sync token).
    Text,
    /// URL keyboard (the sync endpoint).
    Url,
}

/// A focusable text field's on-screen rect (in the wasm's screen-coordinate
/// space, the same one `layout` uses) plus the keyboard layout it wants. Pushed
/// to the web bridge so its touch handler can raise the right keyboard.
pub struct Field {
    pub rect: (f32, f32, f32, f32),
    pub mode: Mode,
}

fn mode_code(mode: Mode) -> i32 {
    match mode {
        Mode::Text => 0,
        Mode::Url => 1,
    }
}

/// Serialize the focusable-field layout to the JSON the web bridge parses:
/// `{"sw":..,"sh":..,"view":[x,y,w,h],"fields":[[x,y,w,h,mode],..]}`. All
/// numbers (no strings), so it needs no escaping. Kept out of the wasm-only
/// module — and unit-tested — because a malformed spec fails silently in JS.
fn fields_spec_json(screen: (f32, f32), view: (f32, f32, f32, f32), fields: &[Field]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(96);
    let _ = write!(
        s,
        "{{\"sw\":{},\"sh\":{},\"view\":[{},{},{},{}],\"fields\":[",
        screen.0, screen.1, view.0, view.1, view.2, view.3
    );
    for (i, f) in fields.iter().enumerate() {
        let sep = if i == 0 { "" } else { "," };
        let _ = write!(
            s,
            "{sep}[{},{},{},{},{}]",
            f.rect.0, f.rect.1, f.rect.2, f.rect.3, mode_code(f.mode)
        );
    }
    s.push_str("]}");
    s
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{mode_code, Field, Mode};
    use sapp_jsutils::{JsObject, JsObjectWeak};

    extern "C" {
        fn fountouki_kb_focus(value: JsObjectWeak, mode: i32);
        fn fountouki_kb_value() -> JsObject;
        fn fountouki_kb_blur();
        fn fountouki_kb_set_fields(spec: JsObjectWeak);
    }

    /// Seed the hidden input with `value` and focus it. On Android/desktop this
    /// also raises the keyboard; on iOS the in-gesture focus in `text_input.js`
    /// does that, and this just keeps the input's value/caret in sync.
    pub fn focus(value: &str, mode: Mode) {
        let obj = JsObject::string(value);
        unsafe { fountouki_kb_focus(obj.weak(), mode_code(mode)) };
    }

    /// The hidden input's current text (unsanitized).
    pub fn value() -> String {
        let mut s = String::new();
        unsafe { fountouki_kb_value() }.to_string(&mut s);
        s
    }

    /// Blur the hidden input, dismissing the keyboard and disarming the bridge's
    /// touch handler (so taps on the underlying scene don't re-raise it).
    pub fn blur() {
        unsafe { fountouki_kb_blur() };
    }

    /// Publish the currently-focusable field rects so the bridge's own touch
    /// handler can raise the soft keyboard in-gesture (the iOS requirement).
    /// `screen` is the wasm viewport size and `view` the visible scroll
    /// viewport (taps outside it are ignored, matching the in-app hit-test).
    pub fn set_fields(screen: (f32, f32), view: (f32, f32, f32, f32), fields: &[Field]) {
        let obj = JsObject::string(&super::fields_spec_json(screen, view, fields));
        unsafe { fountouki_kb_set_fields(obj.weak()) };
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{Field, Mode};
    pub fn focus(_value: &str, _mode: Mode) {}
    pub fn value() -> String {
        String::new()
    }
    pub fn blur() {}
    pub fn set_fields(_screen: (f32, f32), _view: (f32, f32, f32, f32), _fields: &[Field]) {}
}

// `value` is only polled by the web build; native reads the physical keyboard.
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_imports))]
pub use imp::{blur, focus, set_fields, value};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_spec_json_is_well_formed() {
        let json = fields_spec_json(
            (1194.0, 834.0),
            (100.0, 50.0, 600.0, 700.0),
            &[
                Field { rect: (110.0, 200.0, 580.0, 46.0), mode: Mode::Text },
                Field { rect: (110.0, 320.0, 580.0, 46.0), mode: Mode::Url },
            ],
        );
        assert_eq!(
            json,
            "{\"sw\":1194,\"sh\":834,\"view\":[100,50,600,700],\
             \"fields\":[[110,200,580,46,0],[110,320,580,46,1]]}"
        );
    }

    #[test]
    fn fields_spec_json_handles_no_fields() {
        let json = fields_spec_json((10.0, 20.0), (0.0, 0.0, 10.0, 20.0), &[]);
        assert_eq!(json, "{\"sw\":10,\"sh\":20,\"view\":[0,0,10,20],\"fields\":[]}");
    }
}
