//! Gameplay and startup, above the reusable client library.
// No `#![recursion_limit]` or other inner attribute horizon-mixins doesn't know here: the
// launcher's horizon-mixins then rejects every native mod targeting this crate (#8169).
#![forbid(unsafe_code)]
// The browser build's stand-in (web/shims).
#[cfg(target_arch = "wasm32")]
extern crate horizon_web_fs4 as fs4;

// The browser build's stand-in (web/shims).
#[cfg(target_arch = "wasm32")]
extern crate horizon_web_ureq as ureq;

extern crate self as horizon_client;
pub use horizon_client_lib::*;
pub mod entry;
pub mod game;
mod horizon_look;
pub mod mod_api;

#[cfg(test)]
mod brewing_speed_load_tests;
#[cfg(test)]
mod living_death_tests;
