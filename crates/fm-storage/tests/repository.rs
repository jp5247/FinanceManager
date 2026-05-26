//! End-to-end tests for the FilesystemStorage implementation.

use fm_core::UserId;
use fm_storage::{DataRoot, FilesystemStorage, StorageRepository};
use tempfile::tempdir;

fn fresh_storage() -> (tempfile::TempDir, FilesystemStorage, UserId) {
    let tmp = tempdir().unwrap();
    let root = DataRoot::new(tmp.path()).unwrap();
    (
        tmp,
        FilesystemStorage::new(root),
        UserId::new("user-001").unwrap(),
    )
}

#[test]
fn write_then_read_round_trip() {
    let (_g, s, user) = fresh_storage();
    s.write(&user, "settings.json", b"{\"k\":1}").unwrap();
    let bytes = s.read(&user, "settings.json").unwrap();
    assert_eq!(bytes, b"{\"k\":1}");
}

#[test]
fn exists_reflects_writes() {
    let (_g, s, user) = fresh_storage();
    assert!(!s.exists(&user, "x.json").unwrap());
    s.write(&user, "x.json", b"x").unwrap();
    assert!(s.exists(&user, "x.json").unwrap());
}

#[test]
fn write_creates_nested_directories() {
    let (_g, s, user) = fresh_storage();
    s.write(
        &user,
        "source/uploads/2026/04/imp-001/file-meta.json",
        b"{}",
    )
    .unwrap();
    assert!(s
        .exists(&user, "source/uploads/2026/04/imp-001/file-meta.json")
        .unwrap());
}

#[test]
fn overwrites_existing_file() {
    let (_g, s, user) = fresh_storage();
    s.write(&user, "x.json", b"v1").unwrap();
    s.write(&user, "x.json", b"v2").unwrap();
    assert_eq!(s.read(&user, "x.json").unwrap(), b"v2");
}

#[test]
fn cross_profile_isolation_traversal_blocked() {
    let (_g, s, alice) = fresh_storage();
    let bob = UserId::new("bob").unwrap();
    // Seed bob's data directly via the public API.
    s.write(&bob, "secret.json", b"bob's secret").unwrap();
    // alice cannot reach bob's file by any traversal trick.
    for attempt in [
        "../bob/secret.json",
        "../../users/bob/secret.json",
        "settings/../../bob/secret.json",
        "..\\bob\\secret.json",
    ] {
        let r = s.read(&alice, attempt);
        assert!(r.is_err(), "alice should not read {attempt:?}, got {r:?}");
    }
}

#[test]
fn rejects_bad_relative_paths_on_write() {
    let (_g, s, user) = fresh_storage();
    let cases = ["", "..", "/etc/passwd", "C:\\Windows", "a/../b"];
    for bad in cases {
        let r = s.write(&user, bad, b"x");
        assert!(r.is_err(), "should reject write to {bad:?}");
    }
}

#[test]
fn read_missing_returns_io_error() {
    let (_g, s, user) = fresh_storage();
    let r = s.read(&user, "missing.json");
    assert!(matches!(r, Err(fm_storage::StorageError::Io(_))));
}
