//! Default categorization rules.
//!
//! Intentionally **empty**. Auto-applied regex-based defaults proved to
//! cause meaningful false positives (e.g. "INDIAN RAILWAY" in a NACH dividend
//! narration misclassifying an IRFC dividend as a train ticket). The product
//! design has moved to:
//!
//! 1. **User-saved rules** stored per profile (Phase 2)
//! 2. **Curated merchant lookup** (internet, opt-in, merchant-name-only per OD-5)
//! 3. **Manual recategorization** as the fallback
//!
//! The rule engine itself (`fm_categorize::categorize`, [`Rule`], [`RuleSet`])
//! still ships — it's what step 1 above will populate at runtime.

use crate::rule::RuleSet;

pub fn default_rules() -> RuleSet {
    RuleSet::new(Vec::new())
}
