//! Horizon in a browser page. `web/build.py` builds this crate for wasm32 and
//! packs it, with its JavaScript, into one HTML file that boots on its own or
//! inside an Eaglercraft-style runner.
#![forbid(unsafe_code)]

/// The page's entry point: runs once the module is instantiated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("Horizon panicked: {info}").into());
    }));
    horizon_client::entry::main();
}
