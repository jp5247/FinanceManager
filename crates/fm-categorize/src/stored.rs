//! Serializable rule representation for persistence on disk.
//!
//! [`Rule`] holds a compiled [`Regex`] which cannot be serialized. This
//! module provides [`StoredRule`] — pure data — that round-trips through
//! JSON and converts to a runtime [`Rule`] on demand.

use crate::rule::{MatchType, Rule};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredMatchType {
    Contains,
    Regex,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRule {
    pub id: String,
    pub priority: i32,
    pub match_type: StoredMatchType,
    pub match_value: String,
    pub category: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    pub created_at: String,
}

fn default_confidence() -> f32 {
    0.9
}

#[derive(Debug)]
pub struct StoredRuleError(pub String);

impl std::fmt::Display for StoredRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid stored rule: {}", self.0)
    }
}

impl std::error::Error for StoredRuleError {}

impl StoredRule {
    /// Compile this stored rule into a runtime [`Rule`]. Fails if the
    /// `match_value` is an invalid regex (Contains never fails).
    pub fn to_runtime(&self) -> Result<Rule, StoredRuleError> {
        let matcher = match self.match_type {
            StoredMatchType::Contains => MatchType::Contains(self.match_value.clone()),
            StoredMatchType::Regex => MatchType::Regex(
                Regex::new(&self.match_value)
                    .map_err(|e| StoredRuleError(format!("regex `{}`: {e}", self.match_value)))?,
            ),
        };
        Ok(Rule {
            id: self.id.clone(),
            priority: self.priority,
            matcher,
            category: self.category.clone(),
            confidence: self.confidence,
        })
    }
}

/// Compile a slice of [`StoredRule`]s into runtime rules. Bad rules are
/// skipped silently — a malformed user rule shouldn't break categorization
/// for all the others.
pub fn compile_stored(rules: &[StoredRule]) -> Vec<Rule> {
    rules.iter().filter_map(|r| r.to_runtime().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_contains() {
        let s = StoredRule {
            id: "user:abc".into(),
            priority: 1000,
            match_type: StoredMatchType::Contains,
            match_value: "swiggy".into(),
            category: "Food Delivery".into(),
            confidence: 0.9,
            created_at: "2026-05-26T12:00:00Z".into(),
        };
        let r = s.to_runtime().unwrap();
        assert_eq!(r.id, "user:abc");
        assert_eq!(r.category, "Food Delivery");
        assert_eq!(r.priority, 1000);
        match &r.matcher {
            MatchType::Contains(v) => assert_eq!(v, "swiggy"),
            _ => panic!("expected Contains"),
        }
    }

    #[test]
    fn round_trips_regex_compiles_pattern() {
        let s = StoredRule {
            id: "user:rx".into(),
            priority: 800,
            match_type: StoredMatchType::Regex,
            match_value: r"(?i)\bUPI/RENT\b".into(),
            category: "Rent".into(),
            confidence: 0.95,
            created_at: "2026-05-26T12:00:00Z".into(),
        };
        let r = s.to_runtime().unwrap();
        match &r.matcher {
            MatchType::Regex(_) => {}
            _ => panic!("expected Regex"),
        }
    }

    #[test]
    fn invalid_regex_returns_error() {
        let s = StoredRule {
            id: "user:bad".into(),
            priority: 500,
            match_type: StoredMatchType::Regex,
            match_value: "[unterminated".into(),
            category: "X".into(),
            confidence: 0.9,
            created_at: "2026-05-26T12:00:00Z".into(),
        };
        assert!(s.to_runtime().is_err());
    }

    #[test]
    fn compile_stored_skips_bad_rules_silently() {
        let good = StoredRule {
            id: "good".into(),
            priority: 500,
            match_type: StoredMatchType::Contains,
            match_value: "swiggy".into(),
            category: "Food Delivery".into(),
            confidence: 0.9,
            created_at: "2026-05-26T12:00:00Z".into(),
        };
        let bad = StoredRule {
            id: "bad".into(),
            priority: 500,
            match_type: StoredMatchType::Regex,
            match_value: "[unterminated".into(),
            category: "X".into(),
            confidence: 0.9,
            created_at: "2026-05-26T12:00:00Z".into(),
        };
        let compiled = compile_stored(&[good, bad]);
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].id, "good");
    }

    #[test]
    fn json_round_trips() {
        let s = StoredRule {
            id: "user:json".into(),
            priority: 1000,
            match_type: StoredMatchType::Contains,
            match_value: "amazon".into(),
            category: "Online Shopping".into(),
            confidence: 0.9,
            created_at: "2026-05-26T12:00:00Z".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: StoredRule = serde_json::from_str(&j).unwrap();
        assert_eq!(back.id, s.id);
        assert_eq!(back.match_value, s.match_value);
    }
}
