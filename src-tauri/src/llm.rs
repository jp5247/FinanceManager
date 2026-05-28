//! Gemini-backed categorization client.
//!
//! Sends a batched prompt of merchant names + direction to Google's
//! Generative Language API. Per OD-5, **only** these two fields per row
//! leave the device — never amounts, account masks, dates, ref-ids, or
//! customer-identifying narration.
//!
//! Errors are non-fatal: if the LLM is unreachable, mis-configured, or
//! returns gibberish, the upload still succeeds — the affected rows just
//! stay uncategorized.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

/// Pause before the single retry on a 429. Keep it short enough that the user
/// doesn't think the app froze, long enough that a per-minute quota window
/// has a chance to roll over (Gemini's per-minute counter resets every 60s).
const RATE_LIMIT_RETRY_DELAY: Duration = Duration::from_secs(8);

/// What the LLM is told to choose from. Kept in sync with `COMMON_CATEGORIES`
/// in `src/categories.ts` — single source of truth for the canonical taxonomy.
const ALLOWED_CATEGORIES: &[&str] = &[
    // Bills
    "Credit card bill",
    "Electricity bill",
    "Gas bill",
    "Mobile/Internet bill",
    "Laundary bill",
    // EMIs
    "Home Loan EMI",
    "Car loan EMI",
    "CC EMI",
    // Lifestyle expenses
    "Food expenses",
    "Hotel/Vacation expenses",
    "Fuel expenses",
    "Vehicle repairs/maintenance",
    "Medical expenses",
    "Groceries",
    "Transportation",
    "Personal care",
    "Shopping",
    "Entertainment",
    "Gifts",
    // Income
    "Salary",
    "Side Hustle",
    "Interest",
    "Dividend",
    "Refund",
    // Wealth-building
    "SIP",
    "Stock purchase",
    "FD",
    // System / bookkeeping
    "Bank Transfer",
    "EMI Conversion",
    "Uncategorized",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Debit,
    Credit,
}

impl Direction {
    fn as_str(&self) -> &'static str {
        match self {
            Direction::Debit => "outgoing",
            Direction::Credit => "incoming",
        }
    }
}

/// One row's worth of input — only the merchant string + direction.
#[derive(Clone, Debug)]
pub struct LookupItem {
    pub merchant: String,
    pub direction: Direction,
}

#[derive(Clone, Debug)]
pub struct LookupResult {
    pub merchant: String,
    pub category: String,
}

/// Send the batched prompt to Gemini. Returns one result per input item;
/// items that fail to classify are returned with category =
/// `"Uncategorized"` so the caller doesn't have to track gaps.
///
/// Last-line OD-5 defense lives here: any item whose merchant string still
/// looks PII-shaped after [`crate::merchant`]-level extraction (long digit
/// runs, account masks, UPI handle artifacts, amount-like tokens) is dropped
/// before the request leaves the device. Dropped items come back as
/// Uncategorized so the caller doesn't see a gap.
pub fn categorize_via_gemini(
    api_key: &str,
    model: &str,
    items: &[LookupItem],
) -> Result<Vec<LookupResult>, String> {
    if api_key.is_empty() {
        return Err("Gemini API key not configured".into());
    }
    if items.is_empty() {
        return Ok(Vec::new());
    }

    // Split safe vs unsafe up front. We send only the safe subset to Gemini;
    // unsafe entries are stamped Uncategorized in the final output.
    let safe_mask: Vec<bool> = items.iter().map(|it| is_od5_safe(&it.merchant)).collect();
    let safe_items: Vec<LookupItem> = items
        .iter()
        .zip(safe_mask.iter())
        .filter(|(_, &ok)| ok)
        .map(|(it, _)| it.clone())
        .collect();

    if safe_items.is_empty() {
        // Everything got rejected by the OD-5 guard — no network call.
        return Ok(items
            .iter()
            .map(|it| LookupResult {
                merchant: it.merchant.clone(),
                category: "Uncategorized".to_string(),
            })
            .collect());
    }

    let prompt = build_prompt(&safe_items);
    let body = serde_json::json!({
        "contents": [
            { "parts": [ { "text": prompt } ] }
        ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": {
                "type": "object",
                "properties": {
                    "results": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "index": { "type": "integer" },
                                "category": { "type": "string" }
                            },
                            "required": ["index", "category"]
                        }
                    }
                },
                "required": ["results"]
            },
            "temperature": 0.1
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        urlencode(model)
    );

    let body_text = send_with_retry(&url, api_key, &body)?;
    let safe_results = parse_gemini_response(&body_text, &safe_items)?;

    // Re-merge: walk the original items, taking from safe_results when the
    // mask says safe, stamping Uncategorized otherwise.
    let mut safe_iter = safe_results.into_iter();
    let merged = items
        .iter()
        .zip(safe_mask.iter())
        .map(|(it, &ok)| {
            if ok {
                safe_iter.next().unwrap_or(LookupResult {
                    merchant: it.merchant.clone(),
                    category: "Uncategorized".to_string(),
                })
            } else {
                LookupResult {
                    merchant: it.merchant.clone(),
                    category: "Uncategorized".to_string(),
                }
            }
        })
        .collect();
    Ok(merged)
}

