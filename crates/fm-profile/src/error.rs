use fm_core::InvalidIdError;
use fm_crypto::CryptoError;
use fm_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("profile already exists for user {0}")]
    AlreadyExists(String),

    #[error("profile not found for user {0}")]
    NotFound(String),

    #[error("incorrect passphrase")]
    WrongPassphrase,

    #[error("incorrect recovery phrase")]
    WrongRecoveryPhrase,

    #[error("profile file is corrupted or written by an incompatible version")]
    Corrupted,

    #[error("invalid user id: {0}")]
    InvalidUserId(#[from] InvalidIdError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
