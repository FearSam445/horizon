//! The browser build's file system: a tree in memory behind one lock.
//!
//! Paths are absolute from `/`; a relative path is taken from `/` (the page has
//! no working folder). Files the game writes live in memory. A zip archive
//! mounted with [`mount_zip`] lends its entries read-only: each is inflated
//! when it is opened and dropped with its handle, so the archive is held once,
//! compressed. Writing to an entry copies it into memory first.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::SystemTime;

type Bytes = Arc<Mutex<Vec<u8>>>;
type Archive = zip::ZipArchive<Cursor<Arc<[u8]>>>;

fn now() -> SystemTime {
    #[cfg(target_arch = "wasm32")]
    {
        use web_time::web::SystemTimeExt;
        web_time::SystemTime::now().to_std()
    }
    // The desktop compiles this module only for its tests.
    #[cfg(not(target_arch = "wasm32"))]
    SystemTime::now()
}

#[derive(Clone)]
enum Node {
    Dir(Dir),
    File {
        data: Bytes,
        modified: SystemTime,
    },
    Entry {
        archive: Arc<Archive>,
        index: usize,
        len: u64,
        modified: SystemTime,
    },
    /// A file handed over packed ([`add_packed`]): stored or deflated bytes,
    /// inflated when opened.
    Packed {
        data: Arc<[u8]>,
        deflated: bool,
        len: u64,
        modified: SystemTime,
    },
}

#[derive(Clone, Default)]
struct Dir {
    entries: BTreeMap<OsString, Node>,
}

impl Node {
    fn dir() -> Node {
        Node::Dir(Dir::default())
    }
    fn meta(&self) -> Metadata {
        match self {
            Node::Dir(_) => Metadata {
                kind: Kind::Dir,
                len: 0,
                modified: None,
            },
            Node::File { data, modified } => Metadata {
                kind: Kind::File,
                len: lock(data).len() as u64,
                modified: Some(*modified),
            },
            Node::Entry { len, modified, .. } | Node::Packed { len, modified, .. } => Metadata {
                kind: Kind::File,
                len: *len,
                modified: Some(*modified),
            },
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

static TREE: LazyLock<Mutex<Node>> = LazyLock::new(|| Mutex::new(Node::dir()));

/// Bumped by every change to what the game wrote: [`changes`].
static CHANGES: AtomicU64 = AtomicU64::new(0);

fn changed() {
    CHANGES.fetch_add(1, Ordering::Relaxed);
}

/// A number that moves whenever a file or folder the game made changes: the page
/// saves the game's files again only when it moved ([`export`]).
pub fn changes() -> u64 {
    CHANGES.load(Ordering::Relaxed)
}

/// `path`'s parts from the root, with `.` and `..` resolved.
fn parts(path: &Path) -> Vec<OsString> {
    let mut out: Vec<OsString> = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(name) => out.push(name.to_owned()),
        }
    }
    out
}

fn absolute(path: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    out.extend(parts(path));
    out
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{}: no such file or folder", path.display()),
    )
}

fn error(kind: io::ErrorKind, path: &Path, what: &str) -> io::Error {
    io::Error::new(kind, format!("{}: {what}", path.display()))
}

fn find<'a>(root: &'a Node, parts: &[OsString]) -> Option<&'a Node> {
    let mut node = root;
    for name in parts {
        match node {
            Node::Dir(dir) => node = dir.entries.get(name)?,
            _ => return None,
        }
    }
    Some(node)
}

fn find_mut<'a>(root: &'a mut Node, parts: &[OsString]) -> Option<&'a mut Node> {
    let mut node = root;
    for name in parts {
        match node {
            Node::Dir(dir) => node = dir.entries.get_mut(name)?,
            _ => return None,
        }
    }
    Some(node)
}

/// The folder holding `parts`'s last part, which must exist.
fn parent_mut<'a>(root: &'a mut Node, parts: &[OsString], path: &Path) -> io::Result<&'a mut Dir> {
    let (_, folder) = parts
        .split_last()
        .ok_or_else(|| error(io::ErrorKind::InvalidInput, path, "the root"))?;
    match find_mut(root, folder) {
        Some(Node::Dir(dir)) => Ok(dir),
        Some(_) => Err(error(
            io::ErrorKind::NotADirectory,
            path,
            "a parent is not a folder",
        )),
        None => Err(not_found(path)),
    }
}

