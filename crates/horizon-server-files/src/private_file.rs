//! Files only their owner may read: the control API's and the game's control endpoint (each holds a
//! token) and the management protocol's generated key. Written whole to a temporary file this
//! process creates with owner-only permissions, then renamed over the path, so the result never
//! keeps an old file's wider permissions and never writes through a link left at the path.
#[allow(unused_imports)]
use horizon_fs::{KindExt as _, MetaExt as _, PathExt as _};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes `text` to `path`, readable and writable by its owner only (0600 on Unix; on Windows the
/// folder's inherited permissions, as for any file). Replaces what was at `path`, a link included
/// (the link itself, not what it points to).
pub fn write_private(path: &Path, text: impl AsRef<[u8]>) -> io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let tmp = path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        horizon_fs::process_id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = horizon_fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options.open(&tmp).and_then(|mut file| {
        file.write_all(text.as_ref())?;
        file.sync_all()
    });
    let result = written.and_then(|()| horizon_fs::rename(&tmp, path));
    if result.is_err() {
        let _ = horizon_fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = horizon_fs::temp_dir().join(format!(
            "horizon-private-{name}-{}",
            horizon_fs::process_id()
        ));
        let _ = horizon_fs::remove_dir_all(&dir);
        horizon_fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_whole_and_leaves_no_temporary_file() {
        let dir = scratch("whole");
        let path = dir.join("api.json");
        write_private(&path, "one").unwrap();
        write_private(&path, "two").unwrap();
        assert_eq!(horizon_fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(horizon_fs::read_dir(&dir).unwrap().count(), 1);
        horizon_fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_old_readable_file_becomes_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let path = dir.join("api.json");
        horizon_fs::write(&path, "old token").unwrap();
        horizon_fs::set_permissions(&path, horizon_fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, "new token").unwrap();
        let mode = horizon_fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        horizon_fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_link_at_the_path_is_replaced_not_followed() {
        let dir = scratch("link");
        let elsewhere = dir.join("elsewhere.txt");
        horizon_fs::write(&elsewhere, "untouched").unwrap();
        let path = dir.join("api.json");
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        write_private(&path, "token").unwrap();
        assert_eq!(horizon_fs::read_to_string(&elsewhere).unwrap(), "untouched");
        assert!(
            !horizon_fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .fs_is_symlink()
        );
        assert_eq!(horizon_fs::read_to_string(&path).unwrap(), "token");
        horizon_fs::remove_dir_all(dir).unwrap();
    }
}
