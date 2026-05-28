//! Loan Tracker tab — manually-entered loan positions plus payoff
//! strategy and good-loan / bad-loan classification.
//!
//! Storage: per-profile encrypted JSON at `mappings/loans.json`.
//!
//! ## Good / bad loan classification (P6)
//!
//! Decision P6 picked **net effective borrowing cost** as the v1
//! classifier. We approximate it as:
//!
//! - `effective_rate = interest_rate - (tax_benefit ? 2.5pp : 0)`
//!   (the 2.5pp shave is a rough proxy for the tax savings on a
//!   home-loan interest deduction at typical marginal rates).
//!
//! Classification thresholds:
//!
//! - `effective_rate <= 9.0%` → **Good loan** — usually asset-secured
//!   and/or tax-advantaged.
//! - `effective_rate >= 12.0%` → **Bad loan** — pay down aggressively.
//! - In between → **Watch** — neutral; pay the EMI but consider
//!   prepayment if there's headroom.
//!
//! These thresholds are intentionally simple; the rationale string we
//! return tells the user how the call was made so they can override
//! mentally.

use crate::state::AppState;
use crate::upload::session;
use fm_core::UserId;
use fm_crypto::{open, seal, KeyBytes};
use fm_storage::{StorageRepository, VersionedJson};
use rand::rngs::OsRng;
use rand::RngCore;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tauri::State;

