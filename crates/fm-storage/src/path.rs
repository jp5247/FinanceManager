use crate::error::StorageError;
use fm_core::UserId;
use std::path::{Component, Path, PathBuf};

/// Absolute root under which every user profile lives.
///
/// Construct once at app startup. The path must already exist and must be
/// absolute. All later resolution is anchored here.
#[derive(Clone, Debug)]
pub struct DataRoot {
    base: PathBuf,
}

impl DataRoot {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        if !path.is_absolute() || !path.is_dir() {
            return Err(StorageError::InvalidDataRoot);
        }
        Ok(Self { base: path })
    }

    pub fn as_path(&self) -> &Path {
        &self.base
    }

    /// Returns the per-user subtree handle. Does not create the directory.
    pub fn profile(&self, user: &UserId) -> ProfileRoot {
        ProfileRoot {
            base: self.base.join("users").join(user.as_str()),
        }
    }
}

/// Scope under which a single user's files live: `<data_root>/users/<userId>/`.
///
/// All relative paths must resolve to descendants of this directory.
#[derive(Clone, Debug)]
pub struct ProfileRoot {
    base: PathBuf,
}

impl ProfileRoot {
    pub fn as_path(&self) -> &Path {
        &self.base
    }

    /// Resolves a relative path against the profile root.
    ///
    /// Syntactic check only — does not touch the filesystem. Rejects:
    /// - empty input
    /// - absolute paths
    /// - drive-letter / UNC / root prefixes
    /// - any `..` segment (regardless of position)
    /// - paths with embedded NUL bytes
    ///
    /// `.` segments are allowed and ignored.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, StorageError> {
        if relative.is_empty() {
            return Err(StorageError::InvalidPath("empty path"));
        }
        if relative.contains('\0') {
            return Err(StorageError::InvalidPath("NUL byte in path"));
        }
        let rel = Path::new(relative);
        if rel.is_absolute() {
            return Err(StorageError::InvalidPath("absolute path not allowed"));
        }
        let mut out = self.base.clone();
        for c in rel.components() {
            match c {
                Component::Normal(seg) => out.push(seg),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(StorageError::PathTraversalDetected);
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(StorageError::InvalidPath("absolute or drive-prefixed path"));
                }
            }
        }
        Ok(out)
    }
}
