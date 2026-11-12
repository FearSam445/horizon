//! fs4's disk-space queries for the browser build, where there is no disk to
//! ask: both answer `Unsupported`, which the game reads as "unknown".

use std::io;
use std::path::Path;

fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "no disk in the browser build")
}

/// The usable bytes of the disk holding `path`: unknown here.
pub fn available_space(_path: impl AsRef<Path>) -> io::Result<u64> {
    Err(unsupported())
}
/// The free bytes of the disk holding `path`: unknown here.
pub fn free_space(_path: impl AsRef<Path>) -> io::Result<u64> {
    Err(unsupported())
}
/// The size of the disk holding `path`: unknown here.
pub fn total_space(_path: impl AsRef<Path>) -> io::Result<u64> {
    Err(unsupported())
}
