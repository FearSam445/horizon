//! The game's file system.
//!
//! On the desktop this is `std::fs` itself (`pub use std::fs::*`), plus
//! [`PathExt`], whose methods call `Path`'s own. In the browser build (wasm32),
//! where `std::fs` fails every call, it is an in-memory file system with the
//! same API: folders and files the game writes (saves, options), and read-only
//! zip archives mounted at a folder (the player's game files), whose entries
//! are inflated only when they are opened.

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::*;

// The browser build's file system; the desktop compiles it for its tests only.
#[cfg(any(target_arch = "wasm32", test))]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod vfs;
#[cfg(target_arch = "wasm32")]
pub use vfs::*;

use std::io;
use std::path::{Path, PathBuf};

/// `Path`'s methods that read the file system, through this crate's file
/// system: `path.fs_exists()` for `path.exists()`, and so on.
pub trait PathExt {
    fn fs_exists(&self) -> bool;
    fn fs_try_exists(&self) -> io::Result<bool>;
    fn fs_is_file(&self) -> bool;
    fn fs_is_dir(&self) -> bool;
    fn fs_is_symlink(&self) -> bool;
    fn fs_metadata(&self) -> io::Result<Metadata>;
    fn fs_symlink_metadata(&self) -> io::Result<Metadata>;
    fn fs_canonicalize(&self) -> io::Result<PathBuf>;
    fn fs_read_dir(&self) -> io::Result<ReadDir>;
}

#[cfg(not(target_arch = "wasm32"))]
impl PathExt for Path {
    #[inline]
    fn fs_exists(&self) -> bool {
        self.exists()
    }
    #[inline]
    fn fs_try_exists(&self) -> io::Result<bool> {
        self.try_exists()
    }
    #[inline]
    fn fs_is_file(&self) -> bool {
        self.is_file()
    }
    #[inline]
    fn fs_is_dir(&self) -> bool {
        self.is_dir()
    }
    #[inline]
    fn fs_is_symlink(&self) -> bool {
        self.is_symlink()
    }
    #[inline]
    fn fs_metadata(&self) -> io::Result<Metadata> {
        self.metadata()
    }
    #[inline]
    fn fs_symlink_metadata(&self) -> io::Result<Metadata> {
        self.symlink_metadata()
    }
    #[inline]
    fn fs_canonicalize(&self) -> io::Result<PathBuf> {
        self.canonicalize()
    }
    #[inline]
    fn fs_read_dir(&self) -> io::Result<ReadDir> {
        self.read_dir()
    }
}

#[cfg(target_arch = "wasm32")]
impl PathExt for Path {
    fn fs_exists(&self) -> bool {
        metadata(self).is_ok()
    }
    fn fs_try_exists(&self) -> io::Result<bool> {
        exists(self)
    }
    fn fs_is_file(&self) -> bool {
        metadata(self).is_ok_and(|m| m.is_file())
    }
    fn fs_is_dir(&self) -> bool {
        metadata(self).is_ok_and(|m| m.is_dir())
    }
    fn fs_is_symlink(&self) -> bool {
        false
    }
    fn fs_metadata(&self) -> io::Result<Metadata> {
        metadata(self)
    }
    fn fs_symlink_metadata(&self) -> io::Result<Metadata> {
        metadata(self)
    }
    fn fs_canonicalize(&self) -> io::Result<PathBuf> {
        canonicalize(self)
    }
    fn fs_read_dir(&self) -> io::Result<ReadDir> {
        read_dir(self)
    }
}

/// [`PathExt`]'s kind checks on what already knows its kind, so a rewritten
/// `.fs_is_dir()` reads the same on a path, a `Metadata` or a `FileType`.
pub trait KindExt {
    fn fs_is_file(&self) -> bool;
    fn fs_is_dir(&self) -> bool;
    fn fs_is_symlink(&self) -> bool;
}

impl KindExt for Metadata {
    #[inline]
    fn fs_is_file(&self) -> bool {
        self.is_file()
    }
    #[inline]
    fn fs_is_dir(&self) -> bool {
        self.is_dir()
    }
    #[inline]
    fn fs_is_symlink(&self) -> bool {
        self.is_symlink()
    }
}

impl KindExt for FileType {
    #[inline]
    fn fs_is_file(&self) -> bool {
        self.is_file()
    }
    #[inline]
    fn fs_is_dir(&self) -> bool {
        self.is_dir()
    }
    #[inline]
    fn fs_is_symlink(&self) -> bool {
        self.is_symlink()
    }
}

/// [`PathExt::fs_metadata`] on an open file or a folder entry.
pub trait MetaExt {
    fn fs_metadata(&self) -> io::Result<Metadata>;
}

impl MetaExt for File {
    #[inline]
    fn fs_metadata(&self) -> io::Result<Metadata> {
        self.metadata()
    }
}

impl MetaExt for DirEntry {
    #[inline]
    fn fs_metadata(&self) -> io::Result<Metadata> {
        self.metadata()
    }
}

/// `std::env::temp_dir()`: `/tmp` in the browser build, where std's panics.
pub fn temp_dir() -> PathBuf {
    #[cfg(target_arch = "wasm32")]
    {
        let dir = PathBuf::from("/tmp");
        let _ = create_dir_all(&dir);
        dir
    }
    #[cfg(not(target_arch = "wasm32"))]
    std::env::temp_dir()
}

/// `std::env::current_dir()`: `/` in the browser build.
pub fn current_dir() -> io::Result<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return Ok(PathBuf::from("/"));
    #[cfg(not(target_arch = "wasm32"))]
    std::env::current_dir()
}

/// `std::env::set_current_dir()`: the browser build has only `/`.
pub fn set_current_dir(path: impl AsRef<Path>) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return metadata(path).map(|_| ());
    #[cfg(not(target_arch = "wasm32"))]
    std::env::set_current_dir(path)
}

/// `std::env::current_exe()`: `/horizon` in the browser build.
pub fn current_exe() -> io::Result<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return Ok(PathBuf::from("/horizon"));
    #[cfg(not(target_arch = "wasm32"))]
    std::env::current_exe()
}

/// `std::process::id()`, which names temporary files and seeds a few defaults:
/// 1 in the browser build, where std's panics (a page has one process).
pub fn process_id() -> u32 {
    #[cfg(target_arch = "wasm32")]
    return 1;
    #[cfg(not(target_arch = "wasm32"))]
    std::process::id()
}
