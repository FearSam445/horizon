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
    use web_time::web::SystemTimeExt;
    web_time::SystemTime::now().to_std()
}

#[derive(Clone)]
enum Node {
    Dir(Dir),
    File { data: Bytes, modified: SystemTime },
    Entry { archive: Arc<Archive>, index: usize, len: u64, modified: SystemTime },
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
            Node::Dir(_) => Metadata { kind: Kind::Dir, len: 0, modified: None },
            Node::File { data, modified } => Metadata {
                kind: Kind::File,
                len: lock(data).len() as u64,
                modified: Some(*modified),
            },
            Node::Entry { len, modified, .. } => Metadata {
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
    io::Error::new(io::ErrorKind::NotFound, format!("{}: no such file or folder", path.display()))
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
    let (_, folder) = parts.split_last().ok_or_else(|| error(io::ErrorKind::InvalidInput, path, "the root"))?;
    match find_mut(root, folder) {
        Some(Node::Dir(dir)) => Ok(dir),
        Some(_) => Err(error(io::ErrorKind::NotADirectory, path, "a parent is not a folder")),
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
            (entry.name().to_owned(), entry.size(), entry.is_dir())
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
                return Err(error(io::ErrorKind::InvalidData, Path::new(&name), "a file is in the way"));
            };
            node = dir.entries.entry(folder.clone()).or_insert_with(Node::dir);
        }
        let Node::Dir(dir) = node else {
            return Err(error(io::ErrorKind::InvalidData, Path::new(&name), "a file is in the way"));
        };
        if is_dir {
            dir.entries.entry(last.clone()).or_insert_with(Node::dir);
        } else {
            files += 1;
            dir.entries.insert(
                last.clone(),
                Node::Entry { archive: archive.clone(), index, len, modified },
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
        self.modified.ok_or_else(|| io::ErrorKind::Unsupported.into())
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
        OpenOptions::new().write(true).create(true).truncate(true).open(path)
    }
    pub fn create_new(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new().read(true).write(true).create_new(true).open(path)
    }
    pub fn options() -> OpenOptions {
        OpenOptions::new()
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata { kind: Kind::File, len: lock(&self.data).len() as u64, modified: Some(now()) })
    }
    pub fn set_len(&self, size: u64) -> io::Result<()> {
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
            return Err(error(io::ErrorKind::PermissionDenied, &self.path, "not opened for reading"));
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
            return Err(error(io::ErrorKind::PermissionDenied, &self.path, "not opened for writing"));
        }
        let mut data = lock(&self.data);
        let pos = if self.append { data.len() } else { self.pos.load(Ordering::Relaxed) as usize };
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
                    *modified = now();
                    if self.truncate {
                        lock(data).clear();
                    }
                }
                data.clone()
            }
            Some(Node::Entry { archive, index, len, .. }) => {
                let bytes = if writes && self.truncate {
                    Vec::new()
                } else {
                    inflate(archive, *index, *len)?
                };
                let data: Bytes = Arc::new(Mutex::new(bytes));
                if writes {
                    // Copied out of the archive: from now on the file is the game's.
                    dir.entries.insert(name, Node::File { data: data.clone(), modified: now() });
                }
                data
            }
            None if self.create || self.create_new => {
                if !writes {
                    return Err(error(io::ErrorKind::InvalidInput, path, "creating needs write"));
                }
                let data: Bytes = Arc::new(Mutex::new(Vec::new()));
                dir.entries.insert(name, Node::File { data: data.clone(), modified: now() });
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
        if self.recursive { create_dir_all(path) } else { create_dir(path) }
    }
}

pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    let path = path.as_ref();
    let tree = lock(&TREE);
    find(&tree, &parts(path)).map(Node::meta).ok_or_else(|| not_found(path))
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
    Err(error(io::ErrorKind::InvalidInput, path.as_ref(), "not a link"))
}

pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    let node = {
        let tree = lock(&TREE);
        find(&tree, &parts(path)).cloned()
    };
    match node {
        Some(Node::File { data, .. }) => Ok(lock(&data).clone()),
        Some(Node::Entry { archive, index, len, .. }) => inflate(&archive, index, len),
        Some(Node::Dir(_)) => Err(error(io::ErrorKind::IsADirectory, path, "a folder")),
        None => Err(not_found(path)),
    }
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
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
        Node::File { data: Arc::new(Mutex::new(contents.as_ref().to_vec())), modified: now() },
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
    let path = path.as_ref();
    let mut tree = lock(&TREE);
    let mut node = &mut *tree;
    for name in parts(path) {
        let Node::Dir(dir) = node else {
            return Err(error(io::ErrorKind::NotADirectory, path, "a file is in the way"));
        };
        node = dir.entries.entry(name).or_insert_with(Node::dir);
    }
    match node {
        Node::Dir(_) => Ok(()),
        _ => Err(error(io::ErrorKind::AlreadyExists, path, "a file, not a folder")),
    }
}

fn remove(path: &Path, want_dir: bool, recursive: bool) -> io::Result<()> {
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
        Some(_) if want_dir => return Err(error(io::ErrorKind::NotADirectory, path, "not a folder")),
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
    let (from, to) = (from.as_ref(), to.as_ref());
    let (src, dst) = (parts(from), parts(to));
    if src == dst {
        return metadata(from).map(|_| ());
    }
    if dst.starts_with(&src) {
        return Err(error(io::ErrorKind::InvalidInput, to, "inside the folder it moves"));
    }
    let mut tree = lock(&TREE);
    // The target's folder must exist before anything moves.
    parent_mut(&mut tree, &dst, to)?;
    let node = {
        let dir = parent_mut(&mut tree, &src, from)?;
        dir.entries.remove(src.last().expect("checked")).ok_or_else(|| not_found(from))?
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
