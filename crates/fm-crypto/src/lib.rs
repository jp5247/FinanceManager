//! Cryptographic primitives for FinanceManager.
//!
//! Provides:
//! - [`KeyBytes`]: a 32-byte key wrapper that zeroizes on drop.
//! - [`derive_key`]: Argon2id KDF from passphrase + salt + params.
//! - [`seal`] / [`open`]: AES-256-GCM file envelope with per-message nonce.
//! - [`keystore`]: OS-keystore wrapper (DPAPI / Keychain / Secret Service)
//!   for the convenience-unlock flow.
//!
//! ## Envelope format
//!
//! ```text
//! byte 0       version = 0x01
//! bytes 1..13  nonce (12 bytes, OsRng-generated per message)
//! bytes 13..   AES-256-GCM ciphertext concatenated with 16-byte auth tag
//! ```
//!
//! Total overhead per encrypted file: 29 bytes.
//!
//! ## Key handling
//!
//! Key material is held only in [`KeyBytes`], which zeroes on drop. The
//! envelope code derefs the key only for the duration of a single AEAD call.
//! No key is ever logged or serialized in plaintext.

#![forbid(unsafe_code)]

mod envelope;
mod error;
mod kdf;
mod key;
pub mod keystore;

pub use envelope::{open, seal, ENVELOPE_OVERHEAD, ENVELOPE_VERSION};
pub use error::CryptoError;
pub use kdf::{derive_key, generate_salt, KdfParams, Salt, SALT_LEN};
pub use key::{KeyBytes, KEY_LEN};