/// Mounts the zip archive `bytes` read-only at the folder `at` (made if
/// missing): its entries appear as files under it. Returns the number of files.
pub fn mount_zip(at: impl AsRef<Path>, bytes: Vec<u8>) -> io::Result<usize> {
    let bytes: Arc<[u8]> = bytes.into();
    let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(io::Error::other)?;
    let archive = Arc::new(archive);
    let modified = now();
    create_dir_all(at.as_ref())?;
    let base = parts(at.as_ref());
    let mut tree = lock(&TREE);
    let mut files = 0;
    let mut reader = (*archive).clone();
    for index in 0..archive.len() {
        let (name, len, is_dir) = {
            let entry = reader.by_index_raw(index).map_err(io::Error::other)?;
            let name = entry.name().map_err(io::Error::other)?.into_owned();
            (name, entry.size(), entry.is_dir())
        };
        let mut path = base.clone();
        path.extend(parts(Path::new(&name)));
        if path.len() == base.len() {
            continue;
        }
        let (last, folders) = path.split_last().expect("at least one part");
        let mut node = &mut *tree;
        for folder in folders {
            let Node::Dir(dir) = node else {
                return Err(error(
                    io::ErrorKind::InvalidData,
                    Path::new(&name),
                    "a file is in the way",
                ));
            };
            node = dir.entries.entry(folder.clone()).or_insert_with(Node::dir);
        }
        let Node::Dir(dir) = node else {
            return Err(error(
                io::ErrorKind::InvalidData,
                Path::new(&name),
                "a file is in the way",
            ));
        };
        if is_dir {
            dir.entries.entry(last.clone()).or_insert_with(Node::dir);
        } else {
            files += 1;
            dir.entries.insert(
                last.clone(),
                Node::Entry {
                    archive: archive.clone(),
                    index,
                    len,
                    modified,
                },
            );
        }
    }
    Ok(files)
}

/// The bytes of a mounted zip entry.
fn inflate(archive: &Archive, index: usize, len: u64) -> io::Result<Vec<u8>> {
    let mut reader = archive.clone();
    let mut entry = reader.by_index(index).map_err(io::Error::other)?;
    let mut out = Vec::with_capacity(len as usize);
    entry.read_to_end(&mut out)?;
    Ok(out)
}

/// The bytes of a packed file.
fn unpack(data: &[u8], deflated: bool, len: u64) -> io::Result<Vec<u8>> {
    if !deflated {
        return Ok(data.to_vec());
    }
    // Into a buffer of the file's own length: growing one as it fills would hold
    // the old and the new buffer at once (a 77 MB file took 154 MB), and a page's
    // memory never shrinks back.
    let mut out = vec![0; len as usize];
    let written = miniz_oxide::inflate::decompress_slice_iter_to_slice(
        &mut out,
        std::iter::once(data),
        false,
        true,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;
    if written as u64 != len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a packed file's length is wrong",
        ));
    }
    Ok(out)
}

/// Adds a file of `len` bytes at `path`, handed over as it is in a zip:
/// `deflated` (raw deflate) or stored. It stays packed until it is opened, so
/// the game's files take their packed size in memory. Its folders are made.
pub fn add_packed(
    path: impl AsRef<Path>,
    deflated: bool,
    len: u64,
    data: Vec<u8>,
) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let parts = parts(path);
    let mut tree = lock(&TREE);
    let dir = parent_mut(&mut tree, &parts, path)?;
    let name = parts.last().expect("parent_mut checked").clone();
    if let Some(Node::Dir(_)) = dir.entries.get(&name) {
        return Err(error(io::ErrorKind::IsADirectory, path, "a folder"));
    }
    dir.entries.insert(
        name,
        Node::Packed {
            data: data.into(),
            deflated,
            len,
            modified: now(),
        },
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    File,
    Dir,
}

