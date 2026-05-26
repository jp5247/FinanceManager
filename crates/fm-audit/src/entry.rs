use serde::{Deserialize, Serialize};

/// One sealed entry as it appears in the on-disk JSONL file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub ts: String,
    pub user_id: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub entity_id: Option<String>,
    pub details: serde_json::Value,
    pub prev_hash: String,
    pub this_hash: String,
}

/// Caller-supplied event. The appender fills in `ts`, `prev_hash`, `this_hash`.
#[derive(Clone, Debug)]
pub struct EventInput {
    pub user_id: String,
    pub action: String,
    pub entity_id: Option<String>,
    pub details: serde_json::Value,
}

impl EventInput {
    pub fn new(user_id: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            action: action.into(),
            entity_id: None,
            details: serde_json::Value::Null,
        }
    }

    pub fn entity(mut self, id: impl Into<String>) -> Self {
        self.entity_id = Some(id.into());
        self
    }

    pub fn details(mut self, v: serde_json::Value) -> Self {
        self.details = v;
        self
    }
}
