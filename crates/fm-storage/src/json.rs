use crate::atomic::atomic_write;
use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// On-disk envelope for every generated JSON document.
///
/// Wrapping the payload with an explicit `schema_version` lets future code
/// reject or migrate files written by older binaries deterministically, per
/// `docs/design/local-data-schema.md` §7.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedJson<T> {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub data: T,
}

impl<T> VersionedJson<T> {
    pub fn new(schema_version: u32, data: T) -> Self {
        Self {
            schema_version,
            data,
        }
    }
}

/// Read and parse a versioned JSON file. Fails if the on-disk schema version
/// doesn't match `expected_version`.
pub fn read_json<T>(path: &Path, expected_version: u32) -> Result<VersionedJson<T>, StorageError>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = fs::read(path)?;
    let doc: VersionedJson<T> = serde_json::from_slice(&bytes)?;
    if doc.schema_version != expected_version {
        return Err(StorageError::SchemaVersionMismatch {
            expected: expected_version,
            found: doc.schema_version,
        });
    }
    Ok(doc)
}

/// Encode `doc` as pretty JSON and atomically write it to `path`.
pub fn write_json<T>(path: &Path, doc: &VersionedJson<T>) -> Result<(), StorageError>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec_pretty(doc)?;
    atomic_write(path, &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Profile {
        display_name: String,
        currency: String,
    }

    #[test]
    fn round_trips() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("profile.json");
        let doc = VersionedJson::new(
            1,
            Profile {
                display_name: "Asha".into(),
                currency: "INR".into(),
            },
        );
        write_json(&p, &doc).unwrap();
        let back: VersionedJson<Profile> = read_json(&p, 1).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("profile.json");
        let doc = VersionedJson::new(
            1,
            Profile {
                display_name: "Asha".into(),
                currency: "INR".into(),
            },
        );
        write_json(&p, &doc).unwrap();
        let err = read_json::<Profile>(&p, 2).unwrap_err();
        assert!(matches!(
            err,
            StorageError::SchemaVersionMismatch {
                expected: 2,
                found: 1
            }
        ));
    }
}