/// A file or folder's metadata.
#[derive(Clone, Debug)]
pub struct Metadata {
    kind: Kind,
    len: u64,
    modified: Option<SystemTime>,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_file(&self) -> bool {
        self.kind == Kind::File
    }
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }
    pub fn is_symlink(&self) -> bool {
        false
    }
    pub fn file_type(&self) -> FileType {
        FileType(self.kind)
    }
    pub fn modified(&self) -> io::Result<SystemTime> {
        self.modified
            .ok_or_else(|| io::ErrorKind::Unsupported.into())
    }
    pub fn accessed(&self) -> io::Result<SystemTime> {
        self.modified()
    }
    pub fn created(&self) -> io::Result<SystemTime> {
        self.modified()
    }
    pub fn permissions(&self) -> Permissions {
        Permissions { readonly: false }
    }
}

/// Whether an entry is a file or a folder (never a link here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileType(Kind);

impl FileType {
    pub fn is_file(&self) -> bool {
        self.0 == Kind::File
    }
    pub fn is_dir(&self) -> bool {
        self.0 == Kind::Dir
    }
    pub fn is_symlink(&self) -> bool {
        false
    }
}

/// Permissions: only read-only, which nothing here enforces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permissions {
    readonly: bool,
}

impl Permissions {
    pub fn readonly(&self) -> bool {
        self.readonly
    }
    pub fn set_readonly(&mut self, readonly: bool) {
        self.readonly = readonly;
    }
}

/// File times to set; kept only as the modification time.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileTimes {
    modified: Option<SystemTime>,
}

impl FileTimes {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_accessed(self, _t: SystemTime) -> Self {
        self
    }
    pub fn set_modified(mut self, t: SystemTime) -> Self {
        self.modified = Some(t);
        self
    }
}

/// Why `try_lock` did not lock.
#[derive(Debug)]
pub enum TryLockError {
    Error(io::Error),
    WouldBlock,
}

impl fmt::Display for TryLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TryLockError::Error(e) => e.fmt(f),
            TryLockError::WouldBlock => f.write_str("the file is locked"),
        }
    }
}

impl std::error::Error for TryLockError {}

impl From<TryLockError> for io::Error {
    fn from(e: TryLockError) -> io::Error {
        match e {
            TryLockError::Error(e) => e,
            TryLockError::WouldBlock => io::ErrorKind::WouldBlock.into(),
        }
    }
}

/// An open file.
pub struct File {
    data: Bytes,
    pos: AtomicU64,
    read: bool,
    write: bool,
    append: bool,
    path: PathBuf,
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File").field("path", &self.path).finish()
    }
}

impl File {
    pub fn open(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new().read(true).open(path)
    }
    pub fn create(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
    }
    pub fn create_new(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
    }
    pub fn options() -> OpenOptions {
        OpenOptions::new()
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata {
            kind: Kind::File,
            len: lock(&self.data).len() as u64,
            modified: Some(now()),
        })
    }
    pub fn set_len(&self, size: u64) -> io::Result<()> {
        changed();
        lock(&self.data).resize(size as usize, 0);
        Ok(())
    }
    pub fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn sync_data(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn try_clone(&self) -> io::Result<File> {
        Ok(File {
            data: self.data.clone(),
            pos: AtomicU64::new(self.pos.load(Ordering::Relaxed)),
            read: self.read,
            write: self.write,
            append: self.append,
            path: self.path.clone(),
        })
    }
    pub fn set_permissions(&self, _perm: Permissions) -> io::Result<()> {
        Ok(())
    }
    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
        Ok(())
    }
    pub fn set_modified(&self, _time: SystemTime) -> io::Result<()> {
        Ok(())
    }
    pub fn lock(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn lock_shared(&self) -> io::Result<()> {
        Ok(())
    }
    pub fn try_lock(&self) -> Result<(), TryLockError> {
        Ok(())
    }
    pub fn try_lock_shared(&self) -> Result<(), TryLockError> {
        Ok(())
    }
    pub fn unlock(&self) -> io::Result<()> {
        Ok(())
    }

    fn read_into(&self, out: &mut [u8]) -> io::Result<usize> {
        if !self.read {
            return Err(error(
                io::ErrorKind::PermissionDenied,
                &self.path,
                "not opened for reading",
            ));
        }
        let data = lock(&self.data);
        let pos = self.pos.load(Ordering::Relaxed) as usize;
        let n = data.len().saturating_sub(pos).min(out.len());
        out[..n].copy_from_slice(&data[pos.min(data.len())..pos.min(data.len()) + n]);
        self.pos.store((pos + n) as u64, Ordering::Relaxed);
        Ok(n)
    }
    fn write_from(&self, buf: &[u8]) -> io::Result<usize> {
        if !self.write {
            return Err(error(
                io::ErrorKind::PermissionDenied,
                &self.path,
                "not opened for writing",
            ));
        }
        let mut data = lock(&self.data);
        let pos = if self.append {
            data.len()
        } else {
            self.pos.load(Ordering::Relaxed) as usize
        };
        if data.len() < pos + buf.len() {
            data.resize(pos + buf.len(), 0);
        }
        data[pos..pos + buf.len()].copy_from_slice(buf);
        self.pos.store((pos + buf.len()) as u64, Ordering::Relaxed);
        Ok(buf.len())
    }
    fn seek_to(&self, from: SeekFrom) -> io::Result<u64> {
        let len = lock(&self.data).len() as i64;
        let at = match from {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => len + n,
            SeekFrom::Current(n) => self.pos.load(Ordering::Relaxed) as i64 + n,
        };
        if at < 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.pos.store(at as u64, Ordering::Relaxed);
        Ok(at as u64)
    }
}

