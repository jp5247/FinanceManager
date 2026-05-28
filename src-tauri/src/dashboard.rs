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

/// Sentinel month-key used for rows with a malformed date. They still count
/// toward headline totals, but the monthly-trend view filters them out so
/// the UI never surfaces a synthetic "undated" row.
const UNDATED_KEY: &str = "__undated__";

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

    /// Number of rows whose `txnDate` couldn't be parsed as `YYYY-MM-DD`.
    /// When no range filter is applied these still count toward headline
    /// totals (via the internal `__undated__` bucket). When a range filter
    /// IS applied, they're excluded from BOTH headline and trend — the UI
    /// should surface this count so the user knows the slice is partial.
    pub undated_count: u32,

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
pub fn dashboard_aggregate(
    state: State<AppState>,
    from_month: Option<String>,
    to_month: Option<String>,
) -> Result<DashboardData, String> {
    let (user, dek) = session(&state)?;
    let imports = list_imports_internal(&state, &user, &dek)?;
    // Best-effort: if the loans file is missing or malformed, debt burden
    // falls back to the placeholder. Don't fail the dashboard over it.
    let total_monthly_emi = crate::loans::total_monthly_emi(&state, &user, &dek).ok();
    aggregate_imports(
        &state,
        &user,
        &dek,
        &imports,
        total_monthly_emi,
        from_month.as_deref(),
        to_month.as_deref(),
    )
}

fn aggregate_imports(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    imports: &[FileMeta],
    total_monthly_emi: Option<Decimal>,
    from_month: Option<&str>,
    to_month: Option<&str>,
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

    // Count rows whose date doesn't parse at all — surface separately so
    // the UI can warn when a range filter excludes them.
    let undated_count = all_rows
        .iter()
        .filter(|r| parse_iso_month(&r.txn_date).is_none())
        .count() as u32;

    // Apply month-range filter if requested. Filters happen after the
    // full-load so import_count stays honest about the underlying data.
    let filtered: Vec<RawTransaction> = if from_month.is_some() || to_month.is_some() {
        all_rows
            .into_iter()
            .filter(|r| row_in_range(&r.txn_date, from_month, to_month))
            .collect()
    } else {
        all_rows
    };

    let mut out = summarise_rows(imports.len() as u32, &filtered, total_monthly_emi);
    out.undated_count = undated_count;
    Ok(out)
}

fn row_in_range(txn_date: &str, from: Option<&str>, to: Option<&str>) -> bool {
    let month = match parse_iso_month(txn_date) {
        Some(m) => m,
        // Undated rows excluded from any explicit filter (they have no month).
        None => return false,
    };
    if let Some(f) = from {
        if month < f {
            return false;
        }
    }
    if let Some(t) = to {
        if month > t {
            return false;
        }
    }
    true
}

