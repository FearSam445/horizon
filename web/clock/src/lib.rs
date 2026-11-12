//! The wall clock as std's [`SystemTime`], on the desktop and in the browser
//! build (wasm32), where `SystemTime::now` panics: there it reads the page's
//! clock (`Date.now`). Use [`system_now`] wherever the game asks for the time
//! of day; the type stays std's, so file times compare with it as before.

use std::time::SystemTime;

/// `SystemTime::now()`, which also works in the browser build.
#[inline]
pub fn system_now() -> SystemTime {
    #[cfg(target_arch = "wasm32")]
    {
        use web_time::web::SystemTimeExt;
        web_time::SystemTime::now().to_std()
    }
    #[cfg(not(target_arch = "wasm32"))]
    SystemTime::now()
}