impl Read for File {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.read_into(out)
    }
}
impl Read for &File {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.read_into(out)
    }
}
impl Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_from(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Write for &File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_from(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for File {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.seek_to(from)
    }
}
impl Seek for &File {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.seek_to(from)
    }
}

/// How to open a file.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn read(&mut self, on: bool) -> &mut Self {
        self.read = on;
        self
    }
    pub fn write(&mut self, on: bool) -> &mut Self {
        self.write = on;
        self
    }
    pub fn append(&mut self, on: bool) -> &mut Self {
        self.append = on;
        self
    }
    pub fn truncate(&mut self, on: bool) -> &mut Self {
        self.truncate = on;
        self
    }
    pub fn create(&mut self, on: bool) -> &mut Self {
        self.create = on;
        self
    }
    pub fn create_new(&mut self, on: bool) -> &mut Self {
        self.create_new = on;
        self
    }
    pub fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref();
        let parts = parts(path);
        let writes = self.write || self.append;
        let mut tree = lock(&TREE);
        let dir = parent_mut(&mut tree, &parts, path)?;
        let name = parts.last().expect("parent_mut checked").clone();
        let data = match dir.entries.get_mut(&name) {
            Some(Node::Dir(_)) => return Err(error(io::ErrorKind::IsADirectory, path, "a folder")),
            Some(_) if self.create_new => {
                return Err(error(io::ErrorKind::AlreadyExists, path, "already exists"));
            }
            Some(Node::File { data, modified }) => {
                if writes {
                    changed();
                    *modified = now();
                    if self.truncate {
                        lock(data).clear();
                    }
                }
                data.clone()
            }
            Some(node @ (Node::Entry { .. } | Node::Packed { .. })) => {
                let bytes = if writes && self.truncate {
                    Vec::new()
                } else {
                    match node {
                        Node::Entry {
                            archive,
                            index,
                            len,
                            ..
                        } => inflate(archive, *index, *len)?,
                        Node::Packed {
                            data,
                            deflated,
                            len,
                            ..
                        } => unpack(data, *deflated, *len)?,
                        _ => unreachable!("matched above"),
                    }
                };
                let data: Bytes = Arc::new(Mutex::new(bytes));
                if writes {
                    // Copied out of the archive: from now on the file is the game's.
                    dir.entries.insert(
                        name,
                        Node::File {
                            data: data.clone(),
                            modified: now(),
                        },
                    );
                }
                data
            }
            None if self.create || self.create_new => {
                changed();
                if !writes {
                    return Err(error(
                        io::ErrorKind::InvalidInput,
                        path,
                        "creating needs write",
                    ));
                }
                let data: Bytes = Arc::new(Mutex::new(Vec::new()));
                dir.entries.insert(
                    name,
                    Node::File {
                        data: data.clone(),
                        modified: now(),
                    },
                );
                data
            }
            None => return Err(not_found(path)),
        };
        Ok(File {
            data,
            pos: AtomicU64::new(0),
            read: self.read,
            write: writes,
            append: self.append,
            path: absolute(path),
        })
    }
}

