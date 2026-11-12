//! The game's threads.
//!
//! On the desktop this is `std::thread` itself. The browser build (wasm32) has
//! one thread: a page in a runner's sandbox gets no SharedArrayBuffer, so no
//! workers can share the game's memory. There `spawn` and scoped threads run
//! their closure at once, where they are spawned, and the handle keeps the
//! result for `join`: fork-join work (loading steps, scoped helpers) stays
//! correct. `Builder::spawn` fails, as std's does on wasm32, so the workers
//! that loop until told to stop take their callers' no-thread fallbacks; a
//! thread with none must become a step its owner calls (web/README.md).

#[cfg(not(target_arch = "wasm32"))]
pub use std::thread::*;

#[cfg(target_arch = "wasm32")]
mod inline;
#[cfg(target_arch = "wasm32")]
pub use inline::*;
