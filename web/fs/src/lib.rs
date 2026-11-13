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

/// Whether a file can be read now ([`fetch`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// It is there.
    Here,
    /// The page was asked for it: ask again later.
    Waiting,
    /// There is no such file.
    Missing,
}

/// Whether `path` can be read now. On the desktop a file is there or not. The
/// browser build's page holds some of the player's files back until the game
/// first needs one (the sounds: hundreds of megabytes it would keep in memory,
/// each named first with [`withhold`]): the first call asks the page for it
/// ([`take_requests`]), and it is [`Fetch::Waiting`] until the page hands it
/// over or says it has none ([`not_found`]).
pub fn fetch(path: &Path) -> Fetch {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if path.is_file() {
            Fetch::Here
        } else {
            Fetch::Missing
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
        if metadata(path).is_ok_and(|m| m.is_file()) {
            // Handed over: once let go again (`remove_file`), it is asked for again.
            asked.state.remove(path);
            return Fetch::Here;
        }
        if !asked.withheld.contains(path) {
            return Fetch::Missing;
        }
        match asked.state.get(path) {
            Some(false) => Fetch::Missing,
            Some(true) => Fetch::Waiting,
            None => {
                asked.state.insert(path.to_owned(), true);
                asked.queue.push(path.to_owned());
                Fetch::Waiting
            }
        }
    }
}

/// What the game asked the page for ([`fetch`]) and the page has not answered.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct Asked {
    /// true while waiting, false once the page said it has no such file.
    state: std::collections::HashMap<PathBuf, bool>,
    /// Not yet handed to the page.
    queue: Vec<PathBuf>,
    /// The files the page holds back ([`withhold`]).
    withheld: std::collections::BTreeSet<PathBuf>,
}
#[cfg(target_arch = "wasm32")]
static ASKED: std::sync::LazyLock<std::sync::Mutex<Asked>> =
    std::sync::LazyLock::new(Default::default);

/// The files [`fetch`] asked for since the last call, for the page to look up.
#[cfg(target_arch = "wasm32")]
pub fn take_requests() -> Vec<PathBuf> {
    std::mem::take(&mut ASKED.lock().unwrap_or_else(|e| e.into_inner()).queue)
}

/// The page holds the file at `path` back until it is asked for ([`fetch`]).
#[cfg(target_arch = "wasm32")]
pub fn withhold(path: &Path) {
    ASKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .withheld
        .insert(path.to_owned());
}

/// The files under `folder` that the page holds back ([`withhold`]): they are
/// there for the game, though no folder listing shows them. None on the desktop.
pub fn withheld_under(folder: &Path) -> Vec<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = folder;
        Vec::new()
    }
    #[cfg(target_arch = "wasm32")]
    {
        ASKED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .withheld
            .range(folder.to_owned()..)
            .take_while(|p| p.starts_with(folder))
            .cloned()
            .collect()
    }
}

/// The page has no file at `path`: [`fetch`] says [`Fetch::Missing`] from now on.
#[cfg(target_arch = "wasm32")]
pub fn not_found(path: &Path) {
    ASKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .state
        .insert(path.to_owned(), false);
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
