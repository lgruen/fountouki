// Soft-keyboard bridge for the WASM build.
//
// macroquad renders to a <canvas>. A canvas can't raise a mobile on-screen
// keyboard, and on Android it receives no character events at all — so the
// parent-menu token/endpoint fields (drawn by Rust) were dead on touch devices.
//
// We keep one hidden, focusable <input>. While a field is focused in-app the
// wasm calls fountouki_kb_focus() to focus it (which raises the keyboard) and
// reads fountouki_kb_value() back each frame; tapping away calls
// fountouki_kb_blur(). The <input> is the text source on web; native desktop
// keeps using macroquad's physical-keyboard path.
//
// Registered as a miniquad plugin so its imports land in the same wasm import
// object as sapp_jsutils, whose js_object()/js_objects[] string registry we
// reuse (load order in index.html puts sapp_jsutils first).
(function () {
  var input = null;

  function ensure() {
    if (input) return input;
    input = document.createElement("input");
    input.type = "text";
    input.autocomplete = "off";
    input.autocapitalize = "none";
    input.setAttribute("autocorrect", "off");
    input.spellcheck = false;
    input.setAttribute("aria-hidden", "true");
    input.tabIndex = -1;
    // Focusable but invisible. NOT display:none / visibility:hidden — those stop
    // focus() from raising the keyboard. A 1px, fully-transparent box pinned to a
    // corner is focusable yet leaves no visible artifact. font-size 16px keeps
    // iOS from zoom-on-focus even though it's never seen.
    var s = input.style;
    s.position = "fixed";
    s.top = "0";
    s.left = "0";
    s.width = "1px";
    s.height = "1px";
    s.padding = "0";
    s.margin = "0";
    s.border = "0";
    s.outline = "none";
    s.fontSize = "16px";
    s.background = "transparent";
    s.color = "transparent";
    s.caretColor = "transparent";
    document.body.appendChild(input);
    return input;
  }

  function register_plugin(importObject) {
    // value: a sapp_jsutils string-object id (the field's current text);
    // mode: 0 = text (token), 1 = url (endpoint).
    importObject.env.fountouki_kb_focus = function (value, mode) {
      var el = ensure();
      el.inputMode = mode === 1 ? "url" : "text";
      el.value = js_objects[value] || "";
      el.focus({ preventScroll: true });
      // Caret to the end so typing appends.
      try {
        var n = el.value.length;
        el.setSelectionRange(n, n);
      } catch (e) {}
    };
    importObject.env.fountouki_kb_value = function () {
      return js_object(input ? input.value : "");
    };
    importObject.env.fountouki_kb_blur = function () {
      if (input) {
        input.value = "";
        input.blur();
      }
    };
  }

  miniquad_add_plugin({
    register_plugin: register_plugin,
    version: "1.0.0",
    name: "fountouki_text_input",
  });
})();
