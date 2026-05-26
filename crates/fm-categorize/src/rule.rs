use regex::Regex;

/// String literal used by the UI when no rule matches a transaction.
pub const UNCATEGORIZED: &str = "Uncategorized";

/// How a [`Rule`] decides whether a description matches.
pub enum MatchType {
    /// Case-insensitive substring match.
    Contains(String),
    /// Full regex (callers compile via [`regex_rule`]). Use `(?i)` inside
    /// the pattern for case-insensitive matching.
    Regex(Regex),
}

/// One categorization rule.
pub struct Rule {
    /// Stable identifier, e.g. `"food/swiggy"`. Recorded on every
    /// categorized transaction so audit / debug can trace which rule fired.
    pub id: &'static str,

    /// Higher value = tried first. Tie-break by insertion order.
    pub priority: i32,

    pub matcher: MatchType,

    /// Category label shown to the user, e.g. `"Food Delivery"`.
    pub category: &'static str,

    /// 0.0..=1.0 — how confident this rule is in its classification.
    /// Used later for the "flag low-confidence rows" review queue.
    pub confidence: f32,
}

/// Convenience constructor for a case-insensitive `Contains` rule with
/// confidence 0.9.
pub fn contains_rule(
    id: &'static str,
    priority: i32,
    needle: &str,
    category: &'static str,
) -> Rule {
    Rule {
        id,
        priority,
        matcher: MatchType::Contains(needle.to_string()),
        category,
        confidence: 0.9,
    }
}

/// Convenience constructor for a regex rule with confidence 0.9.
/// Panics if `pattern` is not a valid regex — intended for static patterns
/// embedded at compile time.
pub fn regex_rule(id: &'static str, priority: i32, pattern: &str, category: &'static str) -> Rule {
    Rule {
        id,
        priority,
        matcher: MatchType::Regex(Regex::new(pattern).expect("invalid built-in regex")),
        category,
        confidence: 0.9,
    }
}

/// Collection of rules, sorted by priority descending at construction.
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new(mut rules: Vec<Rule>) -> Self {
        rules.sort_by_key(|r| std::cmp::Reverse(r.priority));
        Self { rules }
    }

    /// Append more rules; re-sorts by priority. Use this to merge built-in
    /// rules with user-supplied ones.
    pub fn with(mut self, more: impl IntoIterator<Item = Rule>) -> Self {
        self.rules.extend(more);
        self.rules.sort_by_key(|r| std::cmp::Reverse(r.priority));
        self
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Rule> {
        self.rules.iter()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