/// An entry of a folder.
#[derive(Debug)]
pub struct DirEntry {
    path: PathBuf,
    name: OsString,
    meta: Metadata,
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
    pub fn file_name(&self) -> OsString {
        self.name.clone()
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        Ok(self.meta.clone())
    }
    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(self.meta.file_type())
    }
}

/// A folder's entries, in name order.
#[derive(Debug)]
pub struct ReadDir(std::vec::IntoIter<DirEntry>);

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Ok)
    }
}

/// Makes folders.
#[derive(Debug, Default)]
pub struct DirBuilder {
    recursive: bool,
}

impl DirBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn recursive(&mut self, on: bool) -> &mut Self {
        self.recursive = on;
        self
    }
    pub fn create(&self, path: impl AsRef<Path>) -> io::Result<()> {
        if self.recursive {
            create_dir_all(path)
        } else {
            create_dir(path)
        }
    }
}

pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    let path = path.as_ref();
    let tree = lock(&TREE);
    find(&tree, &parts(path))
        .map(Node::meta)
        .ok_or_else(|| not_found(path))
}

pub fn symlink_metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    metadata(path)
}

pub fn exists(path: impl AsRef<Path>) -> io::Result<bool> {
    Ok(metadata(path).is_ok())
}

pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path = path.as_ref();
    metadata(path)?;
    Ok(absolute(path))
}

pub fn read_link(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    Err(error(
        io::ErrorKind::InvalidInput,
        path.as_ref(),
        "not a link",
    ))
}

pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    let node = {
        let tree = lock(&TREE);
        find(&tree, &parts(path)).cloned()
    };
    match node {
        Some(Node::File { data, .. }) => Ok(lock(&data).clone()),
        Some(Node::Entry {
            archive,
            index,
            len,
            ..
        }) => inflate(&archive, index, len),
        Some(Node::Packed {
            data,
            deflated,
            len,
            ..
        }) => unpack(&data, deflated, len),
        Some(Node::Dir(_)) => Err(error(io::ErrorKind::IsADirectory, path, "a folder")),
        None => Err(not_found(path)),
    }
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    changed();
    let path = path.as_ref();
    let parts = parts(path);
    let mut tree = lock(&TREE);
    let dir = parent_mut(&mut tree, &parts, path)?;
    let name = parts.last().expect("parent_mut checked").clone();
    if let Some(Node::Dir(_)) = dir.entries.get(&name) {
        return Err(error(io::ErrorKind::IsADirectory, path, "a folder"));
    }
    if let Some(Node::File { data, modified }) = dir.entries.get_mut(&name) {
        *lock(data) = contents.as_ref().to_vec();
        *modified = now();
        return Ok(());
    }
    dir.entries.insert(
        name,
        Node::File {
            data: Arc::new(Mutex::new(contents.as_ref().to_vec())),
            modified: now(),
        },
    );
    Ok(())
}

pub fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
    let path = path.as_ref();
    let tree = lock(&TREE);
    match find(&tree, &parts(path)) {
        Some(Node::Dir(dir)) => Ok(ReadDir(
            dir.entries
                .iter()
                .map(|(name, node)| DirEntry {
                    path: path.join(name),
                    name: name.clone(),
                    meta: node.meta(),
                })
                .collect::<Vec<_>>()
                .into_iter(),
        )),
        Some(_) => Err(error(io::ErrorKind::NotADirectory, path, "not a folder")),
        None => Err(not_found(path)),
    }
}

pub fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
    changed();
    let path = path.as_ref();
    let parts = parts(path);
    if parts.is_empty() {
        return Err(error(io::ErrorKind::AlreadyExists, path, "already exists"));
    }
    let mut tree = lock(&TREE);
    let dir = parent_mut(&mut tree, &parts, path)?;
    let name = parts.last().expect("not empty").clone();
    if dir.entries.contains_key(&name) {
        return Err(error(io::ErrorKind::AlreadyExists, path, "already exists"));
    }
    dir.entries.insert(name, Node::dir());
    Ok(())
}

pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    changed();
    let path = path.as_ref();
    let mut tree = lock(&TREE);
    let mut node = &mut *tree;
    for name in parts(path) {
        let Node::Dir(dir) = node else {
            return Err(error(
                io::ErrorKind::NotADirectory,
                path,
                "a file is in the way",
            ));
        };
        node = dir.entries.entry(name).or_insert_with(Node::dir);
    }
    match node {
        Node::Dir(_) => Ok(()),
        _ => Err(error(
            io::ErrorKind::AlreadyExists,
            path,
            "a file, not a folder",
        )),
    }
}

fn remove(path: &Path, want_dir: bool, recursive: bool) -> io::Result<()> {
    changed();
    let parts = parts(path);
    let mut tree = lock(&TREE);
    let dir = parent_mut(&mut tree, &parts, path)?;
    let name = parts.last().expect("parent_mut checked");
    match dir.entries.get(name) {
        None => return Err(not_found(path)),
        Some(Node::Dir(inner)) => {
            if !want_dir {
                return Err(error(io::ErrorKind::IsADirectory, path, "a folder"));
            }
            if !recursive && !inner.entries.is_empty() {
                return Err(error(io::ErrorKind::DirectoryNotEmpty, path, "not empty"));
            }
        }
        Some(_) if want_dir => {
            return Err(error(io::ErrorKind::NotADirectory, path, "not a folder"));
        }
        Some(_) => {}
    }
    dir.entries.remove(name);
    Ok(())
}

pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    remove(path.as_ref(), false, false)
}

pub fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    remove(path.as_ref(), true, false)
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    remove(path.as_ref(), true, true)
}

pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    changed();
    let (from, to) = (from.as_ref(), to.as_ref());
    let (src, dst) = (parts(from), parts(to));
    if src == dst {
        return metadata(from).map(|_| ());
    }
    if dst.starts_with(&src) {
        return Err(error(
            io::ErrorKind::InvalidInput,
            to,
            "inside the folder it moves",
        ));
    }
    let mut tree = lock(&TREE);
    // The target's folder must exist before anything moves.
    parent_mut(&mut tree, &dst, to)?;
    let node = {
        let dir = parent_mut(&mut tree, &src, from)?;
        dir.entries
            .remove(src.last().expect("checked"))
            .ok_or_else(|| not_found(from))?
    };
    let dir = parent_mut(&mut tree, &dst, to).expect("checked above");
    let name = dst.last().expect("checked").clone();
    if let (Some(Node::Dir(existing)), Node::Dir(_)) = (dir.entries.get(&name), &node)
        && !existing.entries.is_empty()
    {
        return Err(error(io::ErrorKind::DirectoryNotEmpty, to, "not empty"));
    }
    dir.entries.insert(name, node);
    Ok(())
}

pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<u64> {
    let bytes = read(from)?;
    let len = bytes.len() as u64;
    write(to, bytes)?;
    Ok(len)
}

pub fn hard_link(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    copy(from, to).map(|_| ())
}

pub fn set_permissions(path: impl AsRef<Path>, _perm: Permissions) -> io::Result<()> {
    metadata(path).map(|_| ())
}

/// What [`export`] writes: a magic, then each entry as its kind (0 a file,
/// 1 a folder), its path's length and path, and for a file its length and bytes.
const SNAPSHOT_MAGIC: &[u8; 8] = b"HZFS\0\0\0\x01";

