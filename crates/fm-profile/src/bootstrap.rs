use crate::error::ProfileError;
use crate::profile::{
    ProfileMeta, ProfileSettings, ProfileSummary, PROFILE_META_SCHEMA, PROFILE_SETTINGS_SCHEMA,
};
use crate::session::Session;
use fm_core::UserId;
use fm_crypto::{
    derive_key, generate_salt, open, seal, unwrap_key, wrap_key, CryptoError, KdfParams, KeyBytes,
    RecoveryPhrase,
};
use fm_storage::{DataRoot, StorageRepository, VersionedJson};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const PROFILE_FILE: &str = "profile.json";
const SETTINGS_FILE: &str = "settings.json";
const WRAPPED_USER_FILE: &str = "wrapped-key-user.bin";
const WRAPPED_RECOVERY_FILE: &str = "wrapped-key-recovery.bin";

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Create a fresh profile. Returns the resulting [`Session`] AND a
/// [`RecoveryPhrase`] that the caller MUST present to the user — it is the
/// only way to recover the data if the passphrase is later forgotten.
///
/// On-disk layout written by this call:
/// - `profile.json`              plaintext meta + salts + KDF params
/// - `wrapped-key-user.bin`      DEK encrypted under passphrase-KEK
/// - `wrapped-key-recovery.bin`  DEK encrypted under recovery-KEK
/// - `settings.json`             default settings encrypted under DEK
pub fn create_profile<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
    display_name: &str,
    passphrase: &[u8],
) -> Result<(Session, RecoveryPhrase), ProfileError> {
    if storage.exists(user_id, PROFILE_FILE)? {
        return Err(ProfileError::AlreadyExists(user_id.to_string()));
    }

    let params = KdfParams::recommended();

    let dek = KeyBytes::random();

    let user_salt = generate_salt();
    let user_kek = derive_key(passphrase, &user_salt, params)?;
    let wrapped_user = wrap_key(&user_kek, &dek)?;

    let recovery_phrase = RecoveryPhrase::generate();
    let recovery_salt = generate_salt();
    let recovery_kek = derive_key(recovery_phrase.as_bytes(), &recovery_salt, params)?;
    let wrapped_recovery = wrap_key(&recovery_kek, &dek)?;

    storage.write(user_id, WRAPPED_USER_FILE, &wrapped_user)?;
    storage.write(user_id, WRAPPED_RECOVERY_FILE, &wrapped_recovery)?;

    let meta = ProfileMeta {
        user_id: user_id.clone(),
        display_name: display_name.to_string(),
        created_at: now_rfc3339(),
        user_salt,
        recovery_salt,
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
    let sealed = seal(&dek, &plaintext)?;
    storage.write(user_id, SETTINGS_FILE, &sealed)?;

    Ok((Session::new(user_id.clone(), dek), recovery_phrase))
}

/// Unlock an existing profile with the user passphrase.
pub fn unlock_profile<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
    passphrase: &[u8],
) -> Result<Session, ProfileError> {
    let meta = read_meta(storage, user_id)?;
    let user_kek = derive_key(passphrase, &meta.user_salt, meta.kdf_params)?;
    unwrap_and_verify(storage, user_id, WRAPPED_USER_FILE, &user_kek, &meta, true)
}

/// Unlock an existing profile with the recovery phrase shown at creation time.
pub fn unlock_profile_with_recovery<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
    recovery_phrase: &RecoveryPhrase,
) -> Result<Session, ProfileError> {
    let meta = read_meta(storage, user_id)?;
    let recovery_kek = derive_key(
        recovery_phrase.as_bytes(),
        &meta.recovery_salt,
        meta.kdf_params,
    )?;
    unwrap_and_verify(
        storage,
        user_id,
        WRAPPED_RECOVERY_FILE,
        &recovery_kek,
        &meta,
        false,
    )
}

fn read_meta<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
) -> Result<ProfileMeta, ProfileError> {
    if !storage.exists(user_id, PROFILE_FILE)? {
        return Err(ProfileError::NotFound(user_id.to_string()));
    }
    let bytes = storage.read(user_id, PROFILE_FILE)?;
    let doc: VersionedJson<ProfileMeta> =
        serde_json::from_slice(&bytes).map_err(|_| ProfileError::Corrupted)?;
    if doc.schema_version != PROFILE_META_SCHEMA {
        return Err(ProfileError::Corrupted);
    }
    Ok(doc.data)
}

/// Unwrap the DEK and round-trip-verify by decrypting `settings.json`. If
/// `is_passphrase_path` is true, an authentication failure maps to
/// [`ProfileError::WrongPassphrase`]; otherwise to
/// [`ProfileError::WrongRecoveryPhrase`].
fn unwrap_and_verify<S: StorageRepository>(
    storage: &S,
    user_id: &UserId,
    wrapped_file: &str,
    kek: &KeyBytes,
    _meta: &ProfileMeta,
    is_passphrase_path: bool,
) -> Result<Session, ProfileError> {
    let wrong = |err: CryptoError| match err {
        CryptoError::AuthenticationFailed => {
            if is_passphrase_path {
                ProfileError::WrongPassphrase
            } else {
                ProfileError::WrongRecoveryPhrase
            }
        }
        other => ProfileError::Crypto(other),
    };

    let sealed_dek = storage.read(user_id, wrapped_file)?;
    let dek = unwrap_key(kek, &sealed_dek).map_err(wrong)?;

    // Round-trip-verify: decrypt settings.json. If the wrapped-key file was
    // tampered with such that AEAD passed but the DEK is wrong, this catches it.
    let sealed_settings = storage.read(user_id, SETTINGS_FILE)?;
    open(&dek, &sealed_settings).map_err(wrong)?;

    Ok(Session::new(user_id.clone(), dek))
}

/// Scan the data root and return one [`ProfileSummary`] per directory whose
/// `profile.json` parses successfully.
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
