//! User-saved categorization rules.
//!
//! Persisted per profile (encrypted) at `mappings/category-rules.json`.
//! Created via the "Save as rule" path inside [`crate::upload::recategorize_transaction`]
//! and managed via the commands in this module.

use crate::state::AppState;
use fm_categorize::{StoredMatchType, StoredRule};
use fm_core::UserId;
use fm_crypto::{open, seal, KeyBytes};
use fm_storage::{StorageRepository, VersionedJson};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tauri::State;

const USER_RULES_FILE: &str = "mappings/category-rules.json";
const USER_RULES_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRulesDoc {
    #[serde(default)]
    pub rules: Vec<StoredRule>,
}

/// What the UI sends when the user saves a new rule via the recategorize
/// modal.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRuleSpec {
    pub match_type: StoredMatchType,
    pub match_value: String,
    pub category: String,
}

#[tauri::command]
pub fn list_user_rules(state: State<AppState>) -> Result<Vec<StoredRule>, String> {
    let (user, dek) = crate::upload::session(&state)?;
    Ok(load_rules(&state, &user, &dek)?.rules)
}

#[tauri::command]
pub fn delete_user_rule(
    rule_id: String,
    state: State<AppState>,
) -> Result<Vec<StoredRule>, String> {
    let (user, dek) = crate::upload::session(&state)?;
    let mut doc = load_rules(&state, &user, &dek)?;
    let before = doc.rules.len();
    doc.rules.retain(|r| r.id != rule_id);
    if doc.rules.len() == before {
        return Err(format!("rule {rule_id} not found"));
    }
    save_rules(&state, &user, &dek, &doc)?;
    // Opaque entity id; no rule body in details (pattern is raw merchant
    // string — PII per the audit-redaction policy).
    crate::audit::record(
        &state,
        &user,
        "delete_user_rule",
        Some(&rule_id),
        serde_json::Value::Null,
    );
    Ok(doc.rules)
}

/// Load the user rules doc, returning an empty default if the file doesn't
/// exist yet.
pub(crate) fn load_rules(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
) -> Result<UserRulesDoc, String> {
    if !state
        .storage
        .exists(user, USER_RULES_FILE)
        .map_err(|e| e.to_string())?
    {
        return Ok(UserRulesDoc::default());
    }
    let sealed = state
        .storage
        .read(user, USER_RULES_FILE)
        .map_err(|e| e.to_string())?;
    let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
    let parsed: VersionedJson<UserRulesDoc> =
        serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
    if parsed.schema_version != USER_RULES_SCHEMA {
        return Err(format!(
            "user rules file has unsupported schema version {}",
            parsed.schema_version
        ));
    }
    Ok(parsed.data)
}

pub(crate) fn save_rules(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    doc: &UserRulesDoc,
) -> Result<(), String> {
    let envelope = VersionedJson::new(USER_RULES_SCHEMA, doc);
    let plaintext = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
    let sealed = seal(dek, &plaintext).map_err(|e| e.to_string())?;
    state
        .storage
        .write(user, USER_RULES_FILE, &sealed)
        .map_err(|e| e.to_string())
}

/// Append a new rule to the user rules file. Returns the persisted
/// [`StoredRule`] with its assigned id + timestamp.
pub(crate) fn append_rule(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    spec: NewRuleSpec,
) -> Result<StoredRule, String> {
    let mut doc = load_rules(state, user, dek)?;
    let rule = StoredRule {
        id: make_rule_id(),
        priority: fm_categorize::USER_RULE_PRIORITY,
        match_type: spec.match_type,
        match_value: spec.match_value.trim().to_string(),
        category: spec.category.trim().to_string(),
        confidence: 0.99,
        created_at: now_rfc3339(),
    };
    if rule.match_value.is_empty() || rule.category.is_empty() {
        return Err("match value and category are required".to_string());
    }
    doc.rules.push(rule.clone());
    save_rules(state, user, dek, &doc)?;
    Ok(rule)
}

fn make_rule_id() -> String {
    let mut rnd = [0u8; 6];
    OsRng.fill_bytes(&mut rnd);
    format!(
        "user:{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        rnd[0], rnd[1], rnd[2], rnd[3], rnd[4], rnd[5]
    )
}

fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
