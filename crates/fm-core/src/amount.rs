use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Monetary amount stored as a fixed-point [`Decimal`].
///
/// Always represents an absolute magnitude — sign / direction is carried by
/// the transaction's `direction` field, not by the amount itself.
///
/// Construction goes through [`Amount::parse_inr`] which accepts both
/// Western (`21,447.00`) and Indian lakhs (`1,12,722.70`) thousands grouping,
/// optional leading `₹` / `Rs` / corrupted `C` glyph, and optional
/// `Cr` / `Dr` suffix. Direction markers consumed by the parser are reported
/// via [`ParsedAmount`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Amount(#[serde(with = "rust_decimal::serde::str")] Decimal);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    /// `+` prefix or `Cr` suffix — money in.
    Credit,
    /// `-` prefix or `Dr` suffix — money out.
    Debit,
    /// No explicit marker present.
    Unmarked,
}

/// Result of parsing an amount token — magnitude + any direction marker found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParsedAmount {
    pub amount: Amount,
    pub sign: Sign,
}

#[derive(Debug, Error)]
pub enum AmountParseError {
    #[error("empty amount token")]
    Empty,
    #[error("amount token contains no digits: {0:?}")]
    NoDigits(String),
    #[error("amount could not be parsed as a decimal: {0:?}")]
    BadDecimal(String),
}

impl Amount {
    pub fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    pub fn from_decimal(d: Decimal) -> Self {
        Self(d.abs())
    }

    pub fn as_decimal(&self) -> Decimal {
        self.0
    }

    /// Parse a free-form Indian-bank amount token, returning the magnitude
    /// and any sign / direction marker found. Examples accepted:
    ///
    /// - `1,582.00`
    /// - `1,12,722.70`        (Indian lakhs grouping)
    /// - `₹ 1,582.00`
    /// - `Rs 1,582.00`
    /// - `C 1,582.00`         (corrupted rupee glyph from pdfium extraction)
    /// - `+ C 21,447.00`      (HDFC payment received marker)
    /// - `1,582.00 Cr`        (credit suffix)
    /// - `1,582.00CR`         (no space)
    pub fn parse_inr(input: &str) -> Result<ParsedAmount, AmountParseError> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err(AmountParseError::Empty);
        }

        // Detect direction markers and strip them along with currency tokens.
        let upper = raw.to_uppercase();
        let mut sign = Sign::Unmarked;
        if raw.starts_with('+') {
            sign = Sign::Credit;
        } else if raw.starts_with('-') {
            sign = Sign::Debit;
        } else if upper.ends_with(" CR") || upper.ends_with("CR") {
            sign = Sign::Credit;
        } else if upper.ends_with(" DR") || upper.ends_with("DR") {
            sign = Sign::Debit;
        }

        // Find the numeric token: anchor on the first digit, then run until
        // a non-[digit,comma,dot] char. This skips any currency prefix
        // (`Rs.` / `₹` / `C` / `+`) without confusing the `.` in `Rs.` for a
        // decimal point.
        let first_digit = raw
            .find(|c: char| c.is_ascii_digit())
            .ok_or_else(|| AmountParseError::NoDigits(raw.to_string()))?;
        let tail = &raw[first_digit..];
        let end = tail
            .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
            .unwrap_or(tail.len());
        let number_text = &tail[..end];
        let normalized = number_text.replace(',', "");
        let decimal = Decimal::from_str(&normalized)
            .map_err(|_| AmountParseError::BadDecimal(raw.to_string()))?;

        Ok(ParsedAmount {
            amount: Amount(decimal.abs()),
            sign,
        })
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Always two decimal places for currency display.
        write!(f, "{:.2}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn ok(input: &str, expected_minor: i64, sign: Sign) {
        let p = Amount::parse_inr(input).unwrap();
        let got_paise = (p.amount.as_decimal() * dec!(100)).round();
        let want_paise = Decimal::from(expected_minor);
        assert_eq!(
            got_paise, want_paise,
            "amount mismatch for {input:?}: got {got_paise:?} want {want_paise:?}"
        );
        assert_eq!(p.sign, sign, "sign mismatch for {input:?}");
    }

    #[test]
    fn parses_plain_western_format() {
        ok("1,582.00", 158_200, Sign::Unmarked);
        ok("21,447.00", 2_144_700, Sign::Unmarked);
        ok("170.64", 17_064, Sign::Unmarked);
    }

    #[test]
    fn parses_indian_lakhs_format() {
        ok("1,12,722.70", 11_272_270, Sign::Unmarked);
        ok("1,00,00,000.00", 1_000_000_000, Sign::Unmarked);
    }

    #[test]
    fn parses_with_corrupted_rupee_glyph() {
        ok("C 1,582.00", 158_200, Sign::Unmarked);
        ok("C170.64", 17_064, Sign::Unmarked);
    }

    #[test]
    fn parses_with_rupee_symbol() {
        ok("₹ 1,582.00", 158_200, Sign::Unmarked);
        ok("Rs 1,582.00", 158_200, Sign::Unmarked);
        ok("Rs.1,582.00", 158_200, Sign::Unmarked);
    }

    #[test]
    fn detects_credit_marker_prefix() {
        ok("+ C 21,447.00", 2_144_700, Sign::Credit);
        ok("+21,447.00", 2_144_700, Sign::Credit);
    }

    #[test]
    fn detects_debit_marker_prefix() {
        ok("- 1,582.00", 158_200, Sign::Debit);
    }

    #[test]
    fn detects_cr_dr_suffix() {
        ok("1,12,722.70 Cr", 11_272_270, Sign::Credit);
        ok("1,12,722.70CR", 11_272_270, Sign::Credit);
        ok("500.00 Dr", 50_000, Sign::Debit);
    }

    #[test]
    fn rejects_empty_and_non_numeric() {
        assert!(matches!(
            Amount::parse_inr(""),
            Err(AmountParseError::Empty)
        ));
        assert!(matches!(
            Amount::parse_inr("   "),
            Err(AmountParseError::Empty)
        ));
        assert!(matches!(
            Amount::parse_inr("Cr only"),
            Err(AmountParseError::NoDigits(_))
        ));
    }

    #[test]
    fn display_always_two_decimals() {
        let a = Amount::from_decimal(dec!(1582));
        assert_eq!(format!("{a}"), "1582.00");
        let b = Amount::from_decimal(dec!(170.64));
        assert_eq!(format!("{b}"), "170.64");
    }

    #[test]
    fn serde_round_trip_via_string() {
        let a = Amount::from_decimal(dec!(11272.70));
        let j = serde_json::to_string(&a).unwrap();
        assert_eq!(j, r#""11272.70""#);
        let back: Amount = serde_json::from_str(&j).unwrap();
        assert_eq!(a, back);
    }
}