pub(crate) fn summarise_rows(
    import_count: u32,
    rows: &[RawTransaction],
    total_monthly_emi: Option<Decimal>,
) -> DashboardData {
    let mut earliest: Option<String> = None;
    let mut latest: Option<String> = None;
    let mut transfer = Decimal::ZERO;
    let mut transfer_count: u32 = 0;

    #[derive(Default)]
    struct CatAcc {
        count: u32,
        debit: Decimal,
        credit: Decimal,
    }
    let mut by_cat: HashMap<String, CatAcc> = HashMap::new();
    // Per (month, category) so each month's expense can also use the
    // "net debits against credits, floor at zero" rule. Avoids letting a
    // single month show a negative OUT bar when refunds dominate.
    let mut by_month_cat: HashMap<(String, String), (Decimal, Decimal)> = HashMap::new();

    for r in rows {
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

        if classify_category(cat) == CategoryKind::Transfer {
            transfer += debit + credit;
            if debit > Decimal::ZERO || credit > Decimal::ZERO {
                transfer_count += 1;
            }
        }

        // Per-(month, category) bucketing. Rows with malformed dates go
        // into a sentinel `UNDATED_KEY` bucket so they still count toward
        // headline totals; the monthly trend filters that bucket out.
        let month_key = parse_iso_month(&r.txn_date).unwrap_or(UNDATED_KEY);
        let mc = by_month_cat
            .entry((month_key.to_string(), cat.to_string()))
            .or_default();
        mc.0 += debit;
        mc.1 += credit;
    }

    // Roll up the (month, category) accumulator into per-month buckets
    // using the accounting rule the user expects: refunds reduce that
    // category's expense (never below zero), never become income. Same
    // for investment redemptions.
    #[derive(Default)]
    struct MonthAcc {
        income: Decimal,
        expense: Decimal,
        investment: Decimal,
        essential: Decimal,
    }
    let mut by_month: HashMap<String, MonthAcc> = HashMap::new();
    for ((month, cat), (mc_debit, mc_credit)) in &by_month_cat {
        let m = by_month.entry(month.clone()).or_default();
        match classify_category(cat) {
            CategoryKind::Income => {
                m.income += *mc_credit;
            }
            CategoryKind::Expense => {
                let net = (*mc_debit - *mc_credit).max(Decimal::ZERO);
                m.expense += net;
                if is_essential(cat) {
                    m.essential += net;
                }
            }
            CategoryKind::Investment => {
                m.investment += (*mc_debit - *mc_credit).max(Decimal::ZERO);
            }
            CategoryKind::Transfer => {}
        }
    }

    // Headline numbers are the sum of per-month buckets — so the trend
    // bars always add up to the totals shown in the overview tiles. If we
    // clamped per-category-across-all-months for the headline AND
    // per-(month, category) for the trend, the two could disagree by
    // refunds that landed in a different month from the original
    // purchase. Per-month aggregation is the user-honest cash-flow view.
    let income: Decimal = by_month.values().map(|m| m.income).sum();
    let expense: Decimal = by_month.values().map(|m| m.expense).sum();
    let essential_spend: Decimal = by_month.values().map(|m| m.essential).sum();

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

    // Count months where the user made an investment outflow — drives
    // the investment-consistency health driver. Exclude the sentinel
    // `UNDATED_KEY` bucket so its rows don't perturb the calendar count.
    let months_with_investment = by_month
        .iter()
        .filter(|(k, m)| k.as_str() != UNDATED_KEY && m.investment > Decimal::ZERO)
        .count() as u32;
    let total_months = by_month
        .keys()
        .filter(|k| k.as_str() != UNDATED_KEY)
        .count() as u32;

    let mut monthly_trend: Vec<MonthlyBucket> = by_month
        .into_iter()
        .filter(|(k, _)| k.as_str() != UNDATED_KEY)
        .map(|(month, acc)| MonthlyBucket {
            month,
            income: format!("{:.2}", acc.income),
            expense: format!("{:.2}", acc.expense),
            net: format!("{:.2}", acc.income - acc.expense),
        })
        .collect();
    monthly_trend.sort_by(|a, b| a.month.cmp(&b.month));
    let months_for_income = total_months.max(1);
    let monthly_income = income / Decimal::from(months_for_income);
    let health_score = compute_health_score(
        income,
        expense,
        essential_spend,
        months_with_investment,
        total_months,
        total_monthly_emi,
        monthly_income,
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
        undated_count: 0,
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
        // New canonical names (user's category brief)
        "electricity bill"
            | "gas bill"
            | "mobile/internet bill"
            | "laundary bill"
            | "home loan emi"
            | "car loan emi"
            | "cc emi"
            | "fuel expenses"
            | "vehicle repairs/maintenance"
            | "medical expenses"
            | "groceries"
            | "transportation"
            // Legacy names kept for back-compat with existing data
            | "rent"
            | "electricity"
            | "gas"
            | "water"
            | "mobile"
            | "internet"
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
/// discretionary, 15% investment consistency. All four drivers use real
/// data once the Loan Tracker has at least one loan; debt burden falls
/// back to a neutral placeholder when no loan data is present.
fn compute_health_score(
    income: Decimal,
    expense: Decimal,
    essential: Decimal,
    months_with_investment: u32,
    total_months: u32,
    total_monthly_emi: Option<Decimal>,
    monthly_income: Decimal,
) -> HealthScore {
    let savings_rate_score = savings_rate_score(income, expense);
    let savings_rate_explainer = savings_rate_detail(income, expense, savings_rate_score);
    let (debt_burden_score, debt_burden_explainer) =
        debt_burden_driver(total_monthly_emi, monthly_income);
    let ess_disc_score = essential_vs_discretionary_score(expense, essential);
    let invest_consistency_score =
        investment_consistency_score(months_with_investment, total_months);
    let invest_consistency_explainer = investment_consistency_detail(
        months_with_investment,
        total_months,
        invest_consistency_score,
    );

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
            detail: if *key == "savingsRate" {
                savings_rate_explainer.clone()
            } else if *key == "debtBurden" {
                debt_burden_explainer.clone()
            } else if *key == "investmentConsistency" {
                invest_consistency_explainer.clone()
            } else {
                driver_detail(key, *score)
            },
        })
        .collect();

    HealthScore { composite, drivers }
}

