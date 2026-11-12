//! The game's threads.
//!
//! On the desktop this is `std::thread` itself. The browser build (wasm32) has
//! one thread: a page in a runner's sandbox gets no SharedArrayBuffer, so no
//! workers can share the game's memory. There a spawned closure runs at once,
//! where it is spawned, and its handle keeps the result for `join`. That keeps
//! fork-join work (loading steps, scoped helpers) correct; a thread that loops
//! until told to stop must instead become a step its owner calls (web/README.md).

#[cfg(not(target_arch = "wasm32"))]
pub use std::thread::*;

#[cfg(target_arch = "wasm32")]
mod inline;
#[cfg(target_arch = "wasm32")]
pub use inline::*;
