use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("AEAD authentication failed — wrong key or tampered ciphertext")]
    AuthenticationFailed,

    #[error("invalid envelope: {0}")]
    InvalidEnvelope(&'static str),

    #[error("Argon2id KDF error: {0}")]
    Kdf(String),

    #[error("keystore error: {0}")]
    Keystore(String),

    #[error("OS RNG failure: {0}")]
    Rng(String),
}
