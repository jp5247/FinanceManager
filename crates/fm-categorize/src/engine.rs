use crate::rule::{MatchType, RuleSet};

#[derive(Clone, Debug, PartialEq)]
pub struct CategoryHit {
    pub category: String,
    pub rule_id: String,
    pub confidence: f32,
}

/// Try each rule in priority order; return the first match.
///
/// `description` is matched directly for [`MatchType::Regex`]; for
/// [`MatchType::Contains`] both sides are lower-cased so matching is
/// case-insensitive.
pub fn categorize(rules: &RuleSet, description: &str) -> Option<CategoryHit> {
    let lowered = description.to_lowercase();
    for rule in &rules.rules {
        let matched = match &rule.matcher {
            MatchType::Contains(needle) => lowered.contains(&needle.to_lowercase()),
            MatchType::Regex(re) => re.is_match(description),
        };
        if matched {
            return Some(CategoryHit {
                category: rule.category.clone(),
                rule_id: rule.id.clone(),
                confidence: rule.confidence,
            });
        }
    }
    None
}
