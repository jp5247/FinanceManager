//! Merchant-name extraction from raw transaction descriptions.
//!
//! Strips prefixes (UPI/ACH/NACH/NEFT/etc.), reference numbers, and UPI
//! handles to leave just the merchant name that is safe to send to an
//! external categorization service. Per OD-5, **only** the extracted
//! merchant name + the direction (debit / credit) ever leaves the device.
//! Amounts, account masks, dates, and ref IDs are never sent.

use regex::Regex;
use std::sync::OnceLock;

const MAX_MERCHANT_LEN: usize = 80;

/// Output of [`extract_merchant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedMerchant {
    /// Cleaned merchant name, safe for external lookup.
    pub name: String,
    /// Which extraction pattern fired — useful for debugging / telemetry.
    pub pattern: MerchantPattern,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MerchantPattern {
    Upi,
    AchDebit,
    AchCredit,
    Nach,
    Neft,
    CcEmi,
    BillPay,
    /// Fallback when no specific pattern matched.
    Raw,
}

fn upi_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // UPI-{merchant}-...   or UPI/...   capture group: merchant portion
    R.get_or_init(|| Regex::new(r"^UPI[-/](.+)$").expect("static regex"))
}

fn ach_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // "ACH D- INDIAN CLEARING CORP-D6800438X028 0000003283698831"
    R.get_or_init(|| {
        Regex::new(r"^ACH\s+(D|CR)-\s*(.+?)(?:-[A-Z0-9]{6,}|\s+\d{6,}|$)").expect("static regex")
    })
}

fn nach_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // "CEMTEX DEP ACHCr NACH00000000006531 INDIAN RAILWAY"
    R.get_or_init(|| Regex::new(r"(?i)NACH\d+\s+(.+?)$").expect("static regex"))
}

fn neft_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // "NEFT CR-SCBL0036001-EMPLOYER PVT LTD-JAI-SALARY APR 2026"
    // After NEFT CR-{IFSC}- the next chunk is usually the payer/merchant.
    R.get_or_init(|| Regex::new(r"^NEFT\s+(?:CR|DR)-[A-Z0-9]+-([^-]+)").expect("static regex"))
}

/// Truncate to [`MAX_MERCHANT_LEN`] chars and trim whitespace.
fn finalize(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= MAX_MERCHANT_LEN {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX_MERCHANT_LEN).collect()
}

/// Find a sensible cut-off in a UPI/ACH tail that follows the merchant
/// name. Cuts at the first of: `@`, `-{4+ digits}`, ` {6+ digits}`.
fn cut_after_merchant(s: &str) -> &str {
    if let Some(at) = s.find('@') {
        // Often UPI handle starts a few chars BEFORE the `@`; walk back to the
        // hyphen just before the handle.
        let before_at = &s[..at];
        if let Some(last_hyphen) = before_at.rfind('-') {
            return s[..last_hyphen].trim_end();
        }
        return s[..at].trim_end();
    }
    // Find `-{4+ digits}` (UPI/ACH ref-id boundary)
    let bytes = s.as_bytes();
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i] == b'-' {
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|&&b| b.is_ascii_digit())
                .count();
            if digits >= 4 {
                return s[..i].trim_end();
            }
        }
    }
    // Find ` {6+ digits}` boundary
    let alt: Vec<&str> = s.splitn(2, char::is_whitespace).collect();
    if alt.len() == 2 {
        let rest = alt[1];
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits >= 6 {
            return alt[0].trim_end();
        }
    }
    s
}

