// Persistent key/value backend (localStorage) for the WASM build, bridged via
// sapp_jsutils. Without this the app's store is in-memory only, so nothing —
// sync token, mute, local progress — survives a reload.
//
// Values are always non-empty JSON, so an empty string back from get() means
// "absent" (localStorage.getItem returns null for a missing key). Reads/writes
// are best-effort: localStorage can throw (private mode, quota, disabled), and
// the store contract is to no-op rather than break gameplay.
//
// Registered as a miniquad plugin so its imports share the wasm import object
// with sapp_jsutils, whose js_object()/js_objects[] string registry it reuses
// (index.html loads sapp_jsutils first).
(function () {
  function register_plugin(importObject) {
    importObject.env.fountouki_ls_get = function (key) {
      var v = null;
      try {
        v = localStorage.getItem(js_objects[key]);
      } catch (e) {}
      return js_object(v === null ? "" : v);
    };
    importObject.env.fountouki_ls_set = function (key, val) {
      try {
        localStorage.setItem(js_objects[key], js_objects[val]);
      } catch (e) {}
    };
    importObject.env.fountouki_ls_remove = function (key) {
      try {
        localStorage.removeItem(js_objects[key]);
      } catch (e) {}
    };
  }

  miniquad_add_plugin({
    register_plugin: register_plugin,
    version: "1.0.0",
    name: "fountouki_storage",
  });
})();
