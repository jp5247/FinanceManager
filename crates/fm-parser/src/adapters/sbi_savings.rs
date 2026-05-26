//! State Bank of India savings-account statement adapter.
//!
//! ## Row anatomy
//!
//! Each transaction begins with two dates back-to-back — the transaction
//! date and the value date — followed by 4–6 lines of narration, then a
//! single line with the amount columns:
//!
//! ```text
//! 04/02/2026 04/02/2026
//! WDL TFR
//! UPI/DR/640109915231/Ghatkopa/
//! PPIW/ombk.aaci1/Paid
//! 0097692162094 AT 01131
//! GHATKOPAR (WEST)
//! - 1,500.00 - 540.20
//! ```
//!
//! ## Amount line layout
//!
//! Four whitespace-separated tokens, each either a number or `-` placeholder.
//! Token 1 is always `-` (column placeholder); the middle two are withdrawal
//! and deposit respectively, and token 4 is the running balance:
//!
//! - Debit row: `- WITHDRAWAL - BALANCE`
//! - Credit row: `- - DEPOSIT BALANCE`
//!
//! Direction is read directly from which column carries the amount — no
//! balance-delta heuristic needed.
//!
//! ## Format quirks vs HDFC
//!
//! - **Year is 4-digit** (`DD/MM/YYYY`) versus HDFC savings' 2-digit.
//! - **Plain numerics** — no corrupted `₹` glyph from pdfium.
//! - **Indian lakhs** thousand-grouping (`1,12,722.70`) — fm-core handles it.
//! - **Two pages of preamble** (relationship summary, branch info, etc.)
//!   before transactions begin. The anchor regex naturally skips it.

use crate::adapter::{BankAdapter, ParseError};
use crate::extracted::ExtractedPdf;
use crate::raw_txn::RawTransaction;
use fm_core::Amount;
use regex::Regex;
use std::sync::OnceLock;

pub struct SbiSavingsAdapter;

const ADAPTER_ID: &str = "sbi-savings";
const ADAPTER_VERSION: &str = "1.0.0";

impl SbiSavingsAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SbiSavingsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn anchor_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^(\d{2})/(\d{2})/(\d{4})\s+(\d{2})/(\d{2})/(\d{4})\s*(.*)$")
            .expect("static regex")
    })
}

/// `- AMOUNT - BALANCE [CR|DR]?$` — debit row.
fn debit_amounts_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"-\s+([\d,]+\.\d{2})\s+-\s+([\d,]+\.\d{2})(?:\s*(?:CR|DR))?\s*$")
            .expect("static regex")
    })
}

/// `- - AMOUNT BALANCE [CR|DR]?$` — credit row.
fn credit_amounts_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"-\s+-\s+([\d,]+\.\d{2})\s+([\d,]+\.\d{2})(?:\s*(?:CR|DR))?\s*$")
            .expect("static regex")
    })
}

fn is_hard_break(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return true;
    }
    if t.starts_with("Page ") {
        return true;
    }
    let lower = t.to_lowercase();
    lower.starts_with("**this is a computer generated")
        || lower.starts_with("statement of account")
        || lower.starts_with("**end of statement")
        || lower == "opening balance"
        || lower == "closing balance"
}

#[derive(Copy, Clone, Debug)]
enum AmountsKind {
    Debit,
    Credit,
}

fn detect_amounts(joined: &str) -> Option<AmountsKind> {
    // Check credit first — the `- - AMT BAL` pattern is more specific.
    if credit_amounts_re().is_match(joined) {
        Some(AmountsKind::Credit)
    } else if debit_amounts_re().is_match(joined) {
        Some(AmountsKind::Debit)
    } else {
        None
    }
}

