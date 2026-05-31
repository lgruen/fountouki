//! Trimmed in-tree fork of quad-net 0.1.2.
//!
//! We only use `quad_net::http_request`. Upstream also ships `web_socket`,
//! `quad_socket` and the `error` module; on native those pull in
//! `qws -> url 1.7.2 -> idna 0.1.5` (RUSTSEC-2024-0421), which has no fix on
//! the 0.1 line. Dropping the unused modules removes that whole chain from the
//! lockfile. `http_request.rs` is byte-identical to upstream; it depends only
//! on `ureq` (native) / `sapp-jsutils` (wasm) / std.

pub mod http_request;

// Kept identical to upstream so the wasm `quad_net` JS plugin's version
// handshake (`init_plugins` -> `wasm_exports.quad_net_crate_version()`) still
// matches. Without it the bundle logs "present in JS bundle, but is not used".
#[no_mangle]
pub extern "C" fn quad_net_crate_version() -> u32 {
    1
}
