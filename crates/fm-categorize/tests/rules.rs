//! Categorization engine tests. With the built-in rule set now empty, these
//! cover the engine itself (priority ordering, contains/regex matching) by
//! constructing ad-hoc rule sets.

use fm_categorize::{categorize, contains_rule, default_rules, regex_rule, Rule, RuleSet};

#[test]
fn default_rules_ship_empty() {
    // The product intentionally ships no auto-rules. User rules and curated
    // merchant lookup arrive in follow-up work.
    assert!(default_rules().is_empty());
}

#[test]
fn contains_rule_matches_case_insensitively() {
    let rs = RuleSet::new(vec![contains_rule(
        "food/swiggy",
        500,
        "swiggy",
        "Food Delivery",
    )]);
    assert_eq!(
        categorize(&rs, "UPI-SWIGGY-MERCHANT").map(|h| h.category),
        Some("Food Delivery".into())
    );
    assert_eq!(
        categorize(&rs, "swiggy lower case").map(|h| h.category),
        Some("Food Delivery".into())
    );
    assert_eq!(categorize(&rs, "unrelated"), None);
}

#[test]
fn regex_rule_supports_case_insensitive_flag() {
    let rs = RuleSet::new(vec![regex_rule(
        "atm",
        500,
        r"(?i)\bATM\s+WDL\b",
        "ATM / Cash",
    )]);
    assert_eq!(
        categorize(&rs, "atm wdl at branch").map(|h| h.category),
        Some("ATM / Cash".into())
    );
}

#[test]
fn higher_priority_wins() {
    let rs = RuleSet::new(vec![
        contains_rule("specific", 1000, "swiggy instamart", "Groceries"),
        contains_rule("generic", 100, "swiggy", "Food Delivery"),
    ]);
    let hit = categorize(&rs, "UPI-SWIGGY INSTAMART-639203").unwrap();
    assert_eq!(hit.category, "Groceries");
    assert_eq!(hit.rule_id, "specific");
}

#[test]
fn rules_sorted_by_priority_descending_on_construction() {
    let rs = RuleSet::new(vec![
        contains_rule("a", 100, "a", "A"),
        contains_rule("b", 500, "b", "B"),
        contains_rule("c", 300, "c", "C"),
    ]);
    let prios: Vec<i32> = rs.iter().map(|r| r.priority).collect();
    assert_eq!(prios, vec![500, 300, 100]);
}

#[test]
fn with_appends_and_resorts() {
    let rs = RuleSet::new(vec![contains_rule("a", 100, "a", "A")])
        .with(vec![contains_rule("b", 999, "b", "B")]);
    let prios: Vec<i32> = rs.iter().map(|r| r.priority).collect();
    assert_eq!(prios, vec![999, 100]);
}

#[test]
fn no_match_returns_none() {
    let rs: RuleSet = RuleSet::new(vec![contains_rule("a", 1, "amazon", "Shop")]);
    assert!(categorize(&rs, "totally unrelated").is_none());
    assert!(categorize(&rs, "").is_none());
}

#[test]
fn rule_set_with_no_rules_returns_none_for_anything() {
    let empty = RuleSet::new(Vec::new());
    assert!(categorize(&empty, "anything goes").is_none());
}

#[test]
fn no_built_in_rule_classifies_indian_railway_dividend() {
    // Regression: a NACH dividend from Indian Railway Finance Corp must NOT
    // be auto-classified as Train Travel. With built-ins empty, it stays
    // unclassified — the manual recategorize UI handles it.
    let rs = default_rules();
    assert!(categorize(&rs, "CEMTEX DEP ACHCr NACH00000000006531 INDIAN RAILWAY").is_none());
}

// Compile-time sanity: Rule constructors stay usable as the public API for
// when user-saved rules land.
#[allow(dead_code)]
fn _types_compile() {
    let _: Rule = contains_rule("x", 1, "y", "Cat");
    let _: Rule = regex_rule("x", 1, r"(?i)y", "Cat");
}
