//! Horizon in a browser page. `web/build.py` builds this crate for wasm32 and
//! packs it, with its JavaScript, into one HTML file that boots on its own or
//! inside an Eaglercraft-style runner.
//!
//! The page hands over the player's prepared game files (a zip of the `26.3`
//! folder the launcher or `horizon-prepare` makes) in pieces with
//! [`game_files_begin`] and [`game_files_chunk`], then calls [`boot`].
#![deny(unsafe_code)]
// Only the page runs it: on the desktop the crate is empty.
#![cfg(target_arch = "wasm32")]

mod big_allocations;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where the game looks for prepared files: `resources/` beside its program,
/// which is `/horizon` in the browser build.
pub const RESOURCES: &str = "/resources";

/// The zip being handed over.
static GAME_FILES: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Starts a hand-over of `len` bytes.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn game_files_begin(len: usize) {
    let mut files = GAME_FILES.lock().unwrap_or_else(|e| e.into_inner());
    *files = Vec::with_capacity(len);
}

/// The next piece of the zip.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn game_files_chunk(chunk: &[u8]) {
    GAME_FILES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend_from_slice(chunk);
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

/// Mounts the zip handed over, if any, then starts the game. An error comes
/// back to the page when the zip can't be read or holds no prepared files.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn boot() -> Result<(), String> {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("Horizon panicked: {info}").into());
    }));
    // A loop that sleeps until chunks are made makes them itself: no worker would.
    horizon_thread::set_idle(|budget| {
        horizon_threads::jobs::pump(budget);
    });
    let zip = std::mem::take(&mut *GAME_FILES.lock().unwrap_or_else(|e| e.into_inner()));
    if !zip.is_empty() {
        mount(zip)?;
    }
    horizon_client::entry::main();
    Ok(())
}

/// Mounts `zip` and moves the folder holding its `horizon-resources.json` to
/// [`RESOURCES`]: the zip may hold the `26.3` folder or its contents.
fn mount(zip: Vec<u8>) -> Result<(), String> {
    const AT: &str = "/mnt/game-files";
    let files =
        horizon_fs::mount_zip(AT, zip).map_err(|e| format!("the zip can't be read: {e}"))?;
    let root = find_resources(Path::new(AT), 3)
        .ok_or("the zip holds no horizon-resources.json: zip the prepared 26.3 folder itself")?;
    horizon_fs::rename(&root, RESOURCES).map_err(|e| e.to_string())?;
    log::info!("Game files: {files} files from {}", root.display());
    Ok(())
}

/// The folder at most `depth` levels under `dir` holding `horizon-resources.json`.
fn find_resources(dir: &Path, depth: u32) -> Option<PathBuf> {
    if horizon_fs::metadata(dir.join("horizon-resources.json")).is_ok() {
        return Some(dir.to_owned());
    }
    if depth == 0 {
        return None;
    }
    horizon_fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .find_map(|entry| find_resources(&entry.path(), depth - 1))
}
