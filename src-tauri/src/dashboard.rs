//! Dashboard aggregation — reads every import for the unlocked profile and
//! computes the cross-statement totals the Dashboard tab renders.
//!
//! ## Money classification (matches FinanceManager.md decisions)
//!
//! - **Income** — rows categorized `Salary`, `Dividend`, `Interest`, or
//!   `Refund`.
//! - **Transfer** (P3) — rows categorized `Credit Card Payment` or
//!   `Bank Transfer`. Excluded from both income and expense; surfaced
//!   separately so the headline numbers don't double-count.
//! - **Expense** — everything else, including `ATM / Cash` (P4: cash
//!   withdrawn is treated as spent by default).
//!
//! This is intentionally simple — heuristic pairing of debit/credit rows
//! across statements is a Phase-2 problem. For v1 we trust the existing
//! categorization to label cross-account movements correctly.

use crate::state::AppState;
use crate::upload::{list_imports_internal, session, FileMeta};
use fm_core::UserId;
use fm_crypto::{open, KeyBytes};
use fm_parser::RawTransaction;
use fm_storage::{StorageRepository, VersionedJson};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

const RAW_TXN_SCHEMA: u32 = 1;

/// Hard cap on a single sealed `raw-transactions.json` we'll decrypt + parse
/// into memory. Real statements top out around 100 KB; 16 MB is plenty of
/// headroom while still bounding a maliciously-crafted (or corrupted)
/// file's ability to wedge the dashboard.
const MAX_SEALED_TXN_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    /// Number of imports the aggregate includes. 0 means "no statements
    /// uploaded yet" → UI shows an empty-state.
    pub import_count: u32,

    /// Earliest and latest transaction dates seen across all imports
    /// (ISO `YYYY-MM-DD`). `None` if there are no transactions.
    pub period_start: Option<String>,
    pub period_end: Option<String>,

    pub transaction_count: u32,

    /// Decimal strings, e.g. `"42500.00"`.
    pub total_income: String,
    pub total_expense: String,
    pub net_savings: String,

    /// Transfers between own accounts — credit-card payments, account-to-
    /// account moves. Counted separately so income/expense aren't inflated.
    pub transfer_count: u32,
    pub transfer_total: String,

    /// Per-category breakdown, sorted by total descending. Combined across
    /// every import in the profile.
    pub category_totals: Vec<CategoryTotal>,

    /// Per-calendar-month buckets, ordered chronologically. Each entry has
    /// the month key (`YYYY-MM`) plus income/expense/net for that month.
    pub monthly_trend: Vec<MonthlyBucket>,

    /// Financial-health composite score (0–100) plus per-driver breakdown.
    /// Weights per FinanceManager.md §11.4 P5: 40/25/20/15.
    pub health_score: HealthScore,

    /// Heuristic recommendations surfaced in the "Fix my finance" panel.
    pub recommendations: Vec<Recommendation>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyBucket {
    /// `YYYY-MM`, e.g. `"2026-04"`.
    pub month: String,
    pub income: String,
    pub expense: String,
    pub net: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthScore {
    /// Composite score 0–100 (rounded to integer).
    pub composite: u32,
    pub drivers: Vec<HealthDriver>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthDriver {
    pub key: &'static str,
    pub label: &'static str,
    /// Raw driver score 0–100 before weighting.
    pub score: u32,
    /// Weight 0.0–1.0; the four drivers' weights sum to 1.0.
    pub weight: f32,
    /// Short one-line explainer rendered next to the bar.
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    /// Decimal string; `null` when the recommendation has no monetary impact
    /// (wealth-building suggestions, behavioral nudges).
    pub monthly_impact: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub count: u32,
    pub total_debit: String,
    pub total_credit: String,
    /// `"income"` | `"expense"` | `"transfer"` — derived classification
    /// the UI uses for coloring + grouping.
    pub kind: &'static str,
}

#[tauri::command]
pub fn dashboard_aggregate(state: State<AppState>) -> Result<DashboardData, String> {
    let (user, dek) = session(&state)?;
    let imports = list_imports_internal(&state, &user, &dek)?;
    aggregate_imports(&state, &user, &dek, &imports)
}

fn aggregate_imports(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    imports: &[FileMeta],
) -> Result<DashboardData, String> {
    let mut all_rows: Vec<RawTransaction> = Vec::new();

    for m in imports {
        let txn_rel = txn_path(&m.import_id);
        if !state
            .storage
            .exists(user, &txn_rel)
            .map_err(|e| e.to_string())?
        {
            continue;
        }
        let sealed = state
            .storage
            .read(user, &txn_rel)
            .map_err(|e| e.to_string())?;
        if sealed.len() > MAX_SEALED_TXN_BYTES {
            return Err(format!(
                "import {} has an unexpectedly large transactions file ({} bytes); refusing to load",
                m.import_id,
                sealed.len()
            ));
        }
        let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
        let doc: VersionedJson<Vec<RawTransaction>> =
            serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
        if doc.schema_version != RAW_TXN_SCHEMA {
            continue;
        }
        all_rows.extend(doc.data);
    }

    Ok(summarise_rows(imports.len() as u32, &all_rows))
}

fn summarise_rows(import_count: u32, rows: &[RawTransaction]) -> DashboardData {
    let mut income = Decimal::ZERO;
    let mut expense = Decimal::ZERO;
    let mut transfer = Decimal::ZERO;
    let mut transfer_count: u32 = 0;
    let mut earliest: Option<String> = None;
    let mut latest: Option<String> = None;

    #[derive(Default)]
    struct CatAcc {
        count: u32,
        debit: Decimal,
        credit: Decimal,
    }
    let mut by_cat: HashMap<String, CatAcc> = HashMap::new();

    #[derive(Default)]
    struct MonthAcc {
        income: Decimal,
        expense: Decimal,
        investment: Decimal,
    }
    let mut by_month: HashMap<String, MonthAcc> = HashMap::new();
    let mut essential_spend = Decimal::ZERO;

    for r in rows {
        // Track period bounds.
        if earliest.as_deref().is_none_or(|e| r.txn_date.as_str() < e) {
            earliest = Some(r.txn_date.clone());
        }
        if latest.as_deref().is_none_or(|e| r.txn_date.as_str() > e) {
            latest = Some(r.txn_date.clone());
        }

        let cat = r.category.as_deref().unwrap_or("Uncategorized");
        let entry = by_cat.entry(cat.to_string()).or_default();
        entry.count += 1;
        let debit = r
            .debit
            .as_ref()
            .map(|a| a.as_decimal())
            .unwrap_or(Decimal::ZERO);
        let credit = r
            .credit
            .as_ref()
            .map(|a| a.as_decimal())
            .unwrap_or(Decimal::ZERO);
        entry.debit += debit;
        entry.credit += credit;

        // Monthly bucketing — only consider rows with a parser-validated
        // ISO date. Anything malformed gets counted in headline totals but
        // skipped from the trend, so a parser regression can't surface a
        // synthetic `"0000-00"` bucket in the UI.
        let month_key = parse_iso_month(&r.txn_date).map(str::to_string);

        match classify_category(cat) {
            CategoryKind::Income => {
                income += credit;
                if let Some(k) = month_key.as_deref() {
                    by_month.entry(k.to_string()).or_default().income += credit;
                }
                // A debit on an income category (e.g. a salary recovery) is
                // an exotic-enough case that we'd rather not silently fold
                // it into expense — the user can recategorize.
            }
            CategoryKind::Expense => {
                // Direction-by-direction: a debit on an expense category is
                // a real outflow; a credit on the same category (refund,
                // return) is income. Previously we netted them per-row,
                // which let a single category's refunds pull the whole
                // month's expense negative — surfacing as a "−₹X" out-bar
                // in the UI and inflating Net.
                if debit > Decimal::ZERO {
                    expense += debit;
                    if let Some(k) = month_key.as_deref() {
                        by_month.entry(k.to_string()).or_default().expense += debit;
                    }
                    if is_essential(cat) {
                        essential_spend += debit;
                    }
                }
                if credit > Decimal::ZERO {
                    income += credit;
                    if let Some(k) = month_key.as_deref() {
                        by_month.entry(k.to_string()).or_default().income += credit;
                    }
                }
            }
            CategoryKind::Transfer => {
                transfer += debit + credit;
                if debit > Decimal::ZERO || credit > Decimal::ZERO {
                    transfer_count += 1;
                }
            }
            CategoryKind::Investment => {
                // Same split as Expense: a debit is the wealth-building
                // outflow (SIP, PPF), a credit is a payout / redemption
                // → income for the cash-flow tile.
                if debit > Decimal::ZERO {
                    if let Some(k) = month_key.as_deref() {
                        by_month.entry(k.to_string()).or_default().investment += debit;
                    }
                }
                if credit > Decimal::ZERO {
                    income += credit;
                    if let Some(k) = month_key.as_deref() {
                        by_month.entry(k.to_string()).or_default().income += credit;
                    }
                }
            }
        }
    }

    let net = income - expense;
    // Sort while we still have Decimals — round-tripping through formatted
    // strings is both slower and silently lossy on parse failure.
    let mut sorted: Vec<(String, CatAcc, CategoryKind)> = by_cat
        .into_iter()
        .map(|(category, acc)| {
            let kind = classify_category(&category);
            (category, acc, kind)
        })
        .collect();
    sorted.sort_by(|a, b| {
        b.1.debit
            .cmp(&a.1.debit)
            .then_with(|| b.1.credit.cmp(&a.1.credit))
    });
    // Snapshot the sorted accumulator for recommendation building before we
    // consume it for category_totals. Cheap clone — at most ~31 entries.
    let sorted_for_recs: Vec<(String, Decimal, u32, CategoryKind)> = sorted
        .iter()
        .map(|(category, acc, kind)| (category.clone(), acc.debit, acc.count, *kind))
        .collect();
    let category_totals: Vec<CategoryTotal> = sorted
        .into_iter()
        .map(|(category, acc, kind)| CategoryTotal {
            category,
            count: acc.count,
            total_debit: format!("{:.2}", acc.debit),
            total_credit: format!("{:.2}", acc.credit),
            kind: kind.as_str(),
        })
        .collect();

    // Count months where the user made an investment outflow before
    // consuming `by_month` for monthly_trend — drives the
    // investment-consistency health driver.
    let months_with_investment = by_month
        .values()
        .filter(|m| m.investment > Decimal::ZERO)
        .count() as u32;
    let total_months = by_month.len() as u32;

    let mut monthly_trend: Vec<MonthlyBucket> = by_month
        .into_iter()
        .map(|(month, acc)| MonthlyBucket {
            month,
            income: format!("{:.2}", acc.income),
            expense: format!("{:.2}", acc.expense),
            net: format!("{:.2}", acc.income - acc.expense),
        })
        .collect();
    monthly_trend.sort_by(|a, b| a.month.cmp(&b.month));
    let health_score = compute_health_score(
        income,
        expense,
        essential_spend,
        months_with_investment,
        total_months,
    );
    // Recommendations consume Decimals directly — never the formatted
    // category_totals — so trim math stays exact (regression of F-INF-3).
    let months_in_period = monthly_trend.len().max(1) as u32;
    let recommendations = build_recommendations(
        &sorted_for_recs,
        income,
        expense,
        &health_score,
        months_in_period,
    );

    DashboardData {
        import_count,
        period_start: earliest,
        period_end: latest,
        transaction_count: rows.len() as u32,
        total_income: format!("{:.2}", income),
        total_expense: format!("{:.2}", expense),
        net_savings: format!("{:.2}", net),
        transfer_count,
        transfer_total: format!("{:.2}", transfer),
        category_totals,
        monthly_trend,
        health_score,
        recommendations,
    }
}

/// Validate that a transaction-date string is a well-formed ISO `YYYY-MM-DD`
/// and return its `YYYY-MM` prefix as a `&str`. Returns `None` on any
/// structural mismatch — the caller skips the row from monthly bucketing
/// but still counts it in headline totals.
fn parse_iso_month(date: &str) -> Option<&str> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    if !b[0..4].iter().all(|c| c.is_ascii_digit())
        || !b[5..7].iter().all(|c| c.is_ascii_digit())
        || !b[8..10].iter().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(&date[..7])
}

/// "Essential" — recurring needs that are hard to compress without a
/// lifestyle change. Used for the essential-vs-discretionary driver of the
/// financial-health score and for tilting recommendations. Matches
/// case-insensitively and recognises common user-coined labels (e.g.
/// `"Loans"` for `"Loan EMI"`, `"Food"` for groceries-leaning home food).
fn is_essential(category: &str) -> bool {
    let n = category.trim().to_lowercase();
    matches!(
        n.as_str(),
        "rent"
            | "electricity"
            | "gas"
            | "water"
            | "mobile"
            | "internet"
            | "groceries"
            | "food"
            | "meals"
            | "maintenance"
            | "insurance"
            | "bills"
            | "loan emi"
            | "loans"
            | "emi"
            | "tax"
            | "taxes"
            | "train travel"
            | "fuel"
    )
}

/// Composite financial-health score per FinanceManager.md §11.4 P5.
///
/// Weights: 40% savings rate, 25% debt burden, 20% essential vs
/// discretionary, 15% investment consistency.
///
/// Savings rate, essential-vs-discretionary, and investment consistency are
/// driven by real data. Debt burden remains a placeholder (100) until the
/// Loan Tracker tab ships.
fn compute_health_score(
    income: Decimal,
    expense: Decimal,
    essential: Decimal,
    months_with_investment: u32,
    total_months: u32,
) -> HealthScore {
    let savings_rate_score = savings_rate_score(income, expense);
    let debt_burden_score = 100u32; // No loan data yet → assume no debt drag.
    let ess_disc_score = essential_vs_discretionary_score(expense, essential);
    let invest_consistency_score =
        investment_consistency_score(months_with_investment, total_months);

    let weights = [
        ("savingsRate", "Savings rate", savings_rate_score, 0.40f32),
        ("debtBurden", "Debt burden", debt_burden_score, 0.25f32),
        (
            "essentialDiscretionary",
            "Essential vs discretionary",
            ess_disc_score,
            0.20f32,
        ),
        (
            "investmentConsistency",
            "Investment consistency",
            invest_consistency_score,
            0.15f32,
        ),
    ];
    let composite_f = weights
        .iter()
        .map(|(_, _, s, w)| (*s as f32) * w)
        .sum::<f32>();
    let composite = composite_f.round().clamp(0.0, 100.0) as u32;

    let drivers = weights
        .iter()
        .map(|(key, label, score, weight)| HealthDriver {
            key,
            label,
            score: *score,
            weight: *weight,
            detail: driver_detail(key, *score),
        })
        .collect();

    HealthScore { composite, drivers }
}

fn driver_detail(key: &str, score: u32) -> String {
    match key {
        "savingsRate" => match score {
            0 => "No income recorded yet — upload a salary statement.".into(),
            1..=24 => "Spending more than 75% of income. Tighten the top categories.".into(),
            25..=49 => "Saving some, but well below the 30% sweet spot.".into(),
            50..=74 => "Healthy savings rate.".into(),
            _ => "Strong saver — over 50% of income kept.".into(),
        },
        "debtBurden" => "No loan data yet — fill in the Loan Tracker for a real score.".into(),
        "essentialDiscretionary" => match score {
            0..=39 => {
                "Discretionary spend dominates. Look at restaurants / shopping / cabs.".into()
            }
            40..=69 => "Balanced mix.".into(),
            _ => "Mostly essentials — little discretionary fat to cut.".into(),
        },
        "investmentConsistency" => match score {
            0 => "No investment outflows detected. Categories like 'SIP', 'Mutual Fund', 'Investments', 'PPF', 'NPS' count toward this.".into(),
            1..=33 => "Investing in fewer than 1 in 3 months — consider a fixed monthly SIP.".into(),
            34..=66 => "Investing in some months but not all. A standing SIP keeps this consistent.".into(),
            67..=99 => "Investing most months — close to a perfect SIP cadence.".into(),
            _ => "Investing every month tracked. Strong consistency.".into(),
        },
        _ => String::new(),
    }
}

fn savings_rate_score(income: Decimal, expense: Decimal) -> u32 {
    if income <= Decimal::ZERO {
        return 0;
    }
    let net = income - expense;
    if net <= Decimal::ZERO {
        return 0;
    }
    let ratio_f = decimal_to_f32(net) / decimal_to_f32(income);
    // 50%+ savings → full marks; linearly scale below.
    let s = (ratio_f / 0.5).clamp(0.0, 1.0) * 100.0;
    s.round() as u32
}

/// Fraction of months in the period where the user made at least one
/// investment outflow. Zero data → neutral placeholder so first-time users
/// don't see a punitive 0.
fn investment_consistency_score(months_with_investment: u32, total_months: u32) -> u32 {
    if total_months == 0 {
        return 50;
    }
    let ratio = (months_with_investment as f32) / (total_months as f32);
    (ratio * 100.0).round().clamp(0.0, 100.0) as u32
}

fn essential_vs_discretionary_score(expense: Decimal, essential: Decimal) -> u32 {
    if expense <= Decimal::ZERO {
        return 100; // No spending at all → can't be "bleeding".
    }
    let ratio_f = decimal_to_f32(essential) / decimal_to_f32(expense);
    // 50% essential → balanced (mid-score); approaching 100% essential
    // means you have little discretionary fat → high score.
    let s = ratio_f.clamp(0.0, 1.0) * 100.0;
    s.round() as u32
}

fn decimal_to_f32(d: Decimal) -> f32 {
    use std::str::FromStr;
    f32::from_str(&d.to_string()).unwrap_or(0.0)
}

/// Surface up to four actionable recommendations. Order matters — we lead
/// with the highest-impact expense cut and finish with wealth-building
/// nudges. Phase 2 will incorporate loan + investment data here.
///
/// `sorted` carries the per-category (debit, count, kind) accumulator in
/// debit-descending order so the first matching discretionary category is
/// the highest-impact cut candidate. `months_in_period` is used to
/// normalize the suggested cut from a period-total to a monthly figure.
fn build_recommendations(
    sorted: &[(String, Decimal, u32, CategoryKind)],
    income: Decimal,
    expense: Decimal,
    health: &HealthScore,
    months_in_period: u32,
) -> Vec<Recommendation> {
    let mut out: Vec<Recommendation> = Vec::new();
    let mut expense_cut_pushed = false;

    // Top discretionary expense category → trim-by-20% suggestion, expressed
    // per-month so the displayed savings figure matches the user's mental
    // model of recurring spend.
    let top_discretionary = sorted.iter().find(|(category, debit, _count, kind)| {
        *kind == CategoryKind::Expense && !is_essential(category) && *debit > Decimal::ZERO
    });
    if let Some((category, period_debit, count, _)) = top_discretionary {
        let months = Decimal::from(months_in_period.max(1));
        let monthly_avg = period_debit / months;
        let monthly_trim = monthly_avg * Decimal::new(2, 1); // × 0.2
        if monthly_trim > Decimal::new(500, 0) {
            out.push(Recommendation {
                kind: "expense-cut",
                title: format!("Cut {category} by 20%"),
                detail: format!(
                    "{category} ran at roughly ₹{monthly_avg:.2}/month across {count} transactions over {months_in_period} month(s). A 20% reduction would free up about ₹{monthly_trim:.2} every month.",
                ),
                monthly_impact: Some(format!("{monthly_trim:.2}")),
            });
            expense_cut_pushed = true;
        }
    }

    // Emergency-fund nudge when savings rate is low.
    if income > Decimal::ZERO {
        let net = income - expense;
        let rate_f = if net > Decimal::ZERO {
            decimal_to_f32(net) / decimal_to_f32(income)
        } else {
            0.0
        };
        if rate_f < 0.10 {
            out.push(Recommendation {
                kind: "wealth-building",
                title: "Build an emergency fund first".into(),
                detail: "Savings rate is under 10%. Target a buffer worth 3 months of expenses in a liquid account before optimizing anything else.".into(),
                monthly_impact: None,
            });
        } else if rate_f < 0.30 {
            out.push(Recommendation {
                kind: "wealth-building",
                title: "Push savings rate toward 30%".into(),
                detail: "Solid baseline. Each additional rupee saved compounds — consider a fixed SIP step-up so saving happens before spending.".into(),
                monthly_impact: None,
            });
        }
    }

    // Discretionary-heavy nudge — skip if the expense-cut already named the
    // specific category to trim. Otherwise it reads as redundant advice.
    let ess_score = health
        .drivers
        .iter()
        .find(|d| d.key == "essentialDiscretionary")
        .map(|d| d.score)
        .unwrap_or(50);
    if ess_score < 40 && !expense_cut_pushed {
        out.push(Recommendation {
            kind: "behavioral",
            title: "Discretionary spend is large vs essentials".into(),
            detail: "More than 60% of expenses are non-essential. Review the top discretionary categories in 'Where the money went' and pick one to reduce next month.".into(),
            monthly_impact: None,
        });
    }

    // Closing investment nudge — only fires when no investment outflow has
    // been detected at all. Once the user starts SIPs (or labels a row as
    // SIP / Mutual Fund / Investments), the consistency driver carries the
    // signal and this generic nudge would be redundant.
    let has_investments = sorted
        .iter()
        .any(|(_, debit, _, kind)| *kind == CategoryKind::Investment && *debit > Decimal::ZERO);
    if !has_investments {
        out.push(Recommendation {
            kind: "wealth-building",
            title: "Start tracking investments".into(),
            detail: "No investment outflows seen yet. Categorize SIPs, mutual-fund purchases, PPF/NPS contributions, or fixed deposits under labels like 'SIP', 'Mutual Fund', 'Investments', 'PPF', or 'NPS' to feed the investment-consistency driver of the health score.".into(),
            monthly_impact: None,
        });
    }

    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CategoryKind {
    Income,
    Expense,
    Transfer,
    /// Wealth-building outflow — SIPs, mutual funds, fixed deposits, etc.
    /// Cash leaves the primary account but goes into an asset, so it does
    /// not count toward "money leakage" (expense) and it drives the
    /// investment-consistency driver of the health score.
    Investment,
}

impl CategoryKind {
    fn as_str(self) -> &'static str {
        match self {
            CategoryKind::Income => "income",
            CategoryKind::Expense => "expense",
            CategoryKind::Transfer => "transfer",
            CategoryKind::Investment => "investment",
        }
    }
}

/// Classify a category label into income / expense / transfer / investment.
///
/// Comparison is case-insensitive and synonym-aware so user-created labels
/// like `"SIP"`, `"Loans"`, or `"Food"` reach the right kind without
/// forcing the user to discover the canonical name. Anything unrecognized
/// falls through to expense (the conservative direction for "leakage").
fn classify_category(category: &str) -> CategoryKind {
    let n = category.trim().to_lowercase();
    if matches!(
        n.as_str(),
        "salary" | "dividend" | "interest" | "refund" | "bonus" | "cashback"
    ) {
        return CategoryKind::Income;
    }
    // P3: own-account moves don't change net worth, so they're neither.
    if matches!(
        n.as_str(),
        "credit card payment" | "cc payment" | "bank transfer"
    ) {
        return CategoryKind::Transfer;
    }
    if matches!(
        n.as_str(),
        "investments"
            | "investment"
            | "sip"
            | "sips"
            | "mutual fund"
            | "mutual funds"
            | "mf"
            | "elss"
            | "ppf"
            | "nps"
            | "equity"
            | "stocks"
            | "fixed deposit"
            | "fd"
            | "recurring deposit"
            | "rd"
    ) {
        return CategoryKind::Investment;
    }
    CategoryKind::Expense
}

fn txn_path(import_id: &str) -> String {
    let (year, month) = parse_year_month(import_id).unwrap_or(("0000", "00"));
    format!("source/uploads/{year}/{month}/{import_id}/raw-transactions.json")
}

fn parse_year_month(import_id: &str) -> Option<(&str, &str)> {
    let after = import_id.strip_prefix("imp-")?;
    let year = after.get(0..4)?;
    let month = after.get(5..7)?;
    Some((year, month))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::Amount;
    use fm_parser::ParserBackend;

    fn row(
        date: &str,
        description: &str,
        category: &str,
        debit: Option<&str>,
        credit: Option<&str>,
    ) -> RawTransaction {
        RawTransaction {
            import_id: "imp-001".into(),
            source_file: "fixture.pdf".into(),
            source_sha256: "deadbeef".into(),
            source_page: 1,
            row_number: 1,
            parser_version: "test@0.0.0".into(),
            parser_backend: ParserBackend::Pdfium,
            txn_date: date.into(),
            description: description.into(),
            debit: debit.map(|s| Amount::parse_inr(s).unwrap().amount),
            credit: credit.map(|s| Amount::parse_inr(s).unwrap().amount),
            balance: None,
            category: Some(category.into()),
            category_rule_id: Some("test".into()),
        }
    }

    #[test]
    fn classify_categories_correctly() {
        assert_eq!(classify_category("Salary"), CategoryKind::Income);
        assert_eq!(classify_category("Dividend"), CategoryKind::Income);
        assert_eq!(
            classify_category("Credit Card Payment"),
            CategoryKind::Transfer
        );
        assert_eq!(classify_category("Bank Transfer"), CategoryKind::Transfer);
        // P4: ATM / Cash counts as expense.
        assert_eq!(classify_category("ATM / Cash"), CategoryKind::Expense);
        assert_eq!(classify_category("Groceries"), CategoryKind::Expense);
        assert_eq!(classify_category("Uncategorized"), CategoryKind::Expense);
    }

    #[test]
    fn classify_recognises_investment_synonyms() {
        assert_eq!(classify_category("SIP"), CategoryKind::Investment);
        assert_eq!(classify_category("sip"), CategoryKind::Investment);
        assert_eq!(classify_category("Mutual Fund"), CategoryKind::Investment);
        assert_eq!(classify_category("PPF"), CategoryKind::Investment);
        assert_eq!(classify_category("ELSS"), CategoryKind::Investment);
        assert_eq!(classify_category("Investments"), CategoryKind::Investment);
        assert_eq!(classify_category("Equity"), CategoryKind::Investment);
        assert_eq!(classify_category("Fixed Deposit"), CategoryKind::Investment);
        assert_eq!(classify_category("FD"), CategoryKind::Investment);
    }

    #[test]
    fn is_essential_recognises_user_synonyms() {
        assert!(is_essential("Rent"));
        assert!(is_essential("rent"));
        assert!(is_essential("Loans"));
        assert!(is_essential("Loan EMI"));
        assert!(is_essential("EMI"));
        assert!(is_essential("Food"));
        assert!(is_essential("Groceries"));
        assert!(!is_essential("Restaurants"));
        assert!(!is_essential("SIP"));
    }

    #[test]
    fn investments_are_excluded_from_expense_and_drive_consistency() {
        let rows = vec![
            row("2026-03-01", "S", "Salary", None, Some("100000.00")),
            row("2026-03-15", "SIP", "SIP", Some("10000.00"), None),
            row("2026-04-01", "S", "Salary", None, Some("100000.00")),
            row("2026-04-15", "SIP", "SIP", Some("10000.00"), None),
            row("2026-05-01", "S", "Salary", None, Some("100000.00")),
            row("2026-05-15", "SIP", "Mutual Fund", Some("10000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_expense, "0.00");
        let inv = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "investmentConsistency")
            .unwrap();
        assert_eq!(inv.score, 100);
    }

    #[test]
    fn investment_consistency_falls_when_user_skips_months() {
        let rows = vec![
            row("2026-02-01", "S", "Salary", None, Some("100000.00")),
            row("2026-02-15", "X", "Groceries", Some("5000.00"), None),
            row("2026-03-01", "S", "Salary", None, Some("100000.00")),
            row("2026-03-15", "SIP", "SIP", Some("10000.00"), None),
            row("2026-04-01", "S", "Salary", None, Some("100000.00")),
            row("2026-04-15", "X", "Groceries", Some("5000.00"), None),
            row("2026-05-01", "S", "Salary", None, Some("100000.00")),
            row("2026-05-15", "SIP", "SIP", Some("10000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        let inv = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "investmentConsistency")
            .unwrap();
        assert_eq!(inv.score, 50);
    }

    #[test]
    fn transfers_are_excluded_from_income_and_expense() {
        // ₹50,000 salary in, ₹20,000 CC payment out, ₹3,000 groceries out.
        // Income should be 50,000. Expense should be 3,000. Transfer 20,000.
        // If transfers leaked into expense, net would be 27,000 (wrong).
        let rows = vec![
            row("2026-04-01", "SALARY CR", "Salary", None, Some("50000.00")),
            row(
                "2026-04-15",
                "BPPY CC PAYMENT",
                "Credit Card Payment",
                Some("20000.00"),
                None,
            ),
            row(
                "2026-04-20",
                "BIGBASKET",
                "Groceries",
                Some("3000.00"),
                None,
            ),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_income, "50000.00");
        assert_eq!(d.total_expense, "3000.00");
        assert_eq!(d.net_savings, "47000.00");
        assert_eq!(d.transfer_total, "20000.00");
        assert_eq!(d.transfer_count, 1);
    }

    #[test]
    fn cash_withdrawal_counted_as_expense() {
        let rows = vec![row(
            "2026-04-10",
            "ATM WDL 1234",
            "ATM / Cash",
            Some("5000.00"),
            None,
        )];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_expense, "5000.00");
        assert_eq!(d.transfer_total, "0.00");
    }

    #[test]
    fn credit_on_expense_category_lands_in_income_not_negative_expense() {
        // Spending ₹1,000 at Amazon, getting ₹400 back on a return, both
        // ending up under "Online Shopping". Per-row cash-flow direction
        // wins: the ₹1,000 debit is real expense and the ₹400 credit is
        // refund-income. Net savings is the same (₹400 − ₹1000 = -₹600)
        // but neither tile shows a negative number.
        let rows = vec![
            row(
                "2026-04-05",
                "AMAZON",
                "Online Shopping",
                Some("1000.00"),
                None,
            ),
            row(
                "2026-04-08",
                "AMAZON REFUND",
                "Online Shopping",
                None,
                Some("400.00"),
            ),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_expense, "1000.00");
        assert_eq!(d.total_income, "400.00");
        assert_eq!(d.net_savings, "-600.00");
        // Monthly bucket must never carry a negative expense.
        assert_eq!(d.monthly_trend[0].expense, "1000.00");
        assert_eq!(d.monthly_trend[0].income, "400.00");
    }

    #[test]
    fn period_covers_earliest_to_latest_date() {
        let rows = vec![
            row("2026-04-15", "X", "Groceries", Some("100.00"), None),
            row("2026-03-02", "X", "Groceries", Some("100.00"), None),
            row("2026-05-20", "X", "Groceries", Some("100.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.period_start.as_deref(), Some("2026-03-02"));
        assert_eq!(d.period_end.as_deref(), Some("2026-05-20"));
    }

    #[test]
    fn salary_debit_is_ignored_not_silently_folded_into_expense() {
        // A debit on a Salary row (e.g. salary clawback) is unusual enough
        // that we'd rather not silently inflate expense. The row's amounts
        // still show up in its category total, but headline expense stays 0.
        let rows = vec![row(
            "2026-04-01",
            "SALARY REVERSAL",
            "Salary",
            Some("5000.00"),
            None,
        )];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.total_expense, "0.00");
        // Category breakdown still surfaces the row so it's not invisible.
        assert_eq!(d.category_totals[0].category, "Salary");
        assert_eq!(d.category_totals[0].total_debit, "5000.00");
    }

    #[test]
    fn empty_imports_return_zeroed_aggregate() {
        let d = summarise_rows(0, &[]);
        assert_eq!(d.import_count, 0);
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.total_expense, "0.00");
        assert_eq!(d.net_savings, "0.00");
        assert!(d.period_start.is_none());
    }

    #[test]
    fn monthly_trend_buckets_by_calendar_month() {
        let rows = vec![
            row("2026-03-15", "X", "Groceries", Some("1000.00"), None),
            row("2026-04-01", "S", "Salary", None, Some("50000.00")),
            row("2026-04-10", "X", "Restaurants", Some("3000.00"), None),
            row("2026-04-20", "X", "Restaurants", Some("2000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.monthly_trend.len(), 2);
        assert_eq!(d.monthly_trend[0].month, "2026-03");
        assert_eq!(d.monthly_trend[0].income, "0.00");
        assert_eq!(d.monthly_trend[0].expense, "1000.00");
        assert_eq!(d.monthly_trend[1].month, "2026-04");
        assert_eq!(d.monthly_trend[1].income, "50000.00");
        assert_eq!(d.monthly_trend[1].expense, "5000.00");
        assert_eq!(d.monthly_trend[1].net, "45000.00");
    }

    #[test]
    fn health_score_high_when_savings_rate_strong() {
        // ₹100k income, ₹30k expense (70% saved, all essentials), plus a SIP
        // in the only tracked month → strong composite across all four drivers.
        let rows = vec![
            row("2026-04-01", "S", "Salary", None, Some("100000.00")),
            row("2026-04-10", "X", "Rent", Some("20000.00"), None),
            row("2026-04-15", "X", "Groceries", Some("10000.00"), None),
            row("2026-04-20", "X", "SIP", Some("10000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        // 70% rate clamped against the 50%-cap → savings driver = 100.
        let sr = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "savingsRate")
            .unwrap();
        assert_eq!(sr.score, 100);
        // All expenses essential → essential-vs-discretionary = 100.
        let ed = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "essentialDiscretionary")
            .unwrap();
        assert_eq!(ed.score, 100);
        // 1 month with investment / 1 total month → 100.
        let inv = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "investmentConsistency")
            .unwrap();
        assert_eq!(inv.score, 100);
        // Composite weighted: 100*0.4 + 100*0.25 + 100*0.20 + 100*0.15 = 100.
        assert_eq!(d.health_score.composite, 100);
    }

    #[test]
    fn health_score_low_when_savings_negative() {
        let rows = vec![
            row("2026-04-01", "S", "Salary", None, Some("20000.00")),
            row("2026-04-10", "X", "Restaurants", Some("30000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        let sr = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "savingsRate")
            .unwrap();
        assert_eq!(sr.score, 0);
    }

    #[test]
    fn recommendations_lead_with_top_discretionary_cut() {
        let rows = vec![
            row("2026-04-01", "S", "Salary", None, Some("100000.00")),
            row("2026-04-10", "X", "Restaurants", Some("15000.00"), None),
            row("2026-04-20", "X", "Rent", Some("20000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        // Restaurants is bigger discretionary → first recommendation.
        assert!(d.recommendations[0].title.contains("Restaurants"));
        assert_eq!(d.recommendations[0].kind, "expense-cut");
        // Single month: 20% of ₹15,000 monthly = ₹3,000.
        assert_eq!(
            d.recommendations[0].monthly_impact.as_deref(),
            Some("3000.00")
        );
    }

    #[test]
    fn expense_cut_impact_is_monthly_not_period_total() {
        // 3 months of restaurants at ₹15k each → period total ₹45k, but the
        // recommendation should surface the *monthly* trim figure (₹3k), not
        // the period total (₹9k). Pins audit M5.
        let rows = vec![
            row("2026-03-01", "S", "Salary", None, Some("100000.00")),
            row("2026-03-10", "X", "Restaurants", Some("15000.00"), None),
            row("2026-04-01", "S", "Salary", None, Some("100000.00")),
            row("2026-04-10", "X", "Restaurants", Some("15000.00"), None),
            row("2026-05-01", "S", "Salary", None, Some("100000.00")),
            row("2026-05-10", "X", "Restaurants", Some("15000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        let cut = d
            .recommendations
            .iter()
            .find(|r| r.kind == "expense-cut")
            .expect("should have an expense-cut recommendation");
        assert_eq!(cut.monthly_impact.as_deref(), Some("3000.00"));
    }

    #[test]
    fn behavioral_nudge_skipped_when_expense_cut_already_fired() {
        // High-discretionary user gets the expense-cut, so the generic
        // "discretionary is too high" nudge should NOT also fire — they
        // would be saying the same thing. Pins audit M3.
        let rows = vec![
            row("2026-04-01", "S", "Salary", None, Some("50000.00")),
            row("2026-04-10", "X", "Restaurants", Some("30000.00"), None),
            row("2026-04-15", "X", "Online Shopping", Some("10000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        let has_cut = d.recommendations.iter().any(|r| r.kind == "expense-cut");
        let has_behavioral = d.recommendations.iter().any(|r| r.kind == "behavioral");
        assert!(has_cut);
        assert!(
            !has_behavioral,
            "behavioral nudge fired alongside expense-cut"
        );
    }

    #[test]
    fn parse_iso_month_validates_shape() {
        // Pins audit M2.
        assert_eq!(parse_iso_month("2026-04-15"), Some("2026-04"));
        assert_eq!(parse_iso_month("2026-12-31"), Some("2026-12"));
        assert!(parse_iso_month("").is_none());
        assert!(parse_iso_month("2026/04/15").is_none());
        assert!(parse_iso_month("not-a-date").is_none());
        assert!(parse_iso_month("2026-4-15").is_none()); // missing zero-pad
        assert!(parse_iso_month("2026-04-15T00:00:00").is_none()); // too long
    }

    #[test]
    fn monthly_trend_skips_malformed_dates() {
        // Pins audit M2 end-to-end: a malformed date row is excluded from
        // the trend, but still contributes to headline totals.
        let rows = vec![
            row("2026-04-10", "X", "Groceries", Some("1000.00"), None),
            row("bad-date", "X", "Groceries", Some("500.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_expense, "1500.00");
        assert_eq!(d.monthly_trend.len(), 1);
        assert_eq!(d.monthly_trend[0].month, "2026-04");
        assert_eq!(d.monthly_trend[0].expense, "1000.00");
    }

    #[test]
    fn decimal_to_f32_handles_edge_values() {
        // Pins audit M1.
        use std::str::FromStr;
        assert_eq!(decimal_to_f32(Decimal::ZERO), 0.0);
        assert!((decimal_to_f32(Decimal::new(15000, 2)) - 150.0).abs() < 0.001);
        let big = Decimal::from_str("999999.99").unwrap();
        assert!((decimal_to_f32(big) - 999_999.99).abs() < 1.0);
    }

    #[test]
    fn recommendations_skip_when_no_qualifying_categories() {
        // Income only, no expenses → no expense-cut recommendation but
        // savings-rate nudges still don't fire (rate is positive infinite-ish).
        let rows = vec![row("2026-04-01", "S", "Salary", None, Some("50000.00"))];
        let d = summarise_rows(1, &rows);
        assert!(d.recommendations.iter().all(|r| r.kind != "expense-cut"));
    }

    #[test]
    fn category_totals_sorted_by_debit_descending() {
        let rows = vec![
            row("2026-04-01", "X", "Groceries", Some("3000.00"), None),
            row("2026-04-02", "X", "Restaurants", Some("8000.00"), None),
            row("2026-04-03", "X", "Fuel", Some("2000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.category_totals[0].category, "Restaurants");
        assert_eq!(d.category_totals[1].category, "Groceries");
        assert_eq!(d.category_totals[2].category, "Fuel");
    }
}
