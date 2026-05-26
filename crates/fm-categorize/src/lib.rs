//! Transaction categorization for FinanceManager.
//!
//! A small rule engine that maps free-text transaction descriptions to
//! categories like `Salary`, `Food Delivery`, `Rent`, `Cab/Ride`, etc.
//!
//! Rules are tried in **priority order** (highest first); the first match
//! wins. The built-in rule set [`default_rules`] ships ~35 patterns covering
//! the most common Indian-banking transaction shapes. User-supplied rules
//! can be appended later (Phase-2 work) — the [`RuleSet::with`] helper is
//! designed for that.

#![forbid(unsafe_code)]

mod builtin;
mod engine;
mod rule;

pub use builtin::default_rules;
pub use engine::{categorize, CategoryHit};
pub use rule::{contains_rule, regex_rule, MatchType, Rule, RuleSet, UNCATEGORIZED};
