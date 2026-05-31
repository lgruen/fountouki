//! Soft-keyboard bridge for the parent-menu text fields.
//!
//! macroquad renders to a `<canvas>`, which can't raise a mobile on-screen
//! keyboard and (on Android) delivers no character events at all — so the
//! token/endpoint fields were unusable on touch devices. `web/text_input.js`
//! overlays a hidden, focusable `<input>`; while a field is focused the wasm
//! [`focus`]es it (raising the keyboard) and reads [`value`] back each frame.
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

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::Mode;
    use sapp_jsutils::{JsObject, JsObjectWeak};

    extern "C" {
        fn fountouki_kb_focus(value: JsObjectWeak, mode: i32);
        fn fountouki_kb_value() -> JsObject;
        fn fountouki_kb_blur();
    }

    /// Focus the hidden input, seeding it with `value` and raising the keyboard.
    pub fn focus(value: &str, mode: Mode) {
        let obj = JsObject::string(value);
        let m = match mode {
            Mode::Text => 0,
            Mode::Url => 1,
        };
        unsafe { fountouki_kb_focus(obj.weak(), m) };
    }

    /// The hidden input's current text (unsanitized).
    pub fn value() -> String {
        let mut s = String::new();
        unsafe { fountouki_kb_value() }.to_string(&mut s);
        s
    }

    /// Blur the hidden input, dismissing the keyboard.
    pub fn blur() {
        unsafe { fountouki_kb_blur() };
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::Mode;
    pub fn focus(_value: &str, _mode: Mode) {}
    pub fn value() -> String {
        String::new()
    }
    pub fn blur() {}
}

// `value` is only polled by the web build; native reads the physical keyboard.
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_imports))]
pub use imp::{blur, focus, value};
