//! The browser build's `mimalloc`: wasm32 has one thread and no C toolchain, so
//! the game uses Rust's built-in allocator (dlmalloc), which is small and keeps
//! the heap compact.

use std::alloc::{GlobalAlloc, Layout, System};

/// mimalloc's allocator type, backed by the system allocator.
#[derive(Clone, Copy, Debug, Default)]
pub struct MiMalloc;

// SAFETY: forwards every call unchanged to `System`.
unsafe impl GlobalAlloc for MiMalloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }
    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc_zeroed(layout) }
    }
    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
