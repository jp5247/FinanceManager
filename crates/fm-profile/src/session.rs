use fm_core::UserId;
use fm_crypto::KeyBytes;

/// An unlocked profile session.
///
/// Holds the derived key in memory. Drop wipes the key bytes
/// ([`KeyBytes`] is `ZeroizeOnDrop`).
///
/// Do NOT serialize, Debug-print, or log a `Session` — its inner key is the
/// secret that decrypts every file in the user's data root.
pub struct Session {
    user_id: UserId,
    key: KeyBytes,
}

impl Session {
    pub fn new(user_id: UserId, key: KeyBytes) -> Self {
        Self { user_id, key }
    }

    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn key(&self) -> &KeyBytes {
        &self.key
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id)
            .field("key", &"<redacted>")
            .finish()
    }
}
