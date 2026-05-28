//! Per-profile LLM categorization settings.
//!
//! Stored encrypted at `settings/llm.json`. The API key is the
//! sensitivity-bearing field; it's wrapped in the same DEK envelope as
//! every other per-profile artifact.

use crate::state::AppState;
use fm_core::UserId;
use fm_crypto::{open, seal, KeyBytes};
use fm_storage::{StorageRepository, VersionedJson};
use serde::{Deserialize, Serialize};
use tauri::State;

const LLM_CONFIG_FILE: &str = "settings/llm.json";
const LLM_CONFIG_SCHEMA: u32 = 1;

/// Default Gemini model. Cheap, fast, structured-output capable.
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.0-flash";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
}

fn default_model() -> String {
    DEFAULT_GEMINI_MODEL.to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_model(),
            api_key: String::new(),
        }
    }
}

/// What the UI receives. The API key is **redacted to a fingerprint** so
/// the configured-vs-empty state is visible without exposing the secret on
/// any IPC round-trip. The full key never leaves the backend after it's
/// stored.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigView {
    pub enabled: bool,
    pub model: String,
    /// Empty string when no key configured; otherwise `"sk-…last4"` style hint.
    pub api_key_hint: String,
    /// True when an API key is configured. Lets the UI light up the
    /// "configured" state without ever seeing the key.
    pub api_key_set: bool,
}

#[tauri::command]
pub fn get_llm_config(state: State<AppState>) -> Result<LlmConfigView, String> {
    let (user, dek) = crate::upload::session(&state)?;
    let cfg = load(&state, &user, &dek)?;
    Ok(to_view(&cfg))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigUpdate {
    pub enabled: Option<bool>,
    pub model: Option<String>,
    /// `Some("")` clears the key, `None` leaves it untouched, `Some(other)`
    /// replaces it.
    pub api_key: Option<String>,
}

#[tauri::command]
pub fn set_llm_config(
    update: LlmConfigUpdate,
    state: State<AppState>,
) -> Result<LlmConfigView, String> {
    let (user, dek) = crate::upload::session(&state)?;
    let mut cfg = load(&state, &user, &dek)?;
    // Capture change-flags up front so the audit payload survives the
    // moves below.
    let enabled_changed = update.enabled.is_some();
    let model_changed = update.model.is_some();
    let api_key_changed = update.api_key.is_some();
    let api_key_cleared = matches!(update.api_key.as_deref(), Some(""));

    if let Some(e) = update.enabled {
        cfg.enabled = e;
    }
    if let Some(m) = update.model {
        let m = m.trim();
        if !m.is_empty() {
            cfg.model = m.to_string();
        }
    }
    if let Some(k) = update.api_key {
        cfg.api_key = k;
    }
    save(&state, &user, &dek, &cfg)?;
    // Audit which fields changed — never the key value itself.
    crate::audit::record(
        &state,
        &user,
        "set_llm_config",
        None,
        serde_json::json!({
            "enabledChanged": enabled_changed,
            "enabled": cfg.enabled,
            "modelChanged": model_changed,
            "model": cfg.model,
            "apiKeyChanged": api_key_changed,
            "apiKeyCleared": api_key_cleared,
        }),
    );
    Ok(to_view(&cfg))
}

pub(crate) fn load(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
) -> Result<LlmConfig, String> {
    if !state
        .storage
        .exists(user, LLM_CONFIG_FILE)
        .map_err(|e| e.to_string())?
    {
        return Ok(LlmConfig::default());
    }
    let sealed = state
        .storage
        .read(user, LLM_CONFIG_FILE)
        .map_err(|e| e.to_string())?;
    let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
    let doc: VersionedJson<LlmConfig> =
        serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
    if doc.schema_version != LLM_CONFIG_SCHEMA {
        return Err(format!(
            "llm config has unsupported schema version {}",
            doc.schema_version
        ));
    }
    Ok(doc.data)
}

fn save(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    cfg: &LlmConfig,
) -> Result<(), String> {
    let doc = VersionedJson::new(LLM_CONFIG_SCHEMA, cfg);
    let plaintext = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    let sealed = seal(dek, &plaintext).map_err(|e| e.to_string())?;
    state
        .storage
        .write(user, LLM_CONFIG_FILE, &sealed)
        .map_err(|e| e.to_string())
}

fn to_view(cfg: &LlmConfig) -> LlmConfigView {
    let api_key_hint = if cfg.api_key.is_empty() {
        String::new()
    } else if cfg.api_key.len() <= 6 {
        "•••".to_string()
    } else {
        let last4: String = cfg
            .api_key
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("•••{last4}")
    };
    LlmConfigView {
        enabled: cfg.enabled,
        model: cfg.model.clone(),
        api_key_set: !cfg.api_key.is_empty(),
        api_key_hint,
    }
}
