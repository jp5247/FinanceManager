use fm_core::UserId;
use fm_crypto::{KdfParams, Salt};
use serde::{Deserialize, Serialize};

/// Schema version for `profile.json`.
///
/// v2 added `recovery_salt` and renamed `salt` to `user_salt` when the
/// key-wrapping refactor landed. v1 profiles are not migrated automatically;
/// pre-release users must recreate them.
pub const PROFILE_META_SCHEMA: u32 = 2;

/// Schema version for the encrypted `settings.json`.
pub const PROFILE_SETTINGS_SCHEMA: u32 = 1;

/// Plaintext identity metadata stored at `profile.json`.
///
/// Must be readable before the user has typed their passphrase, so it cannot
/// be encrypted. Contains no PII — display name, locale, and the salt + KDF
/// parameters needed to derive the unlock key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMeta {
    pub user_id: UserId,
    pub display_name: String,
    pub created_at: String,
    /// Salt for the Argon2id derivation of the passphrase-KEK.
    pub user_salt: Salt,
    /// Salt for the Argon2id derivation of the recovery-phrase-KEK.
    pub recovery_salt: Salt,
    pub kdf_params: KdfParams,
    pub timezone: String,
    pub currency: String,
}

/// User-tunable preferences stored encrypted at `settings.json`.
///
/// Defaults reflect the locked Phase-0 decisions:
/// - `encryption_enabled = true`   (OD-4 revised)
/// - `internet_lookup_enabled = false` (OD-5 default off)
/// - `require_manual_approval_for_lookup = true`
/// - `dashboard_active_window_months = 2`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSettings {
    pub encryption_enabled: bool,
    pub internet_lookup_enabled: bool,
    pub require_manual_approval_for_lookup: bool,
    pub dashboard_active_window_months: u32,
    pub allow_flag_bypass_with_reason: bool,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            encryption_enabled: true,
            internet_lookup_enabled: false,
            require_manual_approval_for_lookup: true,
            dashboard_active_window_months: 2,
            allow_flag_bypass_with_reason: true,
        }
    }
}

/// Lightweight identity record used by [`list_profiles`](crate::list_profiles)
/// — no salt or KDF params, just what the picker UI needs to show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub user_id: UserId,
    pub display_name: String,
    pub created_at: String,
}

impl From<&ProfileMeta> for ProfileSummary {
    fn from(m: &ProfileMeta) -> Self {
        Self {
            user_id: m.user_id.clone(),
            display_name: m.display_name.clone(),
            created_at: m.created_at.clone(),
        }
    }
}
