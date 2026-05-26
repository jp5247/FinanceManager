//! Cryptographic primitives for FinanceManager.
//!
//! Provides the at-rest envelope (AES-256-GCM per file), the passphrase KDF
//! (Argon2id), and the OS-keystore wrapper used for convenience unlock
//! (Windows DPAPI / macOS Keychain / Linux Secret Service).
//!
//! Key material is zeroized on drop. No key ever leaves this crate.

#![forbid(unsafe_code)]