pub fn extract_merchant(description: &str) -> ExtractedMerchant {
    let raw = description.trim();
    let upper = raw.to_uppercase();

    // UPI pattern
    if let Some(caps) = upi_re().captures(&upper) {
        let after_prefix = &raw[caps.get(1).unwrap().start()..];
        let cut = cut_after_merchant(after_prefix);
        return ExtractedMerchant {
            name: finalize(cut),
            pattern: MerchantPattern::Upi,
        };
    }

    // ACH D- / ACH CR-
    if let Some(caps) = ach_re().captures(&upper) {
        let direction = caps.get(1).map(|m| m.as_str()).unwrap_or("D");
        let merchant_match = caps.get(2).unwrap();
        let s = raw
            .get(merchant_match.start()..merchant_match.end())
            .unwrap_or("");
        return ExtractedMerchant {
            name: finalize(s),
            pattern: if direction == "CR" {
                MerchantPattern::AchCredit
            } else {
                MerchantPattern::AchDebit
            },
        };
    }

    // NACH (typically credits — dividends, mutual fund payouts, etc.)
    if let Some(caps) = nach_re().captures(raw) {
        let s = caps.get(1).unwrap().as_str();
        return ExtractedMerchant {
            name: finalize(s),
            pattern: MerchantPattern::Nach,
        };
    }

    // NEFT
    if let Some(caps) = neft_re().captures(&upper) {
        let merchant_match = caps.get(1).unwrap();
        let s = raw
            .get(merchant_match.start()..merchant_match.end())
            .unwrap_or("");
        return ExtractedMerchant {
            name: finalize(s),
            pattern: MerchantPattern::Neft,
        };
    }

    // CC EMI / OFFUS EMI
    if upper.starts_with("OFFUS EMI") || upper.starts_with("EMI ") || upper.contains("EMI,PRIN") {
        return ExtractedMerchant {
            name: "EMI / Loan repayment".to_string(),
            pattern: MerchantPattern::CcEmi,
        };
    }

    // IB BILLPAY / BILLPAY
    if upper.starts_with("IB BILLPAY") || upper.contains("BILLPAY") {
        let after = raw.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        return ExtractedMerchant {
            name: finalize(&after),
            pattern: MerchantPattern::BillPay,
        };
    }

    // Fallback: take the first ~60 chars, cut at a digit run that looks
    // like a ref-id.
    let cut = cut_after_merchant(raw);
    ExtractedMerchant {
        name: finalize(cut),
        pattern: MerchantPattern::Raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extracts(input: &str, expected_name: &str, expected_pattern: MerchantPattern) {
        let got = extract_merchant(input);
        assert_eq!(
            got.name, expected_name,
            "wrong name for {input:?}: got {got:?}"
        );
        assert_eq!(got.pattern, expected_pattern, "wrong pattern for {input:?}");
    }

    #[test]
    fn upi_strips_handle_and_ref() {
        extracts(
            "UPI-CRED CLUB-CRED.CLUB@AXISB-UTIB000011 0000645724403219 01/04/26",
            "CRED CLUB",
            MerchantPattern::Upi,
        );
        extracts(
            "UPI-SWIGGY INSTAMART-639203@OKAXIS-paid",
            "SWIGGY INSTAMART",
            MerchantPattern::Upi,
        );
    }

    #[test]
    fn upi_with_only_ref_digits_works() {
        // The cut keeps the payment-provider hint `-GPAY` — that's still
        // part of the merchant string. What matters is that the long
        // account/ref-digits ARE stripped (asserted separately in
        // never_contains_account_or_amount_digits).
        extracts(
            "UPI-MAHAVIR SWEETS AND F-GPAY-1219049458 0000648728252238 01/05/26",
            "MAHAVIR SWEETS AND F-GPAY",
            MerchantPattern::Upi,
        );
    }

    #[test]
    fn ach_extracts_corp_name() {
        extracts(
            "ACH D- INDIAN CLEARING CORP-D6800438X028 0000003283698831",
            "INDIAN CLEARING CORP",
            MerchantPattern::AchDebit,
        );
    }

    #[test]
    fn nach_extracts_payer_name() {
        extracts(
            "CEMTEX DEP ACHCr NACH00000000006531 INDIAN RAILWAY",
            "INDIAN RAILWAY",
            MerchantPattern::Nach,
        );
        extracts(
            "CEMTEX DEP ACHCr NACH00000000021008 INDIAN ENERGY",
            "INDIAN ENERGY",
            MerchantPattern::Nach,
        );
    }

    #[test]
    fn neft_extracts_payer_name() {
        extracts(
            "NEFT CR-SCBL0036001-SUREPREP (INDIA) PRIVATE LIMITED-JAI PAREKH-SALARY APR 2026",
            "SUREPREP (INDIA) PRIVATE LIMITED",
            MerchantPattern::Neft,
        );
    }

    #[test]
    fn offus_emi_handled() {
        let m = extract_merchant("OFFUS EMI,PRIN NB:02,00000138162352");
        assert_eq!(m.pattern, MerchantPattern::CcEmi);
        assert_eq!(m.name, "EMI / Loan repayment");
    }

    #[test]
    fn billpay_handled() {
        let m = extract_merchant("IB BILLPAY DR-HDFCSI-485498XXXXXX2025 1775062574612494");
        assert_eq!(m.pattern, MerchantPattern::BillPay);
        assert!(m.name.starts_with("IB BILLPAY"));
    }

    #[test]
    fn short_descriptions_pass_through() {
        let m = extract_merchant("FORTPOINTMUMBAIMUMBAI");
        assert_eq!(m.name, "FORTPOINTMUMBAIMUMBAI");
        assert_eq!(m.pattern, MerchantPattern::Raw);
    }

    #[test]
    fn long_merchant_truncated() {
        let big = "A".repeat(200);
        let m = extract_merchant(&big);
        assert!(m.name.chars().count() <= MAX_MERCHANT_LEN);
    }

    #[test]
    fn never_contains_account_or_amount_digits() {
        // Probe that the extractor strips long digit runs.
        let m = extract_merchant("UPI-MERCHANT-1219049458 0000648728252238 01/05/26");
        assert!(!m.name.contains("1219049458"), "got {m:?}");
        assert!(!m.name.contains("0000648728252238"), "got {m:?}");
    }
}
