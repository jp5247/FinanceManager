//! Categorization engine + curated-table + pipeline tests.

use fm_categorize::{
    build_rules, categorize, contains_rule, curated_merchants, default_rules, regex_rule,
    StoredMatchType, StoredRule, CURATED_PRIORITY, USER_RULE_PRIORITY,
};

fn cat(desc: &str) -> Option<String> {
    categorize(&default_rules(), desc).map(|h| h.category)
}

#[test]
fn curated_table_has_entries() {
    assert!(!curated_merchants().is_empty());
    assert!(!default_rules().is_empty());
}

#[test]
fn curated_table_classifies_unambiguous_brands() {
    assert_eq!(cat("UPI-SWIGGY-MERCHANT"), Some("Food expenses".into()));
    assert_eq!(cat("UPI-ZOMATO LTD"), Some("Food expenses".into()));
    assert_eq!(cat("UPI-AMAZON SELLER SERVICES"), Some("Shopping".into()));
    assert_eq!(cat("UPI-FLIPKART INTERNET"), Some("Shopping".into()));
    assert_eq!(cat("UPI-UPSTOX SECURITIES"), Some("Stock purchase".into()));
    assert_eq!(cat("UPI-RAPIDO BIKE"), Some("Transportation".into()));
}

#[test]
fn swiggy_instamart_resolves_to_groceries_not_food_delivery() {
    // Compound rule has higher priority than plain "swiggy".
    let hit = categorize(&default_rules(), "UPI-SWIGGY INSTAMART-MERCHANT").unwrap();
    assert_eq!(hit.category, "Groceries");
}

#[test]
fn cred_payments_resolve_to_credit_card_bill() {
    assert_eq!(
        cat("UPI-CRED CLUB-CRED.CLUB@AXISB"),
        Some("Credit card bill".into())
    );
    assert_eq!(
        cat("BPPY CC PAYMENT DP016 PAYMENT ON CRED"),
        Some("Credit card bill".into())
    );
}

#[test]
fn no_curated_rule_classifies_indian_railway_dividend() {
    // The whole reason the regex defaults were dropped: this NACH dividend
    // must NOT be classified as Train Travel by the curated table.
    assert_eq!(
        cat("CEMTEX DEP ACHCr NACH00000000006531 INDIAN RAILWAY"),
        None
    );
}

#[test]
fn no_curated_rule_classifies_generic_salary_text() {
    // Curated table is brand-based, not keyword-based — generic "salary"
    // string in a merchant name shouldn't auto-categorize.
    assert_eq!(cat("UPI-SALARY ENTERPRISES PVT LTD"), None);
}

#[test]
fn user_rule_overrides_curated() {
    let user = vec![contains_rule(
        "user:my-swiggy",
        USER_RULE_PRIORITY,
        "swiggy",
        "Restaurants",
    )];
    let rs = build_rules(user);
    let hit = categorize(&rs, "UPI-SWIGGY-MERCHANT").unwrap();
    assert_eq!(hit.category, "Restaurants");
    assert_eq!(hit.rule_id, "user:my-swiggy");
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn user_priority_is_above_curated() {
    assert!(USER_RULE_PRIORITY > CURATED_PRIORITY);
}

#[test]
fn build_rules_preserves_both_tiers() {
    let user = vec![contains_rule(
        "user:rent",
        USER_RULE_PRIORITY,
        "house rent",
        "Rent",
    )];
    let rs = build_rules(user);
    // Curated entry still works for unrelated narrations.
    assert_eq!(
        categorize(&rs, "UPI-FLIPKART INTERNET").map(|h| h.category),
        Some("Shopping".into())
    );
    // User rule works (the chosen category here is arbitrary — Rent is no
    // longer in the canonical list but user rules are free-form labels).
    assert_eq!(
        categorize(&rs, "transfer for house rent april").map(|h| h.category),
        Some("Rent".into())
    );
}

#[test]
fn stored_rule_round_trips_through_runtime() {
    let s = StoredRule {
        id: "user:test".into(),
        priority: USER_RULE_PRIORITY,
        match_type: StoredMatchType::Contains,
        match_value: "uber india".into(),
        category: "Cab / Ride".into(),
        confidence: 0.9,
        created_at: "2026-05-26T12:00:00Z".into(),
    };
    let r = s.to_runtime().unwrap();
    let rs = fm_categorize::RuleSet::new(vec![r]);
    assert_eq!(
        categorize(&rs, "UPI-UBER INDIA SYSTEMS").map(|h| h.category),
        Some("Cab / Ride".into())
    );
}

#[test]
fn rule_set_with_no_rules_returns_none_for_anything() {
    let empty = fm_categorize::RuleSet::new(Vec::new());
    assert!(categorize(&empty, "anything goes").is_none());
}

#[test]
fn regex_rule_supports_case_insensitive_flag() {
    let rs = fm_categorize::RuleSet::new(vec![regex_rule(
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