/// The files and folders the game made, outside `skip`'s folders (mounted game
/// files, temporary files), in one block [`import`] restores: what the page keeps
/// between visits. Mounted zip entries are never in it.
pub fn export(skip: &[&str]) -> Vec<u8> {
    fn walk(node: &Node, path: &mut PathBuf, skip: &[PathBuf], out: &mut Vec<u8>) {
        if skip.iter().any(|s| s == path) {
            return;
        }
        let mut entry = |kind: u8, data: Option<&[u8]>| {
            let name = path.to_string_lossy();
            out.push(kind);
            out.extend_from_slice(&(name.len() as u32).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            if let Some(data) = data {
                out.extend_from_slice(&(data.len() as u64).to_le_bytes());
                out.extend_from_slice(data);
            }
        };
        match node {
            Node::File { data, .. } => entry(0, Some(&lock(data))),
            Node::Entry { .. } | Node::Packed { .. } => {}
            Node::Dir(dir) => {
                if path.as_os_str() != "/" {
                    entry(1, None);
                }
                for (name, child) in &dir.entries {
                    path.push(name);
                    walk(child, path, skip, out);
                    path.pop();
                }
            }
        }
    }
    let skip: Vec<PathBuf> = skip.iter().map(|s| absolute(Path::new(s))).collect();
    let mut out = SNAPSHOT_MAGIC.to_vec();
    let tree = lock(&TREE);
    walk(&tree, &mut PathBuf::from("/"), &skip, &mut out);
    out
}

/// Restores what [`export`] wrote, over what is there; returns the files made.
pub fn import(bytes: &[u8]) -> io::Result<usize> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "not a saved file system");
    let rest = bytes
        .strip_prefix(SNAPSHOT_MAGIC.as_slice())
        .ok_or_else(bad)?;
    let mut at = 0;
    let mut take = |n: usize| -> io::Result<&[u8]> {
        let part = rest.get(at..at + n).ok_or_else(bad)?;
        at += n;
        Ok(part)
    };
    let mut files = 0;
    loop {
        let Ok(kind) = take(1) else { break };
        let kind = kind[0];
        let len = u32::from_le_bytes(take(4)?.try_into().map_err(|_| bad())?) as usize;
        let path = PathBuf::from(std::str::from_utf8(take(len)?).map_err(|_| bad())?);
        match kind {
            0 => {
                let len = u64::from_le_bytes(take(8)?.try_into().map_err(|_| bad())?) as usize;
                let data = take(len)?.to_vec();
                if let Some(parent) = path.parent() {
                    create_dir_all(parent)?;
                }
                write(&path, data)?;
                files += 1;
            }
            1 => create_dir_all(&path)?,
            _ => return Err(bad()),
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree is one per process: each test works under its own folder, and the
    /// tests take turns (the change count is the whole tree's: another test's write
    /// would move it in the middle of `writes_move_the_change_count`).
    fn root(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let root = PathBuf::from("/test").join(name);
        let _ = remove_dir_all(&root);
        create_dir_all(&root).unwrap();
        (turn, root)
    }

    #[test]
    fn files_are_written_read_listed_renamed_and_removed() {
        let (_turn, r) = root("files");
        write(r.join("a.txt"), b"hello").unwrap();
        assert_eq!(read_to_string(r.join("a.txt")).unwrap(), "hello");
        assert!(
            write(r.join("missing/b.txt"), b"x").is_err(),
            "a parent must exist"
        );
        create_dir_all(r.join("sub/deeper")).unwrap();
        rename(r.join("a.txt"), r.join("sub/deeper/a.txt")).unwrap();
        assert_eq!(read(r.join("sub/deeper/a.txt")).unwrap(), b"hello");
        assert_eq!(
            metadata(r.join("a.txt")).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        let names: Vec<_> = read_dir(&r)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["sub"]);
        assert_eq!(
            remove_dir(r.join("sub")).unwrap_err().kind(),
            io::ErrorKind::DirectoryNotEmpty
        );
        remove_dir_all(r.join("sub")).unwrap();
        assert_eq!(read_dir(&r).unwrap().count(), 0);
    }

    #[test]
    fn open_options_append_truncate_and_seek_like_std() {
        let (_turn, r) = root("options");
        let path = r.join("log");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"one ").unwrap();
        f.write_all(b"two").unwrap();
        assert_eq!(read(&path).unwrap(), b"one two");
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        f.seek(SeekFrom::Start(4)).unwrap();
        f.write_all(b"TWO").unwrap();
        assert_eq!(read(&path).unwrap(), b"one TWO");
        let mut f = File::open(&path).unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "one TWO");
        assert!(f.write_all(b"x").is_err(), "opened for reading only");
        File::create(&path).unwrap();
        assert_eq!(metadata(&path).unwrap().len(), 0, "create truncates");
        assert_eq!(
            File::create_new(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut out);
        for (name, data) in entries {
            if name.ends_with('/') {
                writer
                    .add_directory(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
            } else {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(data).unwrap();
            }
        }
        writer.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn a_mounted_zip_reads_its_entries_and_copies_one_when_written() {
        let (_turn, r) = root("zip");
        let bytes = zip(&[
            ("26.3/", b""),
            ("26.3/a.json", b"{}"),
            ("26.3/textures/b.png", b"png"),
        ]);
        assert_eq!(mount_zip(r.join("game"), bytes).unwrap(), 2);
        assert_eq!(read(r.join("game/26.3/a.json")).unwrap(), b"{}");
        assert_eq!(
            metadata(r.join("game/26.3/textures/b.png")).unwrap().len(),
            3
        );
        assert!(metadata(r.join("game/26.3/textures")).unwrap().is_dir());
        rename(r.join("game/26.3"), r.join("resources")).unwrap();
        let mut f = OpenOptions::new()
            .append(true)
            .open(r.join("resources/a.json"))
            .unwrap();
        f.write_all(b"!").unwrap();
        assert_eq!(read(r.join("resources/a.json")).unwrap(), b"{}!");
    }

    #[test]
    fn packed_files_read_as_their_bytes_and_stay_out_of_exports() {
        let (_turn, r) = root("packed");
        let text = b"a packed file, a packed file, a packed file".to_vec();
        let deflated = miniz_oxide::deflate::compress_to_vec(&text, 6);
        add_packed(r.join("res/a/b.json"), true, text.len() as u64, deflated).unwrap();
        add_packed(r.join("res/stored.txt"), false, 3, b"abc".to_vec()).unwrap();
        assert_eq!(read(r.join("res/a/b.json")).unwrap(), text);
        assert_eq!(
            metadata(r.join("res/a/b.json")).unwrap().len(),
            text.len() as u64
        );
        let mut s = String::new();
        File::open(r.join("res/stored.txt"))
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "abc");
        add_packed(r.join("res/bad"), true, 99, vec![1, 2, 3]).unwrap();
        assert!(
            read(r.join("res/bad")).is_err(),
            "a wrong length is an error"
        );
        let saved = export(&[]);
        assert!(
            !saved.windows(6).any(|w| w == b"b.json"),
            "the player's files stay out"
        );
    }

    #[test]
    fn an_export_restores_what_the_game_wrote_and_leaves_out_mounts_and_skipped_folders() {
        let (_turn, r) = root("export");
        create_dir_all(r.join("saves/World/region")).unwrap();
        write(r.join("saves/World/level.dat"), [1, 2, 3]).unwrap();
        write(r.join("options.txt"), b"fov:70").unwrap();
        create_dir_all(r.join("tmp")).unwrap();
        write(r.join("tmp/scratch"), b"drop me").unwrap();
        mount_zip(r.join("mounted"), zip(&[("x.json", b"1")])).unwrap();
        let before = changes();
        let tmp = r.join("tmp");
        let saved = export(&[tmp.to_str().unwrap()]);
        assert_eq!(changes(), before, "an export changes nothing");
        remove_dir_all(&r).unwrap();
        assert!(import(&saved).unwrap() >= 2);
        assert_eq!(read(r.join("saves/World/level.dat")).unwrap(), [1, 2, 3]);
        assert_eq!(read_to_string(r.join("options.txt")).unwrap(), "fov:70");
        assert!(
            metadata(r.join("saves/World/region")).unwrap().is_dir(),
            "empty folders too"
        );
        assert!(metadata(r.join("tmp/scratch")).is_err());
        assert!(
            metadata(r.join("mounted/x.json")).is_err(),
            "the player's files are not saved"
        );
        assert!(import(b"not a snapshot").is_err());
    }

    #[test]
    fn writes_move_the_change_count() {
        let (_turn, r) = root("changes");
        let before = changes();
        write(r.join("f"), b"1").unwrap();
        assert!(changes() > before);
        let before = changes();
        let _ = read(r.join("f")).unwrap();
        let _ = metadata(r.join("f")).unwrap();
        assert_eq!(changes(), before, "reads change nothing");
    }
}