const FILE_PATH: &str = "mappings/loans.json";
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoansDoc {
    #[serde(default)]
    pub loans: Vec<Loan>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loan {
    pub id: String,
    /// "Home" | "Car" | "Personal" | "Education" | "Credit Card" |
    /// "Business" | "Other" — free-form, the picker offers the common
    /// ones plus a custom escape hatch.
    pub loan_type: String,
    pub lender: String,
    /// Decimal strings throughout.
    pub principal_outstanding: String,
    /// Annual interest rate as a decimal percent, e.g. `"8.5"` for 8.5%.
    pub interest_rate: String,
    /// "Fixed" | "Floating".
    pub rate_type: String,
    pub remaining_tenure_months: u32,
    pub emi: String,
    /// 0 when no penalty. Stored as decimal percent.
    pub prepayment_penalty_pct: String,
    /// True for loans where interest / principal is tax-deductible
    /// under Indian tax rules (home loan, education loan typically).
    pub tax_benefit: bool,
    pub start_date: String,
    pub next_due_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub last_updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertLoanSpec {
    #[serde(default)]
    pub id: Option<String>,
    pub loan_type: String,
    pub lender: String,
    pub principal_outstanding: String,
    pub interest_rate: String,
    pub rate_type: String,
    pub remaining_tenure_months: u32,
    pub emi: String,
    #[serde(default)]
    pub prepayment_penalty_pct: Option<String>,
    #[serde(default)]
    pub tax_benefit: bool,
    pub start_date: String,
    pub next_due_date: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoansSummary {
    pub loan_count: u32,
    pub total_outstanding: String,
    pub total_monthly_emi: String,
    /// Weighted by outstanding balance.
    pub weighted_avg_rate: String,
    /// Decisions surfaced per-loan so the UI can colour-code them.
    pub classifications: Vec<LoanClassification>,
    /// Avalanche (highest-rate-first) and snowball (smallest-balance-first)
    /// orderings — same loans, different prepayment-priority sort.
    pub avalanche_order: Vec<String>,
    pub snowball_order: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoanClassification {
    pub loan_id: String,
    /// "good" | "watch" | "bad".
    pub verdict: &'static str,
    /// Effective annual rate after the tax-benefit shave, decimal percent.
    pub effective_rate: String,
    /// One-line user-facing rationale.
    pub rationale: String,
}

#[tauri::command]
pub fn list_loans(state: State<AppState>) -> Result<Vec<Loan>, String> {
    let (user, dek) = session(&state)?;
    Ok(load(&state, &user, &dek)?.loans)
}

#[tauri::command]
pub fn upsert_loan(spec: UpsertLoanSpec, state: State<AppState>) -> Result<Loan, String> {
    let (user, dek) = session(&state)?;
    let mut doc = load(&state, &user, &dek)?;

    let loan_type = spec.loan_type.trim();
    let lender = spec.lender.trim();
    if loan_type.is_empty() || lender.is_empty() {
        return Err("loan type and lender are required".into());
    }
    let rate_type = match spec.rate_type.trim().to_lowercase().as_str() {
        "fixed" => "Fixed",
        "floating" => "Floating",
        _ => return Err("rate type must be Fixed or Floating".into()),
    };
    let principal = parse_amount(&spec.principal_outstanding, "principal outstanding")?;
    let rate = parse_amount(&spec.interest_rate, "interest rate")?;
    let emi = parse_amount(&spec.emi, "EMI")?;
    if principal < Decimal::ZERO || emi < Decimal::ZERO {
        return Err("amounts must be non-negative".into());
    }
    if rate < Decimal::ZERO || rate > Decimal::new(60, 0) {
        return Err("interest rate must be between 0 and 60 percent".into());
    }
    let penalty = match spec.prepayment_penalty_pct.as_deref() {
        Some(s) if !s.trim().is_empty() => parse_amount(s, "prepayment penalty")?,
        _ => Decimal::ZERO,
    };

    let now = now_rfc3339();
    let notes = spec
        .notes
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let loan = match spec.id.as_deref().filter(|s| !s.is_empty()) {
        Some(existing_id) => {
            let target = doc
                .loans
                .iter_mut()
                .find(|l| l.id == existing_id)
                .ok_or_else(|| format!("loan {existing_id} not found"))?;
            target.loan_type = loan_type.to_string();
            target.lender = lender.to_string();
            target.principal_outstanding = format!("{principal:.2}");
            target.interest_rate = format!("{rate:.2}");
            target.rate_type = rate_type.to_string();
            target.remaining_tenure_months = spec.remaining_tenure_months;
            target.emi = format!("{emi:.2}");
            target.prepayment_penalty_pct = format!("{penalty:.2}");
            target.tax_benefit = spec.tax_benefit;
            target.start_date = spec.start_date;
            target.next_due_date = spec.next_due_date;
            target.notes = notes;
            target.last_updated_at = now;
            target.clone()
        }
        None => {
            let loan = Loan {
                id: make_loan_id(),
                loan_type: loan_type.to_string(),
                lender: lender.to_string(),
                principal_outstanding: format!("{principal:.2}"),
                interest_rate: format!("{rate:.2}"),
                rate_type: rate_type.to_string(),
                remaining_tenure_months: spec.remaining_tenure_months,
                emi: format!("{emi:.2}"),
                prepayment_penalty_pct: format!("{penalty:.2}"),
                tax_benefit: spec.tax_benefit,
                start_date: spec.start_date,
                next_due_date: spec.next_due_date,
                notes,
                last_updated_at: now,
            };
            doc.loans.push(loan.clone());
            loan
        }
    };

    save(&state, &user, &dek, &doc)?;
    Ok(loan)
}

#[tauri::command]
pub fn delete_loan(id: String, state: State<AppState>) -> Result<(), String> {
    let (user, dek) = session(&state)?;
    let mut doc = load(&state, &user, &dek)?;
    let before = doc.loans.len();
    doc.loans.retain(|l| l.id != id);
    if doc.loans.len() == before {
        return Err(format!("loan {id} not found"));
    }
    save(&state, &user, &dek, &doc)?;
    Ok(())
}

#[tauri::command]
pub fn loans_summary(state: State<AppState>) -> Result<LoansSummary, String> {
    let (user, dek) = session(&state)?;
    let doc = load(&state, &user, &dek)?;
    Ok(summarise(&doc.loans))
}

/// Sum of EMIs across all loans, used by the Dashboard's debt-burden
/// driver. Returns `Decimal::ZERO` when no loans are present.
pub(crate) fn total_monthly_emi(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
) -> Result<Decimal, String> {
    let doc = load(state, user, dek)?;
    let total = doc
        .loans
        .iter()
        .map(|l| Decimal::from_str(&l.emi).unwrap_or(Decimal::ZERO))
        .sum();
    Ok(total)
}

fn summarise(loans: &[Loan]) -> LoansSummary {
    let mut total_outstanding = Decimal::ZERO;
    let mut total_emi = Decimal::ZERO;
    let mut weighted_rate_num = Decimal::ZERO;
    let mut classifications: Vec<LoanClassification> = Vec::with_capacity(loans.len());

    for l in loans {
        let principal = Decimal::from_str(&l.principal_outstanding)
            .expect("principal_outstanding is normalized on write");
        let rate =
            Decimal::from_str(&l.interest_rate).expect("interest_rate is normalized on write");
        let emi = Decimal::from_str(&l.emi).expect("emi is normalized on write");

        total_outstanding += principal;
        total_emi += emi;
        weighted_rate_num += rate * principal;

        let effective_rate = effective_rate_of(rate, l.tax_benefit);
        let (verdict, rationale) = classify(effective_rate, l.tax_benefit, &l.loan_type);
        classifications.push(LoanClassification {
            loan_id: l.id.clone(),
            verdict,
            effective_rate: format!("{effective_rate:.2}"),
            rationale,
        });
    }

    let weighted_avg_rate = if total_outstanding > Decimal::ZERO {
        format!("{:.2}", weighted_rate_num / total_outstanding)
    } else {
        "0.00".to_string()
    };

    // Payoff strategies — return loan IDs ordered by priority.
    let mut avalanche: Vec<(String, Decimal, Decimal)> = loans
        .iter()
        .map(|l| {
            let rate = Decimal::from_str(&l.interest_rate).unwrap_or(Decimal::ZERO);
            let principal = Decimal::from_str(&l.principal_outstanding).unwrap_or(Decimal::ZERO);
            (l.id.clone(), rate, principal)
        })
        .collect();
    avalanche.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    let avalanche_order = avalanche.into_iter().map(|(id, _, _)| id).collect();

    let mut snowball: Vec<(String, Decimal)> = loans
        .iter()
        .map(|l| {
            let principal = Decimal::from_str(&l.principal_outstanding).unwrap_or(Decimal::ZERO);
            (l.id.clone(), principal)
        })
        .collect();
    snowball.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let snowball_order = snowball.into_iter().map(|(id, _)| id).collect();

    LoansSummary {
        loan_count: loans.len() as u32,
        total_outstanding: format!("{total_outstanding:.2}"),
        total_monthly_emi: format!("{total_emi:.2}"),
        weighted_avg_rate,
        classifications,
        avalanche_order,
        snowball_order,
    }
}

const TAX_BENEFIT_SHAVE_PP: Decimal = Decimal::from_parts(25, 0, 0, false, 1); // 2.5 percentage points

fn effective_rate_of(nominal_rate: Decimal, tax_benefit: bool) -> Decimal {
    if tax_benefit {
        let shaved = nominal_rate - TAX_BENEFIT_SHAVE_PP;
        if shaved < Decimal::ZERO {
            Decimal::ZERO
        } else {
            shaved
        }
    } else {
        nominal_rate
    }
}

const GOOD_LOAN_RATE_THRESHOLD: Decimal = Decimal::from_parts(9, 0, 0, false, 0); // 9.00
const BAD_LOAN_RATE_THRESHOLD: Decimal = Decimal::from_parts(12, 0, 0, false, 0); // 12.00

fn classify(effective_rate: Decimal, tax_benefit: bool, loan_type: &str) -> (&'static str, String) {
    let lt = loan_type.trim().to_lowercase();
    // Credit-card revolving debt is bad almost regardless of nominal rate
    // — it's typically 35%+ but a user might enter a promotional teaser.
    if lt == "credit card" {
        return (
            "bad",
            "Credit-card debt — rates effectively reset to 35%+ after promos. Pay this down first."
                .into(),
        );
    }
    if effective_rate <= GOOD_LOAN_RATE_THRESHOLD {
        let why = if tax_benefit {
            format!(
                "Effective rate {effective_rate:.2}% after tax shave is below the {GOOD_LOAN_RATE_THRESHOLD:.0}% good-loan threshold. Tax-advantaged — pay the EMI, prepay only with idle cash."
            )
        } else {
            format!(
                "Effective rate {effective_rate:.2}% is below the {GOOD_LOAN_RATE_THRESHOLD:.0}% threshold. Pay the EMI on time; aggressive prepayment beats most other deployments only if your portfolio's expected return is lower."
            )
        };
        ("good", why)
    } else if effective_rate >= BAD_LOAN_RATE_THRESHOLD {
        (
            "bad",
            format!(
                "Effective rate {effective_rate:.2}% is at or above the {BAD_LOAN_RATE_THRESHOLD:.0}% bad-loan threshold. Prioritize prepaying this — most asset returns can't beat the interest you're paying."
            ),
        )
    } else {
        (
            "watch",
            format!(
                "Effective rate {effective_rate:.2}% sits between {GOOD_LOAN_RATE_THRESHOLD:.0}% and {BAD_LOAN_RATE_THRESHOLD:.0}%. Pay the EMI; consider prepayment if you have surplus cash."
            ),
        )
    }
}

fn parse_amount(raw: &str, label: &str) -> Result<Decimal, String> {
    let trimmed = raw.trim().replace(',', "");
    Decimal::from_str(&trimmed).map_err(|e| format!("{label} '{raw}' is not a decimal: {e}"))
}

fn load(state: &State<AppState>, user: &UserId, dek: &KeyBytes) -> Result<LoansDoc, String> {
    if !state
        .storage
        .exists(user, FILE_PATH)
        .map_err(|e| e.to_string())?
    {
        return Ok(LoansDoc::default());
    }
    let sealed = state
        .storage
        .read(user, FILE_PATH)
        .map_err(|e| e.to_string())?;
    let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
    let parsed: VersionedJson<LoansDoc> =
        serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
    if parsed.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "loans file has unsupported schema version {}",
            parsed.schema_version
        ));
    }
    Ok(parsed.data)
}

fn save(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    doc: &LoansDoc,
) -> Result<(), String> {
    let envelope = VersionedJson::new(SCHEMA_VERSION, doc);
    let plaintext = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
    let sealed = seal(dek, &plaintext).map_err(|e| e.to_string())?;
    state
        .storage
        .write(user, FILE_PATH, &sealed)
        .map_err(|e| e.to_string())
}

fn make_loan_id() -> String {
    let mut rnd = [0u8; 6];
    OsRng.fill_bytes(&mut rnd);
    format!(
        "loan:{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        rnd[0], rnd[1], rnd[2], rnd[3], rnd[4], rnd[5]
    )
}

fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loan(id: &str, loan_type: &str, rate: &str, principal: &str, emi: &str, tax: bool) -> Loan {
        Loan {
            id: id.into(),
            loan_type: loan_type.into(),
            lender: "BANK".into(),
            principal_outstanding: principal.into(),
            interest_rate: rate.into(),
            rate_type: "Floating".into(),
            remaining_tenure_months: 120,
            emi: emi.into(),
            prepayment_penalty_pct: "0.00".into(),
            tax_benefit: tax,
            start_date: "2024-01-01".into(),
            next_due_date: "2026-06-01".into(),
            notes: None,
            last_updated_at: "2026-05-28T00:00:00Z".into(),
        }
    }

    #[test]
    fn empty_summary_returns_zeros() {
        let s = summarise(&[]);
        assert_eq!(s.loan_count, 0);
        assert_eq!(s.total_outstanding, "0.00");
        assert_eq!(s.total_monthly_emi, "0.00");
        assert_eq!(s.weighted_avg_rate, "0.00");
    }

    #[test]
    fn home_loan_with_tax_benefit_classified_good() {
        // 9% nominal, tax-advantaged → effective 6.5% → good.
        let l = loan("h", "Home", "9.00", "5000000.00", "45000.00", true);
        let s = summarise(&[l]);
        assert_eq!(s.classifications[0].verdict, "good");
        assert_eq!(s.classifications[0].effective_rate, "6.50");
    }

    #[test]
    fn personal_loan_at_14_pct_classified_bad() {
        let l = loan("p", "Personal", "14.00", "200000.00", "8000.00", false);
        let s = summarise(&[l]);
        assert_eq!(s.classifications[0].verdict, "bad");
        assert_eq!(s.classifications[0].effective_rate, "14.00");
    }

    #[test]
    fn credit_card_always_bad_even_at_low_teaser_rate() {
        let l = loan("c", "Credit Card", "3.00", "50000.00", "2500.00", false);
        let s = summarise(&[l]);
        assert_eq!(s.classifications[0].verdict, "bad");
        assert!(s.classifications[0].rationale.contains("Credit-card"));
    }

    #[test]
    fn car_loan_at_10_pct_classified_watch() {
        let l = loan("car", "Car", "10.50", "500000.00", "12000.00", false);
        let s = summarise(&[l]);
        assert_eq!(s.classifications[0].verdict, "watch");
    }

    #[test]
    fn weighted_avg_rate_uses_principal_as_weight() {
        // 100k @ 8%, 900k @ 10% → weighted avg = (8*100 + 10*900)/1000 = 9.8
        let loans = vec![
            loan("a", "Home", "8.00", "100000.00", "0.00", true),
            loan("b", "Car", "10.00", "900000.00", "0.00", false),
        ];
        let s = summarise(&loans);
        assert_eq!(s.weighted_avg_rate, "9.80");
    }

    #[test]
    fn avalanche_orders_by_rate_descending() {
        let loans = vec![
            loan("a", "Home", "8.00", "100000.00", "0.00", true),
            loan("b", "Personal", "14.00", "200000.00", "0.00", false),
            loan("c", "Car", "10.00", "500000.00", "0.00", false),
        ];
        let s = summarise(&loans);
        assert_eq!(s.avalanche_order, vec!["b", "c", "a"]);
    }

    #[test]
    fn snowball_orders_by_principal_ascending() {
        let loans = vec![
            loan("a", "Home", "8.00", "100000.00", "0.00", true),
            loan("b", "Personal", "14.00", "200000.00", "0.00", false),
            loan("c", "Car", "10.00", "50000.00", "0.00", false),
        ];
        let s = summarise(&loans);
        assert_eq!(s.snowball_order, vec!["c", "a", "b"]);
    }

    #[test]
    fn tax_shave_never_drops_below_zero() {
        // Hypothetical 1% loan with tax benefit shouldn't go negative.
        assert_eq!(effective_rate_of(Decimal::new(100, 2), true), Decimal::ZERO);
    }
}
