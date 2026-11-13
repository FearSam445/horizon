//! The page's allocator: Rust's own (dlmalloc), reporting on the console each
//! allocation of [`BIG`] bytes or more. A page that asks for too much memory is
//! closed by the browser without a word, so the last such line says why.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};

/// 64 MiB: the size from which an allocation is reported.
const BIG: usize = 64 << 20;

struct Reporting;

#[global_allocator]
static ALLOCATOR: Reporting = Reporting;

// SAFETY: every call goes to `System` unchanged; reporting allocates nothing.
unsafe impl GlobalAlloc for Reporting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        report(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        report(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        report(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Writes "Horizon: allocating N MiB" without allocating.
fn report(size: usize) {
    if size < BIG {
        return;
    }
    let mut text = *b"Horizon: allocating          MiB";
    let mut mib = size >> 20;
    let mut at = 27;
    loop {
        text[at] = b'0' + (mib % 10) as u8;
        mib /= 10;
        if mib == 0 || at == 21 {
            break;
        }
        at -= 1;
    }
    let text = std::str::from_utf8(&text).unwrap_or("Horizon: allocating a lot");
    // As an Error: the console shows where in the game it was asked for.
    web_sys::console::warn_1(&js_sys::Error::new(text));
}
