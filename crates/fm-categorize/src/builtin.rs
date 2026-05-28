//! Curated merchant lookup table.
//!
//! These are NOT regex-based keyword rules (which proved to false-positive —
//! e.g. matching "INDIAN RAILWAY" on IRFC dividends). Every entry here is an
//! **unambiguous brand substring** that, when present in a transaction
//! description, is essentially certain to indicate that merchant.
//!
//! All entries use [`MatchType::Contains`](crate::MatchType::Contains) at
//! priority 500. User-saved rules sit above this at priority 1000, so the
//! user always overrides. Anything that doesn't match here AND doesn't match
//! a user rule stays uncategorized.
//!
//! Be conservative about adding entries: a single false positive is worse
//! than ten uncategorized rows.

use crate::rule::{contains_rule, Rule, RuleSet};

/// Priority assigned to every entry in the curated table.
pub const CURATED_PRIORITY: i32 = 500;

/// Recommended priority for user-saved rules. Higher than curated so the
/// user's intent always wins on overlap.
pub const USER_RULE_PRIORITY: i32 = 1000;

/// The curated merchant table. High-precision brand strings mapped to the
/// canonical taxonomy in `src/categories.ts` / `llm.rs::ALLOWED_CATEGORIES`.
pub fn curated_merchants() -> Vec<Rule> {
    vec![
        // Compound first so it wins the priority tie when nested in plain
        // "swiggy" matches.
        contains_rule(
            "curated:swiggy-instamart",
            CURATED_PRIORITY + 100,
            "swiggy instamart",
            "Groceries",
        ),
        contains_rule(
            "curated:swiggy",
            CURATED_PRIORITY,
            "swiggy",
            "Food expenses",
        ),
        contains_rule(
            "curated:zomato",
            CURATED_PRIORITY,
            "zomato",
            "Food expenses",
        ),
        contains_rule("curated:blinkit", CURATED_PRIORITY, "blinkit", "Groceries"),
        contains_rule("curated:zepto", CURATED_PRIORITY, "zepto", "Groceries"),
        contains_rule(
            "curated:bigbasket",
            CURATED_PRIORITY,
            "bigbasket",
            "Groceries",
        ),
        contains_rule(
            "curated:rapido",
            CURATED_PRIORITY,
            "rapido",
            "Transportation",
        ),
        contains_rule(
            "curated:uber-india",
            CURATED_PRIORITY,
            "uber india",
            "Transportation",
        ),
        contains_rule("curated:ola", CURATED_PRIORITY, "olacabs", "Transportation"),
        contains_rule("curated:irctc", CURATED_PRIORITY, "irctc", "Transportation"),
        contains_rule("curated:amazon", CURATED_PRIORITY, "amazon", "Shopping"),
        contains_rule("curated:flipkart", CURATED_PRIORITY, "flipkart", "Shopping"),
        contains_rule("curated:myntra", CURATED_PRIORITY, "myntra", "Shopping"),
        contains_rule("curated:meesho", CURATED_PRIORITY, "meesho", "Shopping"),
        contains_rule("curated:ajio", CURATED_PRIORITY, "ajio", "Shopping"),
        contains_rule(
            "curated:tata-cliq",
            CURATED_PRIORITY,
            "tata cliq",
            "Shopping",
        ),
        // Investment platforms — keep these split between FD / SIP / Stock
        // purchase based on the platform's primary purpose. Mixed-use
        // platforms (Zerodha, Groww) default to Stock purchase since
        // direct equity is their headline product; the user can
        // recategorize SIP rows specifically.
        contains_rule(
            "curated:upstox",
            CURATED_PRIORITY,
            "upstox",
            "Stock purchase",
        ),
        contains_rule(
            "curated:zerodha",
            CURATED_PRIORITY,
            "zerodha",
            "Stock purchase",
        ),
        contains_rule("curated:groww", CURATED_PRIORITY, "groww", "SIP"),
        contains_rule("curated:indmoney", CURATED_PRIORITY, "indmoney", "SIP"),
        contains_rule("curated:kuvera", CURATED_PRIORITY, "kuvera", "SIP"),
        contains_rule("curated:scripbox", CURATED_PRIORITY, "scripbox", "SIP"),
        // Entertainment + streaming
        contains_rule(
            "curated:bookmyshow",
            CURATED_PRIORITY,
            "bookmyshow",
            "Entertainment",
        ),
        contains_rule(
            "curated:netflix",
            CURATED_PRIORITY,
            "netflix",
            "Entertainment",
        ),
        contains_rule(
            "curated:spotify",
            CURATED_PRIORITY,
            "spotify",
            "Entertainment",
        ),
        contains_rule(
            "curated:hotstar",
            CURATED_PRIORITY,
            "hotstar",
            "Entertainment",
        ),
        contains_rule(
            "curated:prime-video",
            CURATED_PRIORITY,
            "prime video",
            "Entertainment",
        ),
        // Credit-card bill payments via CRED / PayTM / etc.
        contains_rule(
            "curated:cred-club",
            CURATED_PRIORITY,
            "cred club",
            "Credit card bill",
        ),
        contains_rule(
            "curated:cred-dot-club",
            CURATED_PRIORITY,
            "cred.club",
            "Credit card bill",
        ),
        contains_rule(
            "curated:payment-on-cred",
            CURATED_PRIORITY,
            "payment on cred",
            "Credit card bill",
        ),
        contains_rule(
            "curated:bppy-cc-payment",
            CURATED_PRIORITY,
            "bppy cc payment",
            "Credit card bill",
        ),
        // HDFC credit-card EMI bookkeeping rows. When a transaction is
        // converted to EMI, the statement shows three related rows: the
        // original purchase (debit), the loan principal being booked
        // (debit), and the loan disbursement (credit). The credit and
        // one of the debits cancel out — categorising both as "EMI
        // Conversion" (Transfer kind) keeps them out of income / expense
        // so only actual recurring installments + processing fee + GST
        // count as real outflow. Pattern picked from real HDFC Regalia /
        // Rupay statements: "AGGREGATOR-EMI-OFFUSCREDIT" on the credit
        // side and "EMI BOOKING" / "OFFUSCREDIT" on the principal-book
        // debit. The user can still recategorize a specific row to
        // "Loan EMI" if it's an actual installment.
        contains_rule(
            "curated:hdfc-emi-offuscredit",
            CURATED_PRIORITY,
            "offuscredit",
            "EMI Conversion",
        ),
        contains_rule(
            "curated:hdfc-aggregator-emi",
            CURATED_PRIORITY,
            "aggregator-emi",
            "EMI Conversion",
        ),
        contains_rule(
            "curated:hdfc-emi-booking",
            CURATED_PRIORITY,
            "emi booking",
            "EMI Conversion",
        ),
    ]
}

/// The default rule set when no user rules have been added yet — just the
/// curated table.
pub fn default_rules() -> RuleSet {
    RuleSet::new(curated_merchants())
}

/// Build a combined rule set from user-saved rules (already compiled) plus
/// the curated table. User rules keep whatever priority they were created
/// with (typically [`USER_RULE_PRIORITY`]).
pub fn build_rules(user_rules: Vec<Rule>) -> RuleSet {
    let mut all = user_rules;
    all.extend(curated_merchants());
    RuleSet::new(all)
}
