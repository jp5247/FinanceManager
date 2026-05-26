use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to `target` atomically.
///
/// Strategy: write to a sibling `*.tmp.<pid>` file in the same directory, fsync,
/// then rename over `target`. Same-directory ensures the rename is atomic on
/// the same volume (POSIX) and works on Windows too. Parent directory is
/// created if missing.
///
/// On Windows, `fs::rename` overwrites the destination atomically when the
/// underlying filesystem supports it (NTFS does). On older filesystems the
/// operation may not be atomic; we accept this for Phase-1.
pub fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target has no parent dir"))?;
    fs::create_dir_all(parent)?;

    let tmp_name = match target.file_name() {
        Some(name) => format!("{}.tmp.{}", name.to_string_lossy(), std::process::id()),
        None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name")),
    };
    let tmp_path = parent.join(tmp_name);

    {
        let mut f: File = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }

    if let Err(e) = fs::rename(&tmp_path, target) {
        // Best-effort cleanup if rename fails.
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_new_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        atomic_write(&p, b"hello").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"hello");
    }

    #[test]
    fn overwrites_existing() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, b"old").unwrap();
        atomic_write(&p, b"new").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"new");
    }

    #[test]
    fn creates_parent_directories() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("nested").join("sub").join("a.txt");
        atomic_write(&p, b"x").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"x");
    }

    #[test]
    fn leaves_no_tmp_file_behind_on_success() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        atomic_write(&p, b"data").unwrap();
        let tmps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(tmps.is_empty(), "stale tmp files: {tmps:?}");
    }
}
