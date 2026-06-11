//! fountouki-core — all pure game logic, data, and protocol. NO rendering and
//! NO macroquad dependency, so it compiles fast and is trivially unit-testable
//! (`cargo test -p fountouki-core`). The macroquad `app` crate consumes this.
//!
//! Each module is transcribed from docs/port-spec/ and the original TS app;
//! constants and JSON key names are load-bearing for save-compat + sync interop.
pub mod rng;
pub mod deck;
pub mod srs;
pub mod patterns;
pub mod themes;
pub mod audio;
pub mod storage;
pub mod settings;
pub mod sync;
pub mod route;
pub mod tracing;
mod tracing_data;