/// Savings-rate detail needs the actual `income` / `expense` values to
/// distinguish "no income" from "expense exceeds income" — both produce a
/// raw score of 0 but the user-facing message is very different.
fn savings_rate_detail(income: Decimal, expense: Decimal, score: u32) -> String {
    if income <= Decimal::ZERO {
        return "No income recorded yet — categorize a row as Salary, Dividend, Interest, or Refund.".into();
    }
    if expense > income {
        return "Spending exceeds income for this period. The expense tile shows the gap.".into();
    }
    match score {
        0 => "Spending exactly matches income — no net savings.".into(),
        1..=24 => "Saving under 12% of income. Trim the top categories to push this up.".into(),
        25..=49 => "Saving some, but below the 30% sweet spot.".into(),
        50..=74 => "Healthy savings rate.".into(),
        _ => "Strong saver — over 50% of income kept.".into(),
    }
}

fn driver_detail(key: &str, score: u32) -> String {
    match key {
        "savingsRate" => match score {
            // Kept for backwards compatibility with other call sites; the
            // real `savingsRate` text now comes from `savings_rate_detail`.
            0 => "No income recorded yet.".into(),
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

/// Debt-burden driver derived from the Loan Tracker.
///
/// `debt_burden_ratio = total_monthly_emi / monthly_income` (capped at 1.0).
/// Score is `(1 - ratio / 0.4) * 100` clamped to `[0, 100]` — so 0% EMI
/// → 100, 20% EMI → 50, 40%+ EMI → 0. Indian retail-banking rule of
/// thumb is "EMI under 40% of take-home", which anchors the 0-score.
///
/// Falls back to a neutral placeholder (100) when either side is absent.
fn debt_burden_driver(
    total_monthly_emi: Option<Decimal>,
    monthly_income: Decimal,
) -> (u32, String) {
    let emi = match total_monthly_emi {
        Some(e) if e > Decimal::ZERO => e,
        _ => {
            return (
                100,
                "No loan data yet — add loans in the Loan Tracker for a real score.".into(),
            );
        }
    };
    if monthly_income <= Decimal::ZERO {
        return (
            100,
            "No income recorded — debt burden is unknown until a salary statement is uploaded."
                .into(),
        );
    }
    let ratio_f = decimal_to_f32(emi) / decimal_to_f32(monthly_income);
    let score = ((1.0 - (ratio_f / 0.4)).clamp(0.0, 1.0) * 100.0).round() as u32;
    let pct = (ratio_f * 100.0).clamp(0.0, 999.0);
    let detail = match score {
        0 => format!(
            "EMIs are {pct:.0}% of monthly income — at or above the 40% safe-debt ceiling. Prioritize prepayment."
        ),
        1..=49 => format!(
            "EMIs are {pct:.0}% of monthly income — above the 20% comfort zone. Consider prepayment when surplus permits."
        ),
        50..=79 => format!(
            "EMIs are {pct:.0}% of monthly income — in the comfortable middle. Stay disciplined on the EMI."
        ),
        _ => format!(
            "EMIs are {pct:.0}% of monthly income — well below the 20% comfort threshold. Healthy debt level."
        ),
    };
    (score, detail)
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

/// Investment-consistency driver detail. Needs the raw month counts (not
/// just the score) to distinguish "no statements tracked yet" from
/// "tracked months with some investments" — both can produce score 50.
fn investment_consistency_detail(
    months_with_investment: u32,
    total_months: u32,
    score: u32,
) -> String {
    if total_months == 0 {
        return "Upload a bank statement to track monthly investment cadence. Asset positions in the Investments tab feed the Wealth Snapshot tile, not this driver.".into();
    }
    match score {
        0 => format!(
            "No investment outflows across {total_months} tracked month(s). Categorize SIPs / Mutual Fund / PPF / NPS rows to feed this driver."
        ),
        1..=33 => format!(
            "Investing in {months_with_investment} of {total_months} tracked month(s) — under 1 in 3. A standing SIP would steady this."
        ),
        34..=66 => format!(
            "Investing in {months_with_investment} of {total_months} tracked month(s). A standing SIP keeps it consistent."
        ),
        67..=99 => format!(
            "Investing in {months_with_investment} of {total_months} tracked month(s) — close to a perfect SIP cadence."
        ),
        _ => format!(
            "Investing every one of {total_months} tracked month(s). Strong consistency."
        ),
    }
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
        "salary" | "side hustle" | "dividend" | "interest" | "refund" | "bonus" | "cashback"
    ) {
        return CategoryKind::Income;
    }
    // P3: own-account moves don't change net worth, so they're neither.
    // "EMI conversion" covers credit-card EMI bookkeeping: the loan
    // disbursement credit + the loan principal debit booking. They net
    // to zero and should not affect income or expense — only the actual
    // monthly EMI installments (Home Loan EMI / Car loan EMI / CC EMI)
    // are real expense.
    if matches!(
        n.as_str(),
        "credit card bill"
            | "credit card payment"
            | "cc payment"
            | "bank transfer"
            | "emi conversion"
            | "loan disbursement"
            | "loan against transaction"
    ) {
        return CategoryKind::Transfer;
    }
    if matches!(
        n.as_str(),
        "sip"
            | "stock purchase"
            | "fd"
            // Legacy / convenience synonyms — kept so older data and LLM
            // outputs that drift slightly still route correctly.
            | "investments"
            | "investment"
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
        let d = summarise_rows(1, &rows, None);
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
    fn investment_consistency_detail_distinguishes_placeholder_from_tracked() {
        // No transactions at all → placeholder score 50 with a message that
        // says "upload a statement" (NOT "investing in some months").
        let d = summarise_rows(0, &[], None);
        let inv = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "investmentConsistency")
            .unwrap();
        assert_eq!(inv.score, 50);
        assert!(
            inv.detail.contains("Upload"),
            "expected upload-statement message, got: {}",
            inv.detail
        );

        // Real data: 1 of 2 months tracked also scores 50 but the detail
        // should reflect the actual ratio, NOT the placeholder.
        let rows = vec![
            row("2026-03-01", "S", "Salary", None, Some("100000.00")),
            row("2026-03-15", "SIP", "SIP", Some("10000.00"), None),
            row("2026-04-01", "S", "Salary", None, Some("100000.00")),
            row("2026-04-15", "X", "Groceries", Some("5000.00"), None),
        ];
        let d = summarise_rows(1, &rows, None);
        let inv = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "investmentConsistency")
            .unwrap();
        assert_eq!(inv.score, 50);
        assert!(
            inv.detail.contains("1 of 2"),
            "expected '1 of 2 tracked month(s)' message, got: {}",
            inv.detail
        );
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
        let d = summarise_rows(1, &rows, None);
        let inv = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "investmentConsistency")
            .unwrap();
        assert_eq!(inv.score, 50);
    }

    #[test]
    fn undated_count_is_reported_separately_from_headline_when_filtered() {
        // With NO filter applied, a bad-date row goes into the undated
        // bucket and counts toward headline expense (existing contract).
        // With a filter applied, bad-date rows are excluded from headline
        // but reported in `undated_count` so the UI can warn the user.
        // This pins audit A3.
        let rows = vec![
            row("2026-04-10", "X", "Groceries", Some("1000.00"), None),
            row("bad-date", "X", "Groceries", Some("500.00"), None),
        ];
        let d = summarise_rows(1, &rows, None);
        // Unfiltered: bad-date row is in headline via __undated__ bucket.
        assert_eq!(d.total_expense, "1500.00");
        // `undated_count` is set by `aggregate_imports`, NOT by
        // `summarise_rows`. The summarise path doesn't know about the
        // pre-filter step. So in this unit test (which exercises
        // summarise_rows directly) we just confirm the field defaults to 0.
        assert_eq!(d.undated_count, 0);
    }

    #[test]
    fn row_in_range_inclusive_on_both_ends() {
        assert!(row_in_range("2026-04-15", Some("2026-03"), Some("2026-05")));
        assert!(row_in_range("2026-03-01", Some("2026-03"), Some("2026-05")));
        assert!(row_in_range("2026-05-31", Some("2026-03"), Some("2026-05")));
        assert!(!row_in_range(
            "2026-02-15",
            Some("2026-03"),
            Some("2026-05")
        ));
        assert!(!row_in_range(
            "2026-06-01",
            Some("2026-03"),
            Some("2026-05")
        ));
        // Open-ended ranges
        assert!(row_in_range("2026-01-01", None, Some("2026-05")));
        assert!(row_in_range("2030-12-31", Some("2026-03"), None));
        // Malformed date with explicit filter → excluded.
        assert!(!row_in_range("bad-date", Some("2026-03"), None));
    }

    #[test]
    fn debt_burden_driver_scales_with_emi_share_of_income() {
        // 100k salary in one month, 10k EMIs → 10% burden → comfortable
        // (above 50, below 79).
        let rows = vec![row("2026-04-01", "S", "Salary", None, Some("100000.00"))];
        let d = summarise_rows(
            1,
            &rows,
            Some(rust_decimal::Decimal::new(10000, 0)), // ₹10,000 EMIs
        );
        let db = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "debtBurden")
            .unwrap();
        // (1 - 0.10/0.40) * 100 = 75
        assert_eq!(db.score, 75);
    }

    #[test]
    fn debt_burden_driver_zero_when_emi_is_at_or_above_40_percent() {
        let rows = vec![row("2026-04-01", "S", "Salary", None, Some("100000.00"))];
        let d = summarise_rows(
            1,
            &rows,
            Some(rust_decimal::Decimal::new(45000, 0)), // ₹45k EMIs = 45% of income
        );
        let db = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "debtBurden")
            .unwrap();
        assert_eq!(db.score, 0);
    }

    #[test]
    fn debt_burden_driver_neutral_when_no_loan_data() {
        let rows = vec![row("2026-04-01", "S", "Salary", None, Some("100000.00"))];
        let d = summarise_rows(1, &rows, None);
        let db = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "debtBurden")
            .unwrap();
        assert_eq!(db.score, 100);
        assert!(db.detail.contains("No loan data"));
    }

    #[test]
    fn split_category_nets_reimbursements_against_initial_outlay() {
        // User pays ₹3,000 for a group dinner where their actual share is
        // ₹1,000. A friend reimburses ₹2,000 a few days later. Both rows
        // are categorized "Split" — the per-category clamp gives a net
        // expense of ₹1,000, which is the user's real share.
        let rows = vec![
            row(
                "2026-04-10",
                "DINNER PAID FOR FRIENDS",
                "Split",
                Some("3000.00"),
                None,
            ),
            row(
                "2026-04-15",
                "REIMBURSEMENT FROM FRIEND",
                "Split",
                None,
                Some("2000.00"),
            ),
        ];
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.total_expense, "1000.00");
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.net_savings, "-1000.00");
    }

    #[test]
    fn split_category_fully_settled_nets_to_zero_expense() {
        // The user paid the entire bill on behalf of someone and was fully
        // reimbursed — no real personal expense, no fake income either.
        let rows = vec![
            row(
                "2026-04-10",
                "PAID FOR SOMEONE",
                "Split",
                Some("2000.00"),
                None,
            ),
            row(
                "2026-04-15",
                "FULL REIMBURSEMENT",
                "Split",
                None,
                Some("2000.00"),
            ),
        ];
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.total_expense, "0.00");
        assert_eq!(d.total_income, "0.00");
    }

    #[test]
    fn emi_conversion_rows_net_to_zero_across_income_and_expense() {
        // Real HDFC CC EMI conversion pattern: a ₹47,148 phone purchase
        // converted to EMI surfaces as three rows — the original
        // purchase debit, the loan principal being booked debit, and
        // the loan disbursement credit. The user (or curated rules)
        // categorize the bookkeeping rows as "EMI Conversion" so they
        // stay out of income / expense entirely. Only actual recurring
        // installments + processing fee + GST should hit expense.
        let rows = vec![
            // Loan principal booking + disbursement (cancel out).
            row(
                "2026-03-22",
                "EMI BONITO DESIGNS",
                "EMI Conversion",
                Some("47500.00"),
                None,
            ),
            row(
                "2026-03-25",
                "AGGREGATOR-EMI-OFFUSCREDIT",
                "EMI Conversion",
                None,
                Some("47500.00"),
            ),
            // Real costs that should remain expense.
            row(
                "2026-03-26",
                "EMI processing fee",
                "Bills",
                Some("299.00"),
                None,
            ),
            row("2026-03-26", "GST on EMI", "Tax", Some("73.08"), None),
        ];
        let d = summarise_rows(1, &rows, None);
        // EMI Conversion is Transfer kind — out of income / expense.
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.total_expense, "372.08");
        // The 47,500 debit + 47,500 credit shows up as transfer total.
        assert_eq!(d.transfer_total, "95000.00");
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.total_expense, "5000.00");
        assert_eq!(d.transfer_total, "0.00");
    }

    #[test]
    fn refund_nets_against_same_category_does_not_become_income() {
        // Spending ₹1,000 at Amazon, getting ₹400 back on a return, both
        // ending up under "Online Shopping". The refund nets against that
        // category's outflow (cost was ₹600 net) and is NOT counted as
        // income — the ₹400 was originally part of the user's income that
        // briefly left the account and came back, so counting it as new
        // income would double-count.
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
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.total_expense, "600.00");
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.net_savings, "-600.00");
        assert_eq!(d.monthly_trend[0].expense, "600.00");
        assert_eq!(d.monthly_trend[0].income, "0.00");
    }

    #[test]
    fn savings_rate_detail_distinguishes_no_income_from_overspending() {
        // No income at all (only debits) → driver detail should say so.
        let rows = vec![row("2026-04-01", "X", "Groceries", Some("1000.00"), None)];
        let d = summarise_rows(1, &rows, None);
        let sr = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "savingsRate")
            .unwrap();
        assert_eq!(sr.score, 0);
        assert!(
            sr.detail.contains("No income"),
            "expected 'No income' message, got: {}",
            sr.detail
        );

        // Real income, but expense > income → different message.
        let rows = vec![
            row("2026-04-01", "S", "Salary", None, Some("20000.00")),
            row("2026-04-15", "X", "Restaurants", Some("30000.00"), None),
        ];
        let d = summarise_rows(1, &rows, None);
        let sr = d
            .health_score
            .drivers
            .iter()
            .find(|x| x.key == "savingsRate")
            .unwrap();
        assert_eq!(sr.score, 0);
        assert!(
            sr.detail.contains("exceeds income"),
            "expected overspending message, got: {}",
            sr.detail
        );
    }

    #[test]
    fn headline_totals_equal_sum_of_monthly_buckets() {
        // Three months with mixed activity. Headline numbers must equal
        // the sum of the rendered monthly buckets so the UI is internally
        // consistent (no "trend bars don't add up to the overview tiles").
        let rows = vec![
            row("2026-03-01", "S", "Salary", None, Some("50000.00")),
            row("2026-03-10", "X", "Groceries", Some("4000.00"), None),
            row("2026-03-15", "X", "Restaurants", Some("3000.00"), None),
            row("2026-04-01", "S", "Salary", None, Some("50000.00")),
            row("2026-04-12", "X", "Groceries", Some("5000.00"), None),
            row("2026-04-20", "X", "Restaurants", Some("2500.00"), None),
            row("2026-05-01", "S", "Salary", None, Some("50000.00")),
            row("2026-05-08", "X", "Online Shopping", Some("1500.00"), None),
        ];
        let d = summarise_rows(1, &rows, None);
        let sum_income: f64 = d
            .monthly_trend
            .iter()
            .map(|m| m.income.parse::<f64>().unwrap_or(0.0))
            .sum();
        let sum_expense: f64 = d
            .monthly_trend
            .iter()
            .map(|m| m.expense.parse::<f64>().unwrap_or(0.0))
            .sum();
        assert!((sum_income - d.total_income.parse::<f64>().unwrap()).abs() < 0.01);
        assert!((sum_expense - d.total_expense.parse::<f64>().unwrap()).abs() < 0.01);
    }

    #[test]
    fn refunds_exceeding_debits_floor_expense_at_zero_never_become_income() {
        // Edge case the user hit in real data: a month where credits on
        // expense-categorized rows exceed debits (probably misclassified
        // income posing as refunds). The category nets to a non-negative
        // expense (floored at 0) and the surplus credit is silently
        // dropped from totals — counting it as income would inflate Net
        // dishonestly. User can drill in and recategorize if the rows are
        // genuinely misclassified income.
        let rows = vec![
            row("2026-04-05", "X", "Online Shopping", Some("100.00"), None),
            row("2026-04-08", "X", "Online Shopping", None, Some("5000.00")),
        ];
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.total_expense, "0.00");
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.net_savings, "0.00");
        assert_eq!(d.monthly_trend[0].expense, "0.00");
    }

    #[test]
    fn period_covers_earliest_to_latest_date() {
        let rows = vec![
            row("2026-04-15", "X", "Groceries", Some("100.00"), None),
            row("2026-03-02", "X", "Groceries", Some("100.00"), None),
            row("2026-05-20", "X", "Groceries", Some("100.00"), None),
        ];
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.total_expense, "0.00");
        // Category breakdown still surfaces the row so it's not invisible.
        assert_eq!(d.category_totals[0].category, "Salary");
        assert_eq!(d.category_totals[0].total_debit, "5000.00");
    }

    #[test]
    fn empty_imports_return_zeroed_aggregate() {
        let d = summarise_rows(0, &[], None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
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
        let d = summarise_rows(1, &rows, None);
        assert!(d.recommendations.iter().all(|r| r.kind != "expense-cut"));
    }

    #[test]
    fn category_totals_sorted_by_debit_descending() {
        let rows = vec![
            row("2026-04-01", "X", "Groceries", Some("3000.00"), None),
            row("2026-04-02", "X", "Restaurants", Some("8000.00"), None),
            row("2026-04-03", "X", "Fuel", Some("2000.00"), None),
        ];
        let d = summarise_rows(1, &rows, None);
        assert_eq!(d.category_totals[0].category, "Restaurants");
        assert_eq!(d.category_totals[1].category, "Groceries");
        assert_eq!(d.category_totals[2].category, "Fuel");
    }
}
