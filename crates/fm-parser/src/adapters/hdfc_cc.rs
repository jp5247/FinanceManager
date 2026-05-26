//! HDFC Bank credit-card statement adapter.
//!
//! Tuned against six fixture statements seen in the Phase-0 spike:
//! Regalia, Marriott, and Rupay cards across April and May 2026 cycles.
//!
//! ## Row anatomy in the extracted text
//!
//! Each transaction row begins with a literal `DD/MM/YYYY| HH:MM` token,
//! followed by free-text description (often wrapping across 2–3 lines in
//! the raw stream), then an amount near the end:
//!
//! ```text
//! 21/04/2026| 00:00 IGST-VPS2711251279864-RATE 18.0 -27 (Ref# 09999999980421000325273) C 170.64 l
//! 23/04/2026| 12:32 FORTPOINTMUMBAIMUMBAI C 1,582.00 l
//! 04/05/2026| 19:20 BPPY CC PAYMENT DP016124192045RgnrE (Ref# ST261250083000010053740) + C 21,447.00 l
//! ```
//!
//! - The leading `C` before the numeric amount is the rupee `₹` glyph being
//!   mis-encoded by pdfium's font extraction.
//! - A `+` prefix on the currency token means money in (payment received,
//!   refund). Absent → money out (purchase).
//! - The trailing `l` is the Purchase Indicator bullet; we ignore it.
//!
//! Wrap-lines belonging to the same transaction continue until the next
//! `DD/MM/YYYY|` anchor OR a hard break (empty line / `Page N of M` footer
//! / known section header).

use crate::adapter::{BankAdapter, ParseError};
use crate::extracted::ExtractedPdf;
use crate::raw_txn::RawTransaction;
use fm_core::{Amount, Sign};
use regex::Regex;
use std::sync::OnceLock;

pub struct HdfcCreditCardAdapter;

const ADAPTER_ID: &str = "hdfc-cc";
const ADAPTER_VERSION: &str = "1.0.0";

impl HdfcCreditCardAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for HdfcCreditCardAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn anchor_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^(\d{2})/(\d{2})/(\d{4})\|\s*(\d{2}:\d{2})\s+(.*)$").expect("static regex")
    })
}

fn amount_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[\d,]+\.\d{2}").expect("static regex"))
}

/// End-of-row amount marker: optional `+`, the corrupted rupee glyph
/// (`C` / `₹`), the amount, and an optional purchase-indicator suffix
/// (`l` in the original, but pdfium occasionally extracts it as `I`).
///
/// Used to stop wrap-line absorption: once a row has its currency-marked
/// terminator, anything after it (e.g. an inline "Rewards Program Points
/// Summary" table) belongs to the next logical block, not this transaction.
fn row_terminator_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?:\+\s*)?(?:C|₹)\s*[\d,]+\.\d{2}\s*[lI]?\s*$").expect("static regex")
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
    // Known HDFC CC section headers that interrupt transaction rows. We've
    // seen pdfium emit several spelling variants of the rewards section
    // header depending on the card product, so list them all.
    matches!(
        t,
        "Domestic Transactions"
            | "International Transactions"
            | "Past Dues"
            | "Reward Points Summary"
            | "Rewards Program Points Summary"
            | "Cash Back Summary"
            | "Rewards Summary"
    )
}

