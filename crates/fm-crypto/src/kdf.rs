use crate::error::CryptoError;
use crate::key::{KeyBytes, KEY_LEN};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

pub const SALT_LEN: usize = 16;

/// 16-byte random salt. Safe to store on disk alongside the sealed key.
///
/// Serializes to/from a 32-char lowercase hex string so `profile.json` stays
/// human-inspectable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Salt(pub [u8; SALT_LEN]);

impl Salt {
    pub fn to_hex(&self) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(SALT_LEN * 2);
        for b in self.0 {
            write!(s, "{b:02x}").expect("write to String never fails");
        }
        s
    }

    pub fn from_hex(s: &str) -> Result<Self, CryptoError> {
        if s.len() != SALT_LEN * 2 {
            return Err(CryptoError::InvalidEnvelope("salt hex must be 32 chars"));
        }
        let mut out = [0u8; SALT_LEN];
        for (i, byte_out) in out.iter_mut().enumerate() {
            let pair = &s[i * 2..i * 2 + 2];
            *byte_out = u8::from_str_radix(pair, 16)
                .map_err(|_| CryptoError::InvalidEnvelope("salt hex contains non-hex"))?;
        }
        Ok(Salt(out))
    }
}

impl TryFrom<String> for Salt {
    type Error = CryptoError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_hex(&s)
    }
}

impl From<Salt> for String {
    fn from(s: Salt) -> Self {
        s.to_hex()
    }
}

pub fn generate_salt() -> Salt {
    let mut s = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut s);
    Salt(s)
}

/// Argon2id cost parameters. Defaults target ~500 ms on a 2020-class laptop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m_cost: u32,
    /// Iterations.
    pub t_cost: u32,
    /// Parallelism lanes.
    pub p_cost: u32,
}

impl KdfParams {
    pub const fn recommended() -> Self {
        Self {
            m_cost: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }

    /// Faster, weaker params for unit tests. Do not use in production.
    pub const fn for_tests() -> Self {
        Self {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::recommended()
    }
}

/// Derive a 32-byte key from a passphrase + salt using Argon2id.
pub fn derive_key(
    passphrase: &[u8],
    salt: &Salt,
    params: KdfParams,
) -> Result<KeyBytes, CryptoError> {
    let argon_params = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(KEY_LEN))
        .map_err(|e| CryptoError::Kdf(format!("invalid params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(passphrase, &salt.0, &mut out)
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;
    Ok(KeyBytes::from_bytes(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_inputs() {
        let salt = Salt([7u8; SALT_LEN]);
        let p = KdfParams::for_tests();
        let k1 = derive_key(b"correct horse", &salt, p).unwrap();
        let k2 = derive_key(b"correct horse", &salt, p).unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn different_salts_yield_different_keys() {
        let p = KdfParams::for_tests();
        let k1 = derive_key(b"same passphrase", &Salt([1; SALT_LEN]), p).unwrap();
        let k2 = derive_key(b"same passphrase", &Salt([2; SALT_LEN]), p).unwrap();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn different_passphrases_yield_different_keys() {
        let salt = Salt([3u8; SALT_LEN]);
        let p = KdfParams::for_tests();
        let k1 = derive_key(b"alpha", &salt, p).unwrap();
        let k2 = derive_key(b"beta", &salt, p).unwrap();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn generate_salt_produces_random_bytes() {
        let a = generate_salt();
        let b = generate_salt();
        // Astronomically unlikely to collide.
        assert_ne!(a.0, b.0);
    }
}
