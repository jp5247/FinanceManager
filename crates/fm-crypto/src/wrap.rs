use crate::envelope::{open, seal};
use crate::error::CryptoError;
use crate::key::{KeyBytes, KEY_LEN};

/// Wrap a [`KeyBytes`] under another [`KeyBytes`] (a Key-Encryption Key).
///
/// Used by the profile bootstrap to encrypt the per-profile data-encryption
/// key (DEK) under each unlock credential's derived KEK. The resulting blob
/// is a normal AEAD envelope produced by [`seal`].
pub fn wrap_key(kek: &KeyBytes, dek: &KeyBytes) -> Result<Vec<u8>, CryptoError> {
    seal(kek, dek.as_bytes())
}

/// Reverse of [`wrap_key`]. AEAD authentication failure here means the KEK
/// is wrong (i.e. the user typed the wrong passphrase or recovery phrase).
pub fn unwrap_key(kek: &KeyBytes, blob: &[u8]) -> Result<KeyBytes, CryptoError> {
    let plaintext = open(kek, blob)?;
    if plaintext.len() != KEY_LEN {
        return Err(CryptoError::InvalidEnvelope(
            "wrapped key plaintext is not 32 bytes",
        ));
    }
    let mut bytes = [0u8; KEY_LEN];
    bytes.copy_from_slice(&plaintext);
    Ok(KeyBytes::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use rand::RngCore;

    fn fresh_key() -> KeyBytes {
        let mut k = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut k);
        KeyBytes::from_bytes(k)
    }

    #[test]
    fn wrap_then_unwrap_returns_same_dek() {
        let kek = fresh_key();
        let dek = fresh_key();
        let blob = wrap_key(&kek, &dek).unwrap();
        let recovered = unwrap_key(&kek, &blob).unwrap();
        assert_eq!(recovered.as_bytes(), dek.as_bytes());
    }

    #[test]
    fn unwrap_with_wrong_kek_fails() {
        let kek = fresh_key();
        let other = fresh_key();
        let dek = fresh_key();
        let blob = wrap_key(&kek, &dek).unwrap();
        let err = unwrap_key(&other, &blob).unwrap_err();
        assert!(matches!(err, CryptoError::AuthenticationFailed));
    }

    #[test]
    fn each_wrap_produces_different_ciphertext() {
        let kek = fresh_key();
        let dek = fresh_key();
        let a = wrap_key(&kek, &dek).unwrap();
        let b = wrap_key(&kek, &dek).unwrap();
        assert_ne!(a, b, "wrap must use fresh nonce each call");
    }
}
