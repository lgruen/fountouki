// Soft-keyboard bridge for the WASM build.
//
// macroquad renders to a <canvas>. A canvas can't raise a mobile on-screen
// keyboard, and on Android it receives no character events at all — so the
// parent-menu token/endpoint fields (drawn by Rust) were dead on touch devices.
//
// We keep one hidden, focusable <input>. While a field is focused in-app the
// wasm reads fountouki_kb_value() back each frame; tapping away calls
// fountouki_kb_blur(). The <input> is the text source on web; native desktop
// keeps using macroquad's physical-keyboard path.
//
// Raising the keyboard, the iOS catch: WebKit only shows the soft keyboard when
// input.focus() runs *synchronously inside a user-gesture handler*. macroquad
// processes a tap one frame after the touch, so a focus() from the wasm frame
// (fountouki_kb_focus) is too late on iOS — Android/desktop are lenient and show
// it, iPad never did. So we don't rely on the wasm to raise it: each frame the
// panel publishes its focusable field rects (fountouki_kb_set_fields); we add
// our OWN touch listener, hit-test the tap against those rects, and focus the
// input right there, in-gesture. fountouki_kb_focus then only seeds value/caret.
//
// Registered as a miniquad plugin so its imports land in the same wasm import
// object as sapp_jsutils, whose js_object()/js_objects[] string registry we
// reuse (load order in index.html puts sapp_jsutils first).
(function () {
  var input = null;
  // Latest field layout pushed from the wasm: { sw, sh, view:[x,y,w,h],
  // fields:[[x,y,w,h,mode],...] } in the wasm's screen-coordinate space. null
  // when the parent panel is closed (nothing focusable → ignore all taps).
  var fields = null;
  // Touch/click start point (client px) for a small-travel "is this a tap?" gate.
  var startX = 0, startY = 0;

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

  // Map a client (CSS px) point to wasm screen space and return the keyboard
  // mode of the field it hits, or null. Scaling uses the wasm's own reported
  // size vs the canvas's CSS box, so it's independent of devicePixelRatio.
  function hitMode(clientX, clientY) {
    if (!fields) return null;
    var canvas = document.getElementById("glcanvas");
    if (!canvas) return null;
    var r = canvas.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return null;
    var x = ((clientX - r.left) / r.width) * fields.sw;
    var y = ((clientY - r.top) / r.height) * fields.sh;
    var v = fields.view;
    // Outside the visible scroll viewport: ignore (matches the in-app hit-test).
    if (x < v[0] || x > v[0] + v[2] || y < v[1] || y > v[1] + v[3]) return null;
    for (var i = 0; i < fields.fields.length; i++) {
      var f = fields.fields[i];
      if (x >= f[0] && x <= f[0] + f[2] && y >= f[1] && y <= f[1] + f[3]) return f[4];
    }
    return null;
  }

  function point(e) {
    return e.changedTouches ? e.changedTouches[0] : e;
  }

  function onStart(e) {
    var p = point(e);
    startX = p.clientX;
    startY = p.clientY;
  }

  // The in-gesture focus. Runs in capture phase so it lands before macroquad's
  // own canvas handlers (which would otherwise pull focus back to the canvas).
  function onEnd(e) {
    if (!fields) return;
    var p = point(e);
    // Only a stationary tap focuses — a drag (panel scroll) must not.
    if (Math.abs(p.clientX - startX) > 16 || Math.abs(p.clientY - startY) > 16) return;
    var mode = hitMode(p.clientX, p.clientY);
    if (mode === null) return;
    var el = ensure();
    el.inputMode = mode === 1 ? "url" : "text";
    el.focus({ preventScroll: true });
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
      fields = null;
      if (input) {
        input.value = "";
        input.blur();
      }
    };
    // spec: a sapp_jsutils string-object id holding the JSON field layout.
    importObject.env.fountouki_kb_set_fields = function (spec) {
      try {
        fields = JSON.parse(js_objects[spec] || "null");
      } catch (e) {
        fields = null;
      }
    };

    // One set of capture-phase listeners on window, so we see the gesture before
    // macroquad and can focus the input within it (the iOS keyboard requirement).
    window.addEventListener("touchstart", onStart, true);
    window.addEventListener("touchend", onEnd, true);
    window.addEventListener("mousedown", onStart, true);
    window.addEventListener("mouseup", onEnd, true);
  }

  miniquad_add_plugin({
    register_plugin: register_plugin,
    version: "1.0.0",
    name: "fountouki_text_input",
  });
})();