impl BankAdapter for SbiSavingsAdapter {
    fn id(&self) -> &'static str {
        ADAPTER_ID
    }

    fn version(&self) -> &'static str {
        ADAPTER_VERSION
    }

    fn detect(&self, extracted: &ExtractedPdf) -> bool {
        let name = extracted.source_file.to_lowercase();
        if name.contains("sbi")
            && (name.contains("savings") || name.contains("account") || name.contains("statement"))
        {
            return true;
        }
        extracted.pages.iter().any(|p| {
            let lower = p.text.to_lowercase();
            lower.contains("state bank of india") || lower.contains("sbin0")
        })
    }

    fn parse(
        &self,
        extracted: &ExtractedPdf,
        import_id: &str,
    ) -> Result<Vec<RawTransaction>, ParseError> {
        let mut out = Vec::new();
        let mut row_counter: u32 = 0;
        let parser_version = format!("{ADAPTER_ID}@{ADAPTER_VERSION}");

        for page in &extracted.pages {
            let lines: Vec<&str> = page.text.lines().collect();
            let mut i = 0;
            while i < lines.len() {
                let Some(caps) = anchor_re().captures(lines[i]) else {
                    i += 1;
                    continue;
                };

                let day: u32 = caps[1].parse().map_err(|_| {
                    ParseError::BadDate(format!("non-numeric day in: {}", lines[i]))
                })?;
                let month: u32 = caps[2].parse().map_err(|_| {
                    ParseError::BadDate(format!("non-numeric month in: {}", lines[i]))
                })?;
                let year: u32 = caps[3].parse().map_err(|_| {
                    ParseError::BadDate(format!("non-numeric year in: {}", lines[i]))
                })?;
                if !(1..=31).contains(&day)
                    || !(1..=12).contains(&month)
                    || !(2000..=2100).contains(&year)
                {
                    return Err(ParseError::BadDate(format!(
                        "out of range in: {}",
                        lines[i]
                    )));
                }

                // Phase 1: absorb wrap lines until the trailing amounts pattern matches.
                let mut joined = caps[7].to_string();
                let mut j = i + 1;
                let mut kind = detect_amounts(&joined);
                while kind.is_none() {
                    if j >= lines.len() {
                        break;
                    }
                    if is_hard_break(lines[j]) || anchor_re().captures(lines[j]).is_some() {
                        break;
                    }
                    let extra = lines[j].trim();
                    if !extra.is_empty() {
                        if !joined.is_empty() {
                            joined.push(' ');
                        }
                        joined.push_str(extra);
                    }
                    j += 1;
                    kind = detect_amounts(&joined);
                }

                let Some(kind) = kind else {
                    i = j.max(i + 1);
                    continue;
                };

                let (debit_token, credit_token, balance_token, match_start) = match kind {
                    AmountsKind::Debit => {
                        let c = debit_amounts_re().captures(&joined).expect("kind verified");
                        let m = c.get(0).unwrap();
                        (Some(c[1].to_string()), None, c[2].to_string(), m.start())
                    }
                    AmountsKind::Credit => {
                        let c = credit_amounts_re()
                            .captures(&joined)
                            .expect("kind verified");
                        let m = c.get(0).unwrap();
                        (None, Some(c[1].to_string()), c[2].to_string(), m.start())
                    }
                };

                let mut description = joined[..match_start].trim().to_string();

                // Phase 2: further continuation lines → description.
                while j < lines.len() {
                    if is_hard_break(lines[j]) || anchor_re().captures(lines[j]).is_some() {
                        break;
                    }
                    let extra = lines[j].trim();
                    if !extra.is_empty() {
                        if !description.is_empty() {
                            description.push(' ');
                        }
                        description.push_str(extra);
                    }
                    j += 1;
                }

                if description.is_empty() {
                    i = j.max(i + 1);
                    continue;
                }

                let txn_date = format!("{year:04}-{month:02}-{day:02}");
                let debit = debit_token
                    .map(|s| Amount::parse_inr(&s).map(|p| p.amount))
                    .transpose()?;
                let credit = credit_token
                    .map(|s| Amount::parse_inr(&s).map(|p| p.amount))
                    .transpose()?;
                let balance = Amount::parse_inr(&balance_token)?.amount;

                row_counter += 1;
                out.push(RawTransaction {
                    import_id: import_id.to_string(),
                    source_file: extracted.source_file.clone(),
                    source_sha256: extracted.source_sha256.clone(),
                    source_page: page.page_number,
                    row_number: row_counter,
                    parser_version: parser_version.clone(),
                    parser_backend: extracted.backend,
                    txn_date,
                    description,
                    debit,
                    credit,
                    balance: Some(balance),
                });

                i = j.max(i + 1);
            }
        }

        Ok(out)
    }
}
