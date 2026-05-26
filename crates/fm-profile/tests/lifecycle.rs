//! End-to-end profile lifecycle tests.

use fm_core::UserId;
use fm_crypto::RecoveryPhrase;
use fm_profile::{
    create_profile, list_profiles, unlock_profile, unlock_profile_with_recovery, ProfileError,
    ProfileSettings, PROFILE_SETTINGS_SCHEMA,
};
use fm_storage::{DataRoot, FilesystemStorage, StorageRepository, VersionedJson};
use tempfile::tempdir;

fn fresh_env() -> (tempfile::TempDir, DataRoot, FilesystemStorage) {
    let tmp = tempdir().unwrap();
    let root = DataRoot::new(tmp.path()).unwrap();
    let storage = FilesystemStorage::new(root.clone());
    (tmp, root, storage)
}

#[test]
fn create_returns_recovery_phrase_and_session() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    let (session, recovery) = create_profile(&storage, &user, "Asha", b"correct horse").unwrap();
    assert_eq!(session.user_id(), &user);
    // Recovery phrase is non-empty and round-trips through parse.
    let parsed = RecoveryPhrase::parse(&recovery.to_display_string()).unwrap();
    assert_eq!(parsed.as_bytes(), recovery.as_bytes());
}

#[test]
fn unlock_with_passphrase_round_trip() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    let (s1, _r) = create_profile(&storage, &user, "Asha", b"correct horse").unwrap();
    let s2 = unlock_profile(&storage, &user, b"correct horse").unwrap();
    // Both sessions should be able to decrypt the same data — verify the
    // underlying DEK bytes match.
    assert_eq!(s1.key().as_bytes(), s2.key().as_bytes());
}

#[test]
fn unlock_with_recovery_phrase_round_trip() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    let (s1, recovery) = create_profile(&storage, &user, "Asha", b"correct horse").unwrap();
    let s2 = unlock_profile_with_recovery(&storage, &user, &recovery).unwrap();
    assert_eq!(s1.key().as_bytes(), s2.key().as_bytes());
}

#[test]
fn unlock_with_wrong_passphrase_fails() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    create_profile(&storage, &user, "Asha", b"correct horse").unwrap();
    let err = unlock_profile(&storage, &user, b"wrong horse").unwrap_err();
    assert!(matches!(err, ProfileError::WrongPassphrase), "got {err:?}");
}

#[test]
fn unlock_with_wrong_recovery_phrase_fails() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    create_profile(&storage, &user, "Asha", b"correct horse").unwrap();
    let wrong = RecoveryPhrase::generate();
    let err = unlock_profile_with_recovery(&storage, &user, &wrong).unwrap_err();
    assert!(
        matches!(err, ProfileError::WrongRecoveryPhrase),
        "got {err:?}"
    );
}

#[test]
fn recovery_phrase_can_decrypt_settings() {
    use fm_crypto::open;
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    let (_s, recovery) = create_profile(&storage, &user, "Asha", b"x").unwrap();
    let s = unlock_profile_with_recovery(&storage, &user, &recovery).unwrap();
    let sealed = storage.read(&user, "settings.json").unwrap();
    let plaintext = open(s.key(), &sealed).unwrap();
    let doc: VersionedJson<ProfileSettings> = serde_json::from_slice(&plaintext).unwrap();
    assert_eq!(doc.schema_version, PROFILE_SETTINGS_SCHEMA);
    assert_eq!(doc.data, ProfileSettings::default());
}

#[test]
fn create_refuses_duplicate() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("user-001").unwrap();
    create_profile(&storage, &user, "Asha", b"x").unwrap();
    let err = create_profile(&storage, &user, "Asha", b"x").unwrap_err();
    assert!(matches!(err, ProfileError::AlreadyExists(_)), "got {err:?}");
}

#[test]
fn unlock_nonexistent_profile_fails() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("ghost").unwrap();
    let err = unlock_profile(&storage, &user, b"x").unwrap_err();
    assert!(matches!(err, ProfileError::NotFound(_)), "got {err:?}");
}

#[test]
fn list_returns_created_profiles() {
    let (_g, root, storage) = fresh_env();
    let a = UserId::new("alice").unwrap();
    let b = UserId::new("bob").unwrap();
    create_profile(&storage, &a, "Alice", b"x").unwrap();
    create_profile(&storage, &b, "Bob", b"y").unwrap();

    let summaries = list_profiles(&root).unwrap();
    assert_eq!(summaries.len(), 2);
    let ids: Vec<_> = summaries.iter().map(|s| s.user_id.as_str()).collect();
    assert!(ids.contains(&"alice"));
    assert!(ids.contains(&"bob"));
}

#[test]
fn list_empty_when_no_users_dir() {
    let (_g, root, _storage) = fresh_env();
    let summaries = list_profiles(&root).unwrap();
    assert!(summaries.is_empty());
}

#[test]
fn list_skips_corrupted_profiles() {
    let (_g, root, storage) = fresh_env();
    let good = UserId::new("good").unwrap();
    create_profile(&storage, &good, "Good", b"x").unwrap();

    let bad_dir = root.as_path().join("users").join("bad");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("profile.json"), b"not json").unwrap();

    let summaries = list_profiles(&root).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].user_id.as_str(), "good");
}

#[test]
fn profile_json_is_not_encrypted_and_contains_both_salts() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("u").unwrap();
    create_profile(&storage, &user, "Display Name", b"x").unwrap();
    let bytes = storage.read(&user, "profile.json").unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed["data"]["displayName"], "Display Name");
    assert!(parsed["data"]["userSalt"].is_string());
    assert!(parsed["data"]["recoverySalt"].is_string());
    assert_ne!(parsed["data"]["userSalt"], parsed["data"]["recoverySalt"]);
}

#[test]
fn wrapped_key_files_are_distinct() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("u").unwrap();
    create_profile(&storage, &user, "Asha", b"x").unwrap();
    let wu = storage.read(&user, "wrapped-key-user.bin").unwrap();
    let wr = storage.read(&user, "wrapped-key-recovery.bin").unwrap();
    // Two envelopes around the SAME DEK, but with different KEKs and fresh
    // nonces — must not be byte-equal.
    assert_ne!(wu, wr);
    assert_eq!(wu[0], 0x01, "envelope version byte");
    assert_eq!(wr[0], 0x01, "envelope version byte");
}

#[test]
fn settings_json_is_encrypted_and_unreadable_as_plaintext() {
    let (_g, _root, storage) = fresh_env();
    let user = UserId::new("u").unwrap();
    create_profile(&storage, &user, "Asha", b"x").unwrap();
    let bytes = storage.read(&user, "settings.json").unwrap();
    assert_eq!(bytes[0], 0x01);
    assert!(serde_json::from_slice::<serde_json::Value>(&bytes).is_err());
}
