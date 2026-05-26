//! OS-keystore wrapper for the convenience-unlock flow.
//!
//! - **Windows**: Credential Manager (DPAPI under the hood).
//! - **macOS**:   Keychain.
//! - **Linux**:   Secret Service (gnome-keyring / kwallet).
//!
//! The keystore stores a small sealed-key blob (the AES key wrapped under a
//! convenience secret, not the passphrase). Loss of the keystore entry does
//! NOT lock the user out — they can still unlock via passphrase.
//!
//! These functions touch the real OS keystore. Tests that exercise them are
//! marked `#[ignore]` so `cargo test` skips them in CI; run with
//! `cargo test -- --ignored` locally to verify.
//!
//! Item identity is `(service, account)` per the cross-platform `keyring`
//! convention. Recommended naming: service = "dev.financemanager.app",
//! account = `<userId>`.

use crate::error::CryptoError;
use keyring::Entry;

fn entry(service: &str, account: &str) -> Result<Entry, CryptoError> {
    Entry::new(service, account).map_err(|e| CryptoError::Keystore(e.to_string()))
}

/// Store a sealed-key blob under `(service, account)`. Overwrites any
/// existing entry.
pub fn store(service: &str, account: &str, blob: &[u8]) -> Result<(), CryptoError> {
    entry(service, account)?
        .set_secret(blob)
        .map_err(|e| CryptoError::Keystore(e.to_string()))
}

/// Retrieve a previously-stored sealed-key blob.
///
/// Returns `Ok(None)` if the entry does not exist (not an error — caller
/// falls back to passphrase). Returns `Err` only on platform / permission
/// problems.
pub fn load(service: &str, account: &str) -> Result<Option<Vec<u8>>, CryptoError> {
    match entry(service, account)?.get_secret() {
        Ok(b) => Ok(Some(b)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(CryptoError::Keystore(e.to_string())),
    }
}

/// Delete the entry. Idempotent — missing entry is not an error.
pub fn delete(service: &str, account: &str) -> Result<(), CryptoError> {
    match entry(service, account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CryptoError::Keystore(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SERVICE: &str = "dev.financemanager.app.tests";

    #[test]
    #[ignore = "touches real OS keystore; run with `cargo test -- --ignored`"]
    fn store_load_delete_cycle() {
        let account = format!("test-{}", std::process::id());
        let payload: &[u8] = b"sealed-key-blob";

        store(TEST_SERVICE, &account, payload).unwrap();
        let got = load(TEST_SERVICE, &account)
            .unwrap()
            .expect("entry present");
        assert_eq!(got, payload);

        delete(TEST_SERVICE, &account).unwrap();
        assert!(load(TEST_SERVICE, &account).unwrap().is_none());
    }

    #[test]
    #[ignore = "touches real OS keystore; run with `cargo test -- --ignored`"]
    fn delete_missing_is_noop() {
        let account = format!("never-stored-{}", std::process::id());
        delete(TEST_SERVICE, &account).unwrap();
    }

    #[test]
    #[ignore = "touches real OS keystore; run with `cargo test -- --ignored`"]
    fn load_missing_returns_none() {
        let account = format!("never-stored-{}", std::process::id());
        assert!(load(TEST_SERVICE, &account).unwrap().is_none());
    }
}
