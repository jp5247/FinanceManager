use crate::atomic::atomic_write;
use crate::error::StorageError;
use crate::path::{DataRoot, ProfileRoot};
use fm_core::UserId;
use std::fs;
use std::path::PathBuf;

/// The seam between the app and on-disk storage.
///
/// All per-user reads and writes go through this trait. Phase 1 ships a
/// filesystem-backed implementation ([`FilesystemStorage`]); Phase 2 may swap
/// in a SQLite-backed implementation without touching any caller.
pub trait StorageRepository {
    fn read(&self, user: &UserId, relative: &str) -> Result<Vec<u8>, StorageError>;
    fn write(&self, user: &UserId, relative: &str, bytes: &[u8]) -> Result<(), StorageError>;
    fn exists(&self, user: &UserId, relative: &str) -> Result<bool, StorageError>;
    fn resolve(&self, user: &UserId, relative: &str) -> Result<PathBuf, StorageError>;
}

/// Filesystem-backed [`StorageRepository`]. Every operation is scoped to the
/// caller's [`ProfileRoot`] and routed through the path-traversal guard.
#[derive(Clone, Debug)]
pub struct FilesystemStorage {
    root: DataRoot,
}

impl FilesystemStorage {
    pub fn new(root: DataRoot) -> Self {
        Self { root }
    }

    fn profile(&self, user: &UserId) -> ProfileRoot {
        self.root.profile(user)
    }
}

impl StorageRepository for FilesystemStorage {
    fn read(&self, user: &UserId, relative: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.profile(user).resolve(relative)?;
        Ok(fs::read(&path)?)
    }

    fn write(&self, user: &UserId, relative: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let path = self.profile(user).resolve(relative)?;
        atomic_write(&path, bytes)?;
        Ok(())
    }

    fn exists(&self, user: &UserId, relative: &str) -> Result<bool, StorageError> {
        let path = self.profile(user).resolve(relative)?;
        Ok(path.exists())
    }

    fn resolve(&self, user: &UserId, relative: &str) -> Result<PathBuf, StorageError> {
        self.profile(user).resolve(relative)
    }
}
