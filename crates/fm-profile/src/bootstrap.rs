use crate::error::ProfileError;
use crate::profile::{
    ProfileMeta, ProfileSettings, ProfileSummary, PROFILE_META_SCHEMA, PROFILE_SETTINGS_SCHEMA,
};
use crate::session::Session;
use fm_core::UserId;
use fm_crypto::{derive_key, generate_salt, open, seal, CryptoError, KdfParams};
use fm_storage::{DataRoot, StorageRepository, VersionedJson};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const PROFILE_FILE: &str = "profile.json";
const SETTINGS_FILE: &str = "settings.json";

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Create a fresh profile.
///
/// Steps:
/// 1. Refuse if `profile.json` already exists for `user_id`.
/// 2. Generate 16-byte salt; pick `KdfParams::recommended()`.
/// 3. Derive a 32-byte key from `passphrase` + salt via Argon2id.
/// 4. Write plaintext `profile.json` (display name, salt, KDF params).
/// 5. Seal default `ProfileSettings` and write encrypted `settings.json`.
///
/// Returns the resulting [`Session`] so the caller can avoid a follow-up unlock.
pub fn create_profile<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
    display_name: &str,
    passphrase: &[u8],
) -> Result<Session, ProfileError> {
    if storage.exists(user_id, PROFILE_FILE)? {
        return Err(ProfileError::AlreadyExists(user_id.to_string()));
    }
    let salt = generate_salt();
    let params = KdfParams::recommended();
    let key = derive_key(passphrase, &salt, params)?;

    let meta = ProfileMeta {
        user_id: user_id.clone(),
        display_name: display_name.to_string(),
        created_at: now_rfc3339(),
        salt,
        kdf_params: params,
        timezone: "Asia/Kolkata".to_string(),
        currency: "INR".to_string(),
    };
    let meta_doc = VersionedJson::new(PROFILE_META_SCHEMA, meta);
    let bytes = serde_json::to_vec_pretty(&meta_doc)?;
    storage.write(user_id, PROFILE_FILE, &bytes)?;

    let settings = ProfileSettings::default();
    let settings_doc = VersionedJson::new(PROFILE_SETTINGS_SCHEMA, settings);
    let plaintext = serde_json::to_vec(&settings_doc)?;
    let sealed = seal(&key, &plaintext)?;
    storage.write(user_id, SETTINGS_FILE, &sealed)?;

    Ok(Session::new(user_id.clone(), key))
}

/// Unlock an existing profile by passphrase.
///
/// 1. Read `profile.json` to recover salt + KDF params.
/// 2. Derive the key.
/// 3. Read and decrypt `settings.json`. AEAD authentication failure here
///    means the passphrase is wrong (or the file was tampered with).
///
/// On success returns a [`Session`] holding the derived key.
pub fn unlock_profile<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
    passphrase: &[u8],
) -> Result<Session, ProfileError> {
    if !storage.exists(user_id, PROFILE_FILE)? {
        return Err(ProfileError::NotFound(user_id.to_string()));
    }
    let meta_bytes = storage.read(user_id, PROFILE_FILE)?;
    let meta_doc: VersionedJson<ProfileMeta> =
        serde_json::from_slice(&meta_bytes).map_err(|_| ProfileError::Corrupted)?;
    if meta_doc.schema_version != PROFILE_META_SCHEMA {
        return Err(ProfileError::Corrupted);
    }
    let meta = meta_doc.data;
    let key = derive_key(passphrase, &meta.salt, meta.kdf_params)?;

    // Round-trip-verify the passphrase by decrypting settings.json.
    let sealed = storage.read(user_id, SETTINGS_FILE)?;
    match open(&key, &sealed) {
        Ok(_) => Ok(Session::new(user_id.clone(), key)),
        Err(CryptoError::AuthenticationFailed) => Err(ProfileError::WrongPassphrase),
        Err(e) => Err(ProfileError::Crypto(e)),
    }
}

/// Scan the data root and return one [`ProfileSummary`] per directory whose
/// `profile.json` parses successfully. Directories with missing or malformed
/// metadata are skipped silently — listing must never fail because one
/// profile is broken.
pub fn list_profiles(data_root: &DataRoot) -> Result<Vec<ProfileSummary>, ProfileError> {
    let users_dir = data_root.as_path().join("users");
    if !users_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&users_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let profile_path = entry.path().join(PROFILE_FILE);
        if !profile_path.is_file() {
            continue;
        }
        let bytes = match std::fs::read(&profile_path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let meta_doc: VersionedJson<ProfileMeta> = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if meta_doc.schema_version != PROFILE_META_SCHEMA {
            continue;
        }
        out.push(ProfileSummary::from(&meta_doc.data));
    }
    out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(out)
}
