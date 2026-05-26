use zeroize::{Zeroize, ZeroizeOnDrop};

pub const KEY_LEN: usize = 32;

/// 32-byte symmetric key that zeroizes its bytes on drop.
///
/// Use this for keys derived from a passphrase or freshly generated for new
/// profiles. Do NOT clone unless you have a specific need to hand the same
/// key to two owners — each clone gets its own zeroizing buffer.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct KeyBytes([u8; KEY_LEN]);

impl KeyBytes {
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for KeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print key material, even with {:?}.
        f.write_str("KeyBytes(<redacted>)")
    }
}