/// Last-chance OD-5 sanity check: reject merchant strings that still look
/// like they carry account numbers, UPI handles, masked digits, or amounts.
/// The merchant extractor strips these at parse time; this is belt-and-
/// suspenders for adapters that haven't been hardened yet.
fn is_od5_safe(merchant: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let unsafe_re = R.get_or_init(|| {
        // - `\d{6,}` long digit runs (ref-IDs, account numbers, mobile #s)
        // - `@` UPI handle that survived prefix stripping
        // - `X{4,}` masked account/card digits
        // - `\d+\.\d{2}` decimal amount that leaked into description
        Regex::new(r"\d{6,}|@|X{4,}|\d+\.\d{2}").expect("static regex")
    });
    let trimmed = merchant.trim();
    if trimmed.is_empty() {
        return false;
    }
    !unsafe_re.is_match(trimmed)
}

/// One retry on 429 — Gemini's free tier has both a per-minute and per-day
/// quota. A short pause covers the per-minute window; the per-day case still
/// needs to be surfaced as a friendly error.
fn send_with_retry(url: &str, api_key: &str, body: &serde_json::Value) -> Result<String, String> {
    for attempt in 0..2 {
        let resp = ureq::post(url)
            .set("Content-Type", "application/json")
            .set("x-goog-api-key", api_key)
            .timeout(Duration::from_secs(45))
            .send_json(body.clone());

        match resp {
            Ok(r) => return r.into_string().map_err(|e| format!("read body: {e}")),
            Err(ureq::Error::Status(429, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                if attempt == 0 && !is_per_day_quota(&detail) {
                    thread::sleep(RATE_LIMIT_RETRY_DELAY);
                    continue;
                }
                return Err(friendly_429_message(&detail));
            }
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                return Err(format!(
                    "Gemini returned HTTP {code}: {}",
                    detail.chars().take(300).collect::<String>()
                ));
            }
            Err(e) => return Err(format!("Gemini request failed: {e}")),
        }
    }
    unreachable!("retry loop always returns")
}

/// Gemini reports per-day exhaustion with `quotaId` strings that contain
/// `PerDay`. If we see that, retrying in 8s is pointless — surface it.
fn is_per_day_quota(detail: &str) -> bool {
    detail.contains("PerDay") || detail.contains("per day")
}

fn friendly_429_message(detail: &str) -> String {
    if is_per_day_quota(detail) {
        "Gemini daily quota exhausted. Free tier allows ~1,500 requests/day on \
         gemini-2.0-flash. Switch to a different model in settings, or try again \
         tomorrow."
            .to_string()
    } else {
        "Gemini per-minute rate limit hit and retry didn't clear it. Try a \
         model with a higher RPM (e.g. gemini-2.0-flash-lite) in settings, or \
         wait a minute and re-upload."
            .to_string()
    }
}

