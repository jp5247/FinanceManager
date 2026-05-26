//! Per-profile merchant-categorization cache.
//!
//! Once a merchant has been classified by the LLM (or any future external
//! source), the result is cached locally so future uploads skip the round
//! trip. Stored encrypted at `mappings/merchant-cache.json`.

use crate::llm::Direction;
use crate::state::AppState;
use fm_core::UserId;
use fm_crypto::{open, seal, KeyBytes};
use fm_storage::{StorageRepository, VersionedJson};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CACHE_FILE: &str = "mappings/merchant-cache.json";
const CACHE_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerchantCacheEntry {
    pub category: String,
    pub source: String,
    pub looked_up_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerchantCacheDoc {
    #[serde(default)]
    pub entries: HashMap<String, MerchantCacheEntry>,
}

/// Cache key includes the direction (debit / credit) because the same
/// merchant string can land in different categories depending on which way
/// money is flowing — e.g. INDIAN RAILWAY incoming is a Dividend (IRFC),
/// outgoing is Train Travel. A direction-blind key would re-introduce that
/// false-positive class.
pub(crate) fn cache_key(merchant: &str, direction: Direction) -> String {
    let dir = match direction {
        Direction::Debit => 'd',
        Direction::Credit => 'c',
    };
    format!("{dir}|{}", merchant.trim().to_lowercase())
}

pub(crate) fn load(
    state: &tauri::State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
) -> Result<MerchantCacheDoc, String> {
    if !state
        .storage
        .exists(user, CACHE_FILE)
        .map_err(|e| e.to_string())?
    {
        return Ok(MerchantCacheDoc::default());
    }
    let sealed = state
        .storage
        .read(user, CACHE_FILE)
        .map_err(|e| e.to_string())?;
    let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
    let doc: VersionedJson<MerchantCacheDoc> =
        serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
    if doc.schema_version != CACHE_SCHEMA {
        return Err(format!(
            "merchant cache has unsupported schema version {}",
            doc.schema_version
        ));
    }
    Ok(doc.data)
}

pub(crate) fn save(
    state: &tauri::State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    doc: &MerchantCacheDoc,
) -> Result<(), String> {
    let envelope = VersionedJson::new(CACHE_SCHEMA, doc);
    let plaintext = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
    let sealed = seal(dek, &plaintext).map_err(|e| e.to_string())?;
    state
        .storage
        .write(user, CACHE_FILE, &sealed)
        .map_err(|e| e.to_string())
}

pub(crate) fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_normalizes_case_and_whitespace() {
        assert_eq!(
            cache_key("SWIGGY INSTAMART", Direction::Debit),
            "d|swiggy instamart"
        );
        assert_eq!(cache_key("  Amazon  ", Direction::Debit), "d|amazon");
    }

    #[test]
    fn key_separates_directions_for_same_merchant() {
        // Regression for the IRFC / Indian Railway false-positive class: a
        // credit hit on this merchant must not poison the debit-side lookup.
        let debit = cache_key("INDIAN RAILWAY", Direction::Debit);
        let credit = cache_key("INDIAN RAILWAY", Direction::Credit);
        assert_ne!(debit, credit);
        assert!(debit.starts_with("d|"));
        assert!(credit.starts_with("c|"));
    }
}
