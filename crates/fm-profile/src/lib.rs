//! Profile lifecycle for FinanceManager.
//!
//! Orchestrates [`fm-storage`](fm_storage) and [`fm-crypto`](fm_crypto) to
//! create, unlock, and list local user profiles. This is the layer where the
//! foundation crates become product behavior the UI can call.
//!
//! ## On-disk layout
//!
//! Per user (`data/users/<userId>/`):
//!
//! | File | Encrypted? | Purpose |
//! |---|---|---|
//! | `profile.json`  | no  | Identity + salt + KDF params. Read at app startup. |
//! | `settings.json` | yes | User-tunable preferences. Reading it round-trip-verifies the passphrase. |
//!
//! Encryption uses the profile's per-passphrase key (Argon2id → 32-byte → AES-256-GCM).
//! `profile.json` is plaintext because it must be readable before the user
//! has typed their passphrase, and it contains no PII — only display name,
//! salt, and KDF params.

#![forbid(unsafe_code)]

mod bootstrap;
mod error;
mod profile;
mod session;

pub use bootstrap::{create_profile, list_profiles, unlock_profile};
pub use error::ProfileError;
pub use profile::{
    ProfileMeta, ProfileSettings, ProfileSummary, PROFILE_META_SCHEMA, PROFILE_SETTINGS_SCHEMA,
};
pub use session::Session;
