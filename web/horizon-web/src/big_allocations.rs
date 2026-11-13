//! The page's allocator: Rust's own (dlmalloc), reporting on the console each
//! allocation of [`BIG`] bytes or more, and each time the game's memory grows
//! past another [`STEP`]. A page that asks for too much memory is closed by the
//! browser without a word, so the last such line says why; and a page's memory
//! never shrinks, so the lines say what made it as large as it is.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 64 MiB: the size from which an allocation is reported.
const BIG: usize = 64 << 20;
/// The memory's growth reported: once past each further 64 MiB.
const STEP: usize = 64 << 20;
/// The memory size up to which growth was reported.
static REPORTED: AtomicUsize = AtomicUsize::new(0);

struct Reporting;

#[global_allocator]
static ALLOCATOR: Reporting = Reporting;

// SAFETY: every call goes to `System` unchanged; reporting allocates nothing.
unsafe impl GlobalAlloc for Reporting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        report(b"Horizon: allocating          MiB", layout.size(), BIG);
        let ptr = unsafe { System.alloc(layout) };
        grown();
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        report(b"Horizon: allocating          MiB", layout.size(), BIG);
        let ptr = unsafe { System.alloc_zeroed(layout) };
        grown();
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        report(b"Horizon: allocating          MiB", new_size, BIG);
        let ptr = unsafe { System.realloc(ptr, layout, new_size) };
        grown();
        ptr
    }
}

/// The memory, once it grew past the next [`STEP`]: "Horizon: memory now N MiB".
fn grown() {
    #[cfg(target_arch = "wasm32")]
    {
        let size = core::arch::wasm32::memory_size(0) * 65536;
        let reported = REPORTED.load(Ordering::Relaxed);
        if size >= reported + STEP {
            REPORTED.store(size - size % STEP, Ordering::Relaxed);
            report(b"Horizon: memory now          MiB", size, 0);
        }
    }
}

/// Writes `text` with `size` in MiB where its spaces end, without allocating,
/// when `size` is at least `from`.
fn report(text: &[u8; 32], size: usize, from: usize) {
    if size < from {
        return;
    }
    let mut text = *text;
    let mut mib = size >> 20;
    let mut at = 27;
    loop {
        text[at] = b'0' + (mib % 10) as u8;
        mib /= 10;
        if mib == 0 || text[at - 1] != b' ' {
            break;
        }
        at -= 1;
    }
    let text = std::str::from_utf8(&text).unwrap_or("Horizon: allocating a lot");
    // As an Error: the console shows where in the game it was asked for.
    web_sys::console::warn_1(&js_sys::Error::new(text));
}
