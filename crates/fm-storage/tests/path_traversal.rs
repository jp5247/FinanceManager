//! Adversarial inputs to the path resolver. Every case here must be rejected
//! before any I/O happens. If a case ever returns `Ok`, that is a SEV-1
//! security regression — the cross-profile isolation invariant is broken.

use fm_core::UserId;
use fm_storage::{DataRoot, StorageError};
use tempfile::tempdir;

fn fresh_profile() -> (tempfile::TempDir, fm_storage::ProfileRoot) {
    let tmp = tempdir().unwrap();
    let root = DataRoot::new(tmp.path()).unwrap();
    let user = UserId::new("user-001").unwrap();
    let profile = root.profile(&user);
    (tmp, profile)
}

#[test]
fn rejects_empty_path() {
    let (_g, p) = fresh_profile();
    assert!(matches!(p.resolve(""), Err(StorageError::InvalidPath(_))));
}

#[test]
fn rejects_dot_dot_segments() {
    let (_g, p) = fresh_profile();
    for bad in [
        "..",
        "../",
        "../etc/passwd",
        "../../etc/passwd",
        "settings/../../../../escape",
        "a/b/../../c",
        "..\\windows",
    ] {
        let r = p.resolve(bad);
        assert!(
            matches!(r, Err(StorageError::PathTraversalDetected)),
            "should reject {bad:?}, got {r:?}"
        );
    }
}

#[test]
fn rejects_absolute_paths() {
    let (_g, p) = fresh_profile();
    let cases = [
        "/etc/passwd",
        "/",
        "C:\\Windows\\System32",
        "C:/",
        "\\\\server\\share\\x",
    ];
    for bad in cases {
        let r = p.resolve(bad);
        assert!(
            matches!(r, Err(StorageError::InvalidPath(_))),
            "should reject {bad:?}, got {r:?}"
        );
    }
}

#[test]
fn rejects_nul_bytes() {
    let (_g, p) = fresh_profile();
    let r = p.resolve("settings\0hidden");
    assert!(matches!(r, Err(StorageError::InvalidPath(_))), "got {r:?}");
}

#[test]
fn accepts_well_formed_relative_paths() {
    let (_g, p) = fresh_profile();
    let good = [
        "profile.json",
        "settings.json",
        "mappings/merchant-canonical-map.json",
        "source/uploads/2026/04/imp-001/file-meta.json",
        "analytics/monthly/2026-04.summary.json",
        "./profile.json",
        "a/./b",
    ];
    for ok in good {
        let r = p.resolve(ok);
        assert!(r.is_ok(), "should accept {ok:?}, got {r:?}");
    }
}

#[test]
fn resolved_paths_stay_under_profile_root() {
    let (_g, p) = fresh_profile();
    let resolved = p.resolve("source/uploads/2026/04/x.json").unwrap();
    assert!(
        resolved.starts_with(p.as_path()),
        "resolved {resolved:?} must be under {:?}",
        p.as_path()
    );
}

#[test]
fn different_users_get_different_roots() {
    let tmp = tempdir().unwrap();
    let root = DataRoot::new(tmp.path()).unwrap();
    let a = root.profile(&UserId::new("alice").unwrap());
    let b = root.profile(&UserId::new("bob").unwrap());
    assert_ne!(a.as_path(), b.as_path());
    let a_path = a.resolve("profile.json").unwrap();
    let b_path = b.resolve("profile.json").unwrap();
    assert_ne!(a_path, b_path);
    // Neither path resolves into the other's subtree.
    assert!(!a_path.starts_with(b.as_path()));
    assert!(!b_path.starts_with(a.as_path()));
}