fn build_prompt(items: &[LookupItem]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(1024);
    out.push_str(
        "You categorize Indian bank-statement merchants into ONE category from this exact list:\n",
    );
    for c in ALLOWED_CATEGORIES {
        out.push_str("- ");
        out.push_str(c);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(
        "Use the direction (incoming = money in, outgoing = money out) to disambiguate. \
         Examples for the Indian retail context: 'INDIAN RAILWAY' incoming via NACH/ACH is \
         'Dividend' (IRFC shares); outgoing to 'IRCTC' is 'Transportation'. Swiggy / Zomato / \
         restaurants → 'Food expenses'. Blinkit / Zepto / BigBasket → 'Groceries'. Amazon / \
         Flipkart / Myntra → 'Shopping'. Uber / Rapido / metro / bus → 'Transportation'. \
         BookMyShow / Netflix / cinema → 'Entertainment'. CRED / Payment on CRED → \
         'Credit card bill'. \
         Loan EMIs: choose 'Home Loan EMI' / 'Car loan EMI' / 'CC EMI' based on what the \
         merchant string suggests; default to 'CC EMI' for credit-card statement rows. \
         Side gigs (freelance, consulting, content) → 'Side Hustle'. \
         Credit-card EMI bookkeeping rows (e.g. 'AGGREGATOR-EMI-OFFUSCREDIT', 'EMI BOOKING', \
         or any 'EMI <merchant>' row that represents the loan principal being booked rather \
         than an actual monthly installment) should be 'EMI Conversion' — these net to zero \
         across the loan disbursement. \
         If the merchant is a person's name with no business context, lean toward \
         'Bank Transfer' (own-account) or the most likely expense category. \
         If you cannot confidently pick, return 'Uncategorized'.\n\n",
    );
    out.push_str("Return JSON matching the response schema. Each result's `index` must match the input item's number (1-based).\n\n");
    out.push_str("Categorize:\n");
    for (i, item) in items.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}. {} ({})",
            i + 1,
            item.merchant,
            item.direction.as_str()
        );
    }
    out
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    parts: Option<Vec<CandidatePart>>,
}

#[derive(Debug, Deserialize)]
struct CandidatePart {
    text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StructuredOutput {
    results: Vec<StructuredItem>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StructuredItem {
    index: u32,
    category: String,
}

fn parse_gemini_response(body: &str, items: &[LookupItem]) -> Result<Vec<LookupResult>, String> {
    let parsed: GeminiResponse =
        serde_json::from_str(body).map_err(|e| format!("parse Gemini envelope: {e}"))?;
    let text = parsed
        .candidates
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|p| p.into_iter().next())
        .and_then(|p| p.text)
        .ok_or_else(|| "Gemini response had no text part".to_string())?;

    let structured: StructuredOutput =
        serde_json::from_str(&text).map_err(|e| format!("parse structured output: {e}"))?;

    // Map index → category, then build a result per input. Missing items
    // default to Uncategorized rather than failing the whole batch.
    let mut by_index = std::collections::HashMap::new();
    for item in structured.results {
        if let Some(cat) = sanitize_category(&item.category) {
            by_index.insert(item.index, cat);
        }
    }
    let out: Vec<LookupResult> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let idx = (i as u32) + 1;
            let category = by_index
                .remove(&idx)
                .unwrap_or_else(|| "Uncategorized".to_string());
            LookupResult {
                merchant: item.merchant.clone(),
                category,
            }
        })
        .collect();
    Ok(out)
}

/// Coerce LLM output into a value the rest of the app understands. If the
/// model returns something outside the allowed list, drop it — we'd rather
/// leave the row uncategorized than ship a category the picker doesn't show.
fn sanitize_category(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    for c in ALLOWED_CATEGORIES {
        if trimmed.eq_ignore_ascii_case(c) {
            return Some((*c).to_string());
        }
    }
    None
}