impl BankAdapter for HdfcCreditCardAdapter {
    fn id(&self) -> &'static str {
        ADAPTER_ID
    }

    fn version(&self) -> &'static str {
        ADAPTER_VERSION
    }

    fn detect(&self, extracted: &ExtractedPdf) -> bool {
        let name = extracted.source_file.to_lowercase();
        let hits_filename = name.contains("hdfc")
            && (name.contains("regalia")
                || name.contains("marriott")
                || name.contains("rupay")
                || name.contains("billedstatements"));
        if hits_filename {
            return true;
        }
        extracted.pages.iter().any(|p| {
            let lower = p.text.to_lowercase();
            lower.contains("hdfc bank credit cards")
                || lower.contains("regalia gold credit card")
                || lower.contains("hdfc credit card statement")
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
                let line = lines[i];
                let Some(caps) = anchor_re().captures(line) else {
                    i += 1;
                    continue;
                };
                let day: u32 = caps[1]
                    .parse()
                    .map_err(|_| ParseError::BadDate(format!("non-numeric day in: {line}")))?;
                let month: u32 = caps[2]
                    .parse()
                    .map_err(|_| ParseError::BadDate(format!("non-numeric month in: {line}")))?;
                let year: u32 = caps[3]
                    .parse()
                    .map_err(|_| ParseError::BadDate(format!("non-numeric year in: {line}")))?;
                if !(1..=31).contains(&day)
                    || !(1..=12).contains(&month)
                    || !(2000..=2100).contains(&year)
                {
                    return Err(ParseError::BadDate(format!("out of range in: {line}")));
                }

                let mut joined = caps[5].to_string();
                let mut j = i + 1;
                // Absorb wrap-lines until we hit the next transaction anchor,
                // a hard break, OR the row has already captured its amount
                // terminator. The last condition prevents inline section
                // tables (rewards summaries, cash-back tables) that the PDF
                // extractor places right under a transaction row from being
                // glued onto the description.
                while !row_terminator_re().is_match(joined.trim_end())
                    && j < lines.len()
                    && anchor_re().captures(lines[j]).is_none()
                    && !is_hard_break(lines[j])
                {
                    joined.push(' ');
                    joined.push_str(lines[j].trim());
                    j += 1;
                }

                let advance = j.max(i + 1);

                if let Some(txn) = parse_row(
                    &joined,
                    year,
                    month,
                    day,
                    import_id,
                    extracted,
                    page.page_number,
                    row_counter + 1,
                    &parser_version,
                )? {
                    out.push(txn);
                    row_counter += 1;
                }
                i = advance;
            }
        }

        Ok(out)
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_row(
    joined: &str,
    year: u32,
    month: u32,
    day: u32,
    import_id: &str,
    extracted: &ExtractedPdf,
    page: u32,
    row_number: u32,
    parser_version: &str,
) -> Result<Option<RawTransaction>, ParseError> {
    let mat = match amount_re().find_iter(joined).last() {
        Some(m) => m,
        None => return Ok(None),
    };
    let amount_token = &joined[mat.start()..mat.end()];
    let prefix = &joined[..mat.start()];

    // Direction: a `+` marker (possibly followed by spaces and the corrupted
    // rupee glyph `C`) immediately before the amount means credit. Anything
    // else is debit.
    let trimmed = prefix.trim_end_matches(|c: char| c.is_whitespace() || c == 'C' || c == '₹');
    let is_credit = trimmed.ends_with('+');

    let parsed = Amount::parse_inr(amount_token)?;
    debug_assert!(
        parsed.sign == Sign::Unmarked,
        "raw amount token shouldn't carry sign"
    );

    // Strip trailing currency glyphs + sign from the description.
    let cleaned_prefix =
        prefix.trim_end_matches(|c: char| c.is_whitespace() || c == 'C' || c == '₹' || c == '+');
    let description = cleaned_prefix.trim().to_string();

    if description.is_empty() {
        return Ok(None);
    }

    let txn_date = format!("{year:04}-{month:02}-{day:02}");

    let (debit, credit) = if is_credit {
        (None, Some(parsed.amount))
    } else {
        (Some(parsed.amount), None)
    };

    Ok(Some(RawTransaction {
        import_id: import_id.to_string(),
        source_file: extracted.source_file.clone(),
        source_sha256: extracted.source_sha256.clone(),
        source_page: page,
        row_number,
        parser_version: parser_version.to_string(),
        parser_backend: extracted.backend,
        txn_date,
        description,
        debit,
        credit,
        balance: None,
        category: None,
        category_rule_id: None,
    }))
}
