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

/// Starts work that ends on its own (a load, a world's preparation: nothing
/// that waits for others): on the desktop a thread from `builder`; in the
/// browser build, with one thread, it runs at once, and the handle keeps the
/// result. Where a thread may loop until told to stop, use `builder.spawn`,
/// which fails in the browser build, and fall back.
pub fn spawn_bounded<F, T>(builder: Builder, f: F) -> std::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        let _ = builder;
        Ok(spawn(f))
    }
    #[cfg(not(target_arch = "wasm32"))]
    builder.spawn(f)
}
