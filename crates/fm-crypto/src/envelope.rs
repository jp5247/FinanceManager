use crate::error::CryptoError;
use crate::key::KeyBytes;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::rngs::OsRng;
use rand::RngCore;

pub const ENVELOPE_VERSION: u8 = 0x01;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 1 + NONCE_LEN; // version + nonce

/// Total overhead bytes added per envelope (version + nonce + AEAD tag).
pub const ENVELOPE_OVERHEAD: usize = HEADER_LEN + TAG_LEN;

/// Encrypt `plaintext` under `key` and return the on-disk envelope blob.
pub fn seal(key: &KeyBytes, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(key.as_bytes().into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::AuthenticationFailed)?;

    let mut out = Vec::with_capacity(HEADER_LEN + ct.len());
    out.push(ENVELOPE_VERSION);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt an envelope blob produced by [`seal`]. Returns the plaintext.
///
/// Fails with [`CryptoError::AuthenticationFailed`] if the key is wrong or
/// any byte of the blob has been tampered with.
pub fn open(key: &KeyBytes, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < HEADER_LEN + TAG_LEN {
        return Err(CryptoError::InvalidEnvelope("blob too short"));
    }
    if blob[0] != ENVELOPE_VERSION {
        return Err(CryptoError::InvalidEnvelope("unsupported version byte"));
    }
    let nonce = Nonce::from_slice(&blob[1..HEADER_LEN]);
    let ct = &blob[HEADER_LEN..];
    let cipher = Aes256Gcm::new(key.as_bytes().into());
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| CryptoError::AuthenticationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_key() -> KeyBytes {
        let mut k = [0u8; 32];
        OsRng.fill_bytes(&mut k);
        KeyBytes::from_bytes(k)
    }

    #[test]
    fn round_trip() {
        let key = fresh_key();
        let pt = b"hello FinanceManager".to_vec();
        let blob = seal(&key, &pt).unwrap();
        assert!(blob.len() >= pt.len() + ENVELOPE_OVERHEAD);
        let back = open(&key, &blob).unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn empty_plaintext_round_trips() {
        let key = fresh_key();
        let blob = seal(&key, b"").unwrap();
        assert_eq!(open(&key, &blob).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn distinct_nonces_produce_distinct_ciphertexts() {
        let key = fresh_key();
        let a = seal(&key, b"same plaintext").unwrap();
        let b = seal(&key, b"same plaintext").unwrap();
        assert_ne!(
            a, b,
            "two seals of identical plaintext must use different nonces"
        );
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = fresh_key();
        let mut blob = seal(&key, b"sensitive payload").unwrap();
        // Flip a byte somewhere inside the ciphertext+tag region.
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        let err = open(&key, &blob).unwrap_err();
        assert!(matches!(err, CryptoError::AuthenticationFailed));
    }

    #[test]
    fn tampered_nonce_is_rejected() {
        let key = fresh_key();
        let mut blob = seal(&key, b"x").unwrap();
        blob[5] ^= 0x42;
        let err = open(&key, &blob).unwrap_err();
        assert!(matches!(err, CryptoError::AuthenticationFailed));
    }

    #[test]
    fn wrong_key_is_rejected() {
        let key = fresh_key();
        let other = fresh_key();
        let blob = seal(&key, b"x").unwrap();
        let err = open(&other, &blob).unwrap_err();
        assert!(matches!(err, CryptoError::AuthenticationFailed));
    }

    #[test]
    fn short_blob_is_invalid() {
        let key = fresh_key();
        let err = open(&key, &[0x01, 0x00, 0x00]).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidEnvelope(_)));
    }

    #[test]
    fn unknown_version_is_invalid() {
        let key = fresh_key();
        let mut blob = seal(&key, b"x").unwrap();
        blob[0] = 0xFF;
        let err = open(&key, &blob).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidEnvelope(_)));
    }
}
