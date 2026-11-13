//! Horizon in a browser page. `web/build.py` builds this crate for wasm32 and
//! packs it, with its JavaScript, into one HTML file that boots on its own or
//! inside an Eaglercraft-style runner.
//!
//! The page reads the player's prepared game files (a zip of the `26.3` folder
//! the launcher or `horizon-prepare` makes) and hands over the files the game
//! uses one at a time with [`game_file`], then calls [`boot`].
#![deny(unsafe_code)]
// Only the page runs it: on the desktop the crate is empty.
#![cfg(target_arch = "wasm32")]

mod big_allocations;
mod webgl_upload;

use std::path::Path;

/// Where the game looks for prepared files: `resources/` beside its program,
/// which is `/horizon` in the browser build.
pub const RESOURCES: &str = "/resources";

/// One of the player's prepared files, from the page's zip reader
/// (`page/zip_reader.js`): its path under the prepared folder and its bytes as
/// the zip packs them. It stays packed until the game opens it.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn game_file(path: &str, deflated: bool, length: f64, data: &[u8]) -> Result<(), String> {
    horizon_fs::add_packed(
        Path::new(RESOURCES).join(path),
        deflated,
        length as u64,
        data.to_vec(),
    )
    .map_err(|e| format!("{path}: {e}"))
}

/// The game files the game asked for since the last call (`horizon_fs::fetch`:
/// the sounds, which the page reads from the zip only when one first plays),
/// as paths under the prepared folder. One outside it can't be in the zip.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn wanted_files() -> Vec<String> {
    let mut wanted = Vec::new();
    for path in horizon_fs::take_requests() {
        match path.strip_prefix(RESOURCES) {
            Ok(inside) => wanted.push(inside.to_string_lossy().replace('\\', "/")),
            Err(_) => horizon_fs::not_found(&path),
        }
    }
    wanted
}

/// A file of the zip (`path` under the prepared folder) the page holds back
/// until the game asks for it (`wanted_files`): the sounds. Named before [`boot`].
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn game_file_later(path: &str) {
    horizon_fs::withhold(&Path::new(RESOURCES).join(path));
}

/// The zip has no file at `path` (under the prepared folder), which the game
/// asked for: it stops waiting for it.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn game_file_missing(path: &str) {
    horizon_fs::not_found(&Path::new(RESOURCES).join(path));
}

/// Turns the frame's shared work budget on or off, for measuring
/// (`horizon_client::web_frame`): `wasm_bindgen.set_frame_budget(false)`.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn set_frame_budget(on: bool) {
    horizon_client::web_frame::set_shared_budget(on);
}

/// Folders the page doesn't keep between visits: the player's game files (they
/// come from the zip each time), the zip's mount, temporary files, and the logs
/// and crash reports of the game's home (`logging::data_dir`, `/data` here).
const NOT_KEPT: &[&str] = &[
    RESOURCES,
    "/mnt",
    "/tmp",
    "/data/horizon/logs",
    "/data/horizon/crash-reports",
];

/// A number that moves when the game changes a file it made (a world's save,
/// the options): the page keeps the files again only when it moved.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn kept_changes() -> f64 {
    horizon_fs::changes() as f64
}

/// The files the game made (worlds, options), in one block the page keeps in
/// the browser's storage between visits.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn kept_files() -> Vec<u8> {
    horizon_fs::export(NOT_KEPT)
}

/// Restores [`kept_files`]' block from an earlier visit, before [`boot`].
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn restore_kept_files(block: &[u8]) -> Result<u32, String> {
    let files = horizon_fs::import(block).map_err(|e| e.to_string())?;
    log::info!("Kept files: {files} restored");
    Ok(files as u32)
}

/// Starts the game.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn boot() -> Result<(), String> {
    std::panic::set_hook(Box::new(|info| {
        // The page's stack names the Rust functions it went through: where it happened.
        let error = js_sys::Error::new("");
        let stack = js_sys::Reflect::get(&error, &"stack".into())
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_default();
        web_sys::console::error_1(&format!("Horizon panicked: {info}\n{stack}").into());
    }));
    webgl_upload::install();
    // A loop that sleeps until chunks are made makes them itself: no worker would.
    horizon_thread::set_idle(|budget| {
        horizon_threads::jobs::pump(budget);
    });
    horizon_client::entry::main();
    Ok(())
}
