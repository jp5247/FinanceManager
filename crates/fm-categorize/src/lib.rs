//! Transaction categorization for FinanceManager.
//!
//! ## Pipeline order
//!
//! Categorization at upload time follows this order:
//!
//! 1. **User-saved rules** (priority [`USER_RULE_PRIORITY`], 1000) —
//!    persisted per-profile via [`StoredRule`].
//! 2. **Curated merchant table** (priority [`CURATED_PRIORITY`], 500) —
//!    [`curated_merchants`] ships with the app; only unambiguous brand
//!    substrings.
//! 3. **Internet merchant lookup** — not yet implemented; will plug in
//!    here.
//! 4. **Uncategorized** — manual recategorization via the UI.
//!
//! [`build_rules`] composes the user rules + curated table into a single
//! [`RuleSet`] for one call to [`categorize`].

#![forbid(unsafe_code)]

mod builtin;
mod engine;
mod rule;
mod stored;

pub use builtin::{
    build_rules, curated_merchants, default_rules, CURATED_PRIORITY, USER_RULE_PRIORITY,
};
pub use engine::{categorize, CategoryHit};
pub use rule::{contains_rule, regex_rule, MatchType, Rule, RuleSet, UNCATEGORIZED};
pub use stored::{compile_stored, StoredMatchType, StoredRule, StoredRuleError};