/// Minimal URL component encoder for API key + model name. We deliberately
/// avoid pulling in a heavy URL crate just for this.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_all_categories_and_items() {
        let items = vec![
            LookupItem {
                merchant: "SWIGGY INSTAMART".into(),
                direction: Direction::Debit,
            },
            LookupItem {
                merchant: "INDIAN RAILWAY".into(),
                direction: Direction::Credit,
            },
        ];
        let p = build_prompt(&items);
        assert!(p.contains("Groceries"));
        assert!(p.contains("Dividend"));
        assert!(p.contains("SWIGGY INSTAMART"));
        assert!(p.contains("INDIAN RAILWAY"));
        assert!(p.contains("(outgoing)"));
        assert!(p.contains("(incoming)"));
    }

    #[test]
    fn prompt_does_not_leak_amounts_or_accounts() {
        let items = vec![LookupItem {
            merchant: "MERCHANT".into(),
            direction: Direction::Debit,
        }];
        let p = build_prompt(&items);
        // Random sanity checks that the prompt-building helper would never
        // include things outside the merchant string.
        assert!(!p.contains("12345.67"));
        assert!(!p.contains("XXXXXX"));
        assert!(!p.contains("/04/26"));
    }

    #[test]
    fn parses_well_formed_response() {
        let items = vec![
            LookupItem {
                merchant: "SWIGGY INSTAMART".into(),
                direction: Direction::Debit,
            },
            LookupItem {
                merchant: "INDIAN RAILWAY".into(),
                direction: Direction::Credit,
            },
        ];
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": "{\"results\":[{\"index\":1,\"category\":\"Groceries\"},{\"index\":2,\"category\":\"Dividend\"}]}"
                    }]
                }
            }]
        }"#;
        let r = parse_gemini_response(body, &items).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].category, "Groceries");
        assert_eq!(r[1].category, "Dividend");
    }

    #[test]
    fn parses_partial_response_filling_gaps_with_uncategorized() {
        let items = vec![
            LookupItem {
                merchant: "A".into(),
                direction: Direction::Debit,
            },
            LookupItem {
                merchant: "B".into(),
                direction: Direction::Debit,
            },
            LookupItem {
                merchant: "C".into(),
                direction: Direction::Debit,
            },
        ];
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": "{\"results\":[{\"index\":2,\"category\":\"Groceries\"}]}"
                    }]
                }
            }]
        }"#;
        let r = parse_gemini_response(body, &items).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].category, "Uncategorized");
        assert_eq!(r[1].category, "Groceries");
        assert_eq!(r[2].category, "Uncategorized");
    }

    #[test]
    fn rejects_off_list_category() {
        let items = vec![LookupItem {
            merchant: "X".into(),
            direction: Direction::Debit,
        }];
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": "{\"results\":[{\"index\":1,\"category\":\"Cryptocurrency\"}]}"
                    }]
                }
            }]
        }"#;
        let r = parse_gemini_response(body, &items).unwrap();
        // Off-list category dropped → Uncategorized fallback.
        assert_eq!(r[0].category, "Uncategorized");
    }

    #[test]
    fn rejects_missing_text_part() {
        let items = vec![LookupItem {
            merchant: "X".into(),
            direction: Direction::Debit,
        }];
        let body = r#"{"candidates":[]}"#;
        assert!(parse_gemini_response(body, &items).is_err());
    }

    #[test]
    fn od5_guard_rejects_long_digit_runs() {
        assert!(!is_od5_safe("SOMEMERCHANT 1234567"));
        assert!(!is_od5_safe("UPI-REF 09999999980421000325273"));
    }

    #[test]
    fn od5_guard_rejects_masked_or_handle_artifacts() {
        assert!(!is_od5_safe("XXXX1234 SWIPE"));
        assert!(!is_od5_safe("john@oksbi"));
        assert!(!is_od5_safe("INDIA XXXX"));
    }

    #[test]
    fn od5_guard_rejects_embedded_amounts() {
        assert!(!is_od5_safe("MERCHANT 1582.00"));
    }

    #[test]
    fn od5_guard_accepts_clean_merchants() {
        assert!(is_od5_safe("SWIGGY INSTAMART"));
        assert!(is_od5_safe("Amazon Pay"));
        assert!(is_od5_safe("MAHESH SHETTY G M"));
        assert!(is_od5_safe("FORTPOINTMUMBAIMUMBAI"));
    }

    #[test]
    fn od5_guard_rejects_empty() {
        assert!(!is_od5_safe(""));
        assert!(!is_od5_safe("   "));
    }

    #[test]
    fn per_day_quota_detected_from_quota_id() {
        let detail = r#"{"error":{"details":[{"quotaId":"GenerateContentRequestsPerDayPerProjectPerModel"}]}}"#;
        assert!(is_per_day_quota(detail));
        assert!(friendly_429_message(detail).contains("daily quota"));
    }

    #[test]
    fn per_minute_quota_friendly_message() {
        let detail = r#"{"error":{"details":[{"quotaId":"GenerateContentRequestsPerMinutePerProjectPerModel"}]}}"#;
        assert!(!is_per_day_quota(detail));
        assert!(friendly_429_message(detail).contains("per-minute"));
    }

    #[test]
    fn urlencode_handles_reserved_chars() {
        assert_eq!(urlencode("abc"), "abc");
        assert_eq!(urlencode("AIzaSyA_BC"), "AIzaSyA_BC");
        // Real keys are alphanumeric + `_-`, but be defensive.
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("a/b"), "a%2Fb");
    }
}
