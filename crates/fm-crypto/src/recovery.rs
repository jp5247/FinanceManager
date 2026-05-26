use crate::error::CryptoError;
use rand::rngs::OsRng;
use rand::RngCore;
use std::fmt::{self, Write as FmtWrite};
use zeroize::Zeroize;

/// 12 raw bytes → 96 bits of entropy.
const RECOVERY_BYTES: usize = 12;
/// 24 hex chars + 5 dashes between 6 groups of 4 = 29 chars.
const FORMATTED_LEN: usize = 24 + 5;

/// A 96-bit recovery secret shown once at profile creation. Used as the
/// passphrase input to Argon2id to derive the recovery KEK that wraps the
/// data-encryption key.
///
/// Displayed format: six groups of four lowercase hex chars separated by
/// dashes, e.g. `a3f2-1e4b-7c8d-9012-34ab-cdef`. Parsing accepts the same
/// input with or without dashes and in either case.
///
/// The struct zeroizes its inner bytes on drop. Do not log or
/// `format!("{:?}", phrase)` — `Debug` is redacted.
#[derive(Clone)]
pub struct RecoveryPhrase {
    bytes: [u8; RECOVERY_BYTES],
}

impl RecoveryPhrase {
    /// Generate a fresh recovery phrase using the OS RNG.
    pub fn generate() -> Self {
        let mut b = [0u8; RECOVERY_BYTES];
        OsRng.fill_bytes(&mut b);
        Self { bytes: b }
    }

    /// Format as `xxxx-xxxx-xxxx-xxxx-xxxx-xxxx` (lowercase hex).
    pub fn to_display_string(&self) -> String {
        let mut s = String::with_capacity(FORMATTED_LEN);
        for (i, b) in self.bytes.iter().enumerate() {
            if i > 0 && i % 2 == 0 {
                s.push('-');
            }
            write!(s, "{b:02x}").expect("write to String never fails");
        }
        s
    }

    /// Parse a user-entered recovery phrase. Accepts upper/lowercase and
    /// strips ASCII whitespace + dashes before decoding.
    pub fn parse(input: &str) -> Result<Self, CryptoError> {
        let cleaned: String = input
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '-')
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if cleaned.len() != RECOVERY_BYTES * 2 {
            return Err(CryptoError::InvalidEnvelope(
                "recovery phrase must be 24 hex characters",
            ));
        }
        let mut out = [0u8; RECOVERY_BYTES];
        for (i, byte_out) in out.iter_mut().enumerate() {
            let pair = &cleaned[i * 2..i * 2 + 2];
            *byte_out = u8::from_str_radix(pair, 16)
                .map_err(|_| CryptoError::InvalidEnvelope("recovery phrase has non-hex chars"))?;
        }
        Ok(Self { bytes: out })
    }

    /// The raw secret bytes — feed to Argon2id as the passphrase input.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for RecoveryPhrase {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl fmt::Debug for RecoveryPhrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryPhrase(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_has_expected_shape() {
        let p = RecoveryPhrase {
            bytes: [
                0xa3, 0xf2, 0x1e, 0x4b, 0x7c, 0x8d, 0x90, 0x12, 0x34, 0xab, 0xcd, 0xef,
            ],
        };
        assert_eq!(p.to_display_string(), "a3f2-1e4b-7c8d-9012-34ab-cdef");
        assert_eq!(p.to_display_string().len(), FORMATTED_LEN);
    }

    #[test]
    fn parse_round_trip() {
        let p = RecoveryPhrase::generate();
        let s = p.to_display_string();
        let back = RecoveryPhrase::parse(&s).unwrap();
        assert_eq!(p.bytes, back.bytes);
    }

    #[test]
    fn parse_accepts_no_dashes_and_mixed_case() {
        let plain = "A3F21E4B7C8D901234ABCDEF";
        let p1 = RecoveryPhrase::parse(plain).unwrap();
        let p2 = RecoveryPhrase::parse("a3f2-1e4b-7c8d-9012-34ab-cdef").unwrap();
        let p3 = RecoveryPhrase::parse(" a3f2  1e4b 7c8d 9012 34ab cdef ").unwrap();
        assert_eq!(p1.bytes, p2.bytes);
        assert_eq!(p2.bytes, p3.bytes);
    }

    #[test]
    fn parse_rejects_wrong_length() {
        assert!(RecoveryPhrase::parse("a3f2").is_err());
        assert!(RecoveryPhrase::parse(&"a".repeat(23)).is_err());
        assert!(RecoveryPhrase::parse(&"a".repeat(25)).is_err());
    }

    #[test]
    fn parse_rejects_non_hex() {
        assert!(RecoveryPhrase::parse("zzzz-zzzz-zzzz-zzzz-zzzz-zzzz").is_err());
    }

    #[test]
    fn two_generations_differ() {
        let a = RecoveryPhrase::generate();
        let b = RecoveryPhrase::generate();
        assert_ne!(a.bytes, b.bytes);
    }

    #[test]
    fn debug_is_redacted() {
        let p = RecoveryPhrase::generate();
        assert_eq!(format!("{p:?}"), "RecoveryPhrase(<redacted>)");
    }
}
