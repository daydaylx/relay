use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sha256::{Sha256, hex};

/// Replace `path` with `contents` so readers see either the old or the new file, never a mix.
pub(crate) fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!(
            "output directory does not exist: {}",
            parent.display()
        ));
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path
        .file_name()
        .ok_or_else(|| "output path must name a file".to_owned())?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.relay-{}-{nonce}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)
            .map_err(|error| format!("could not create temporary file: {error}"))?;
        file.write_all(contents)
            .map_err(|error| format!("could not write temporary file: {error}"))?;
        // `mode` is subject to the umask; make the requested permissions exact.
        file.set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|error| format!("could not set file permissions: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("could not sync temporary file: {error}"))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Create `path` (and missing parents) readable only by the current user.
pub(crate) fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|error| format!("could not create directory {}: {error}", path.display()))
}

/// Write a private (0600) file atomically; used for state that may contain evaluated values.
pub(crate) fn write_private(path: &Path, contents: &[u8]) -> Result<(), String> {
    write_atomic(path, contents, 0o600)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Keep at most `limit` bytes of command output for a diagnostic log.
pub(crate) fn truncate_log(bytes: &[u8], limit: usize) -> Vec<u8> {
    if bytes.len() <= limit {
        return bytes.to_vec();
    }
    let mut kept = bytes[bytes.len() - limit..].to_vec();
    let mut marked = b"[...truncated...]\n".to_vec();
    marked.append(&mut kept);
    marked
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique scratch directory that is removed on drop.
    pub(crate) struct TempDir(PathBuf);

    impl TempDir {
        pub(crate) fn new(label: &str) -> Self {
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "relay-test-{label}-{}-{nonce}-{id}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::TempDir;
    use super::{sha256_file, truncate_log, write_atomic};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn atomic_write_replaces_content_with_exact_mode_and_leaves_no_temporaries() {
        let dir = TempDir::new("fsutil");
        let target = dir.path().join("managed.nix");
        write_atomic(&target, b"one", 0o644).unwrap();
        write_atomic(&target, b"two", 0o600).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"two");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            sha256_file(&target).unwrap(),
            crate::sha256::sha256_hex(b"two")
        );
    }

    #[test]
    fn atomic_write_refuses_missing_directories() {
        let dir = TempDir::new("fsutil");
        assert!(write_atomic(&dir.path().join("missing/file"), b"x", 0o644).is_err());
    }

    #[test]
    fn log_truncation_keeps_the_tail() {
        let kept = truncate_log(b"0123456789", 4);
        assert!(kept.ends_with(b"6789"));
        assert!(kept.starts_with(b"[...truncated"));
        assert_eq!(truncate_log(b"abc", 4), b"abc");
    }
}
