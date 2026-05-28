//! Investment Inputs tab — manually-entered asset positions.
//!
//! The Dashboard's headline numbers come from parsed bank statements
//! (transactions). The Investments tab is the OTHER half: a hand-curated
//! list of asset positions (mutual funds, stocks, FDs, PPF, NPS, etc.)
//! that the user updates periodically so wealth-tracking surfaces have
//! real numbers to display.
//!
//! Storage: per-profile encrypted JSON at `mappings/investments.json`.

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

const FILE_PATH: &str = "mappings/investments.json";
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentsDoc {
    #[serde(default)]
    pub assets: Vec<InvestmentAsset>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentAsset {
    pub id: String,
    /// One of the suggested asset types (see `ASSET_TYPES`) or a custom
    /// label. Stored as plain string for forward-compat.
    pub asset_type: String,
    pub asset_name: String,
    /// Decimal strings end-to-end so we never round-trip through f64.
    pub invested_amount: String,
    pub current_value: String,
    pub last_updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// What the UI sends when creating or editing an asset. `id` is empty
/// for a new asset (backend assigns one); set to an existing id for an
/// update.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertInvestmentSpec {
    #[serde(default)]
    pub id: Option<String>,
    pub asset_type: String,
    pub asset_name: String,
    pub invested_amount: String,
    pub current_value: String,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Aggregate snapshot for the Dashboard's Investment-snapshot tile.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentsSummary {
    pub asset_count: u32,
    pub total_invested: String,
    pub total_current_value: String,
    pub unrealized_gain_loss: String,
    /// Signed decimal percent, e.g. `"12.4"` or `"-3.2"`. Empty when
    /// invested == 0.
    pub return_pct: String,
    /// By-type allocation, sorted by current_value descending.
    pub allocation: Vec<AllocationSlice>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSlice {
    pub asset_type: String,
    pub current_value: String,
    /// 0..=100 percent of `total_current_value`.
    pub share_pct: String,
    pub asset_count: u32,
}

#[tauri::command]
pub fn list_investments(state: State<AppState>) -> Result<Vec<InvestmentAsset>, String> {
    let (user, dek) = session(&state)?;
    Ok(load(&state, &user, &dek)?.assets)
}

#[tauri::command]
pub fn upsert_investment(
    spec: UpsertInvestmentSpec,
    state: State<AppState>,
) -> Result<InvestmentAsset, String> {
    let (user, dek) = session(&state)?;
    let mut doc = load(&state, &user, &dek)?;

    let asset_type = spec.asset_type.trim();
    let asset_name = spec.asset_name.trim();
    if asset_type.is_empty() || asset_name.is_empty() {
        return Err("asset type and name are required".into());
    }
    let invested = parse_amount(&spec.invested_amount, "invested amount")?;
    let current = parse_amount(&spec.current_value, "current value")?;
    if invested < Decimal::ZERO || current < Decimal::ZERO {
        return Err("amounts must be non-negative".into());
    }

    let now = now_rfc3339();
    let notes = spec
        .notes
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let asset = match spec.id.as_deref().filter(|s| !s.is_empty()) {
        Some(existing_id) => {
            let target = doc
                .assets
                .iter_mut()
                .find(|a| a.id == existing_id)
                .ok_or_else(|| format!("asset {existing_id} not found"))?;
            target.asset_type = asset_type.to_string();
            target.asset_name = asset_name.to_string();
            target.invested_amount = format!("{invested:.2}");
            target.current_value = format!("{current:.2}");
            target.last_updated_at = now;
            target.notes = notes;
            target.clone()
        }
        None => {
            let asset = InvestmentAsset {
                id: make_asset_id(),
                asset_type: asset_type.to_string(),
                asset_name: asset_name.to_string(),
                invested_amount: format!("{invested:.2}"),
                current_value: format!("{current:.2}"),
                last_updated_at: now,
                notes,
            };
            doc.assets.push(asset.clone());
            asset
        }
    };

    save(&state, &user, &dek, &doc)?;
    Ok(asset)
}

#[tauri::command]
pub fn delete_investment(id: String, state: State<AppState>) -> Result<(), String> {
    let (user, dek) = session(&state)?;
    let mut doc = load(&state, &user, &dek)?;
    let before = doc.assets.len();
    doc.assets.retain(|a| a.id != id);
    if doc.assets.len() == before {
        return Err(format!("asset {id} not found"));
    }
    save(&state, &user, &dek, &doc)?;
    Ok(())
}

#[tauri::command]
pub fn investments_summary(state: State<AppState>) -> Result<InvestmentsSummary, String> {
    let (user, dek) = session(&state)?;
    let doc = load(&state, &user, &dek)?;
    Ok(summarise(&doc.assets))
}

fn summarise(assets: &[InvestmentAsset]) -> InvestmentsSummary {
    let mut total_invested = Decimal::ZERO;
    let mut total_current = Decimal::ZERO;
    use std::collections::HashMap;
    #[derive(Default)]
    struct TypeAcc {
        current: Decimal,
        count: u32,
    }
    let mut by_type: HashMap<String, TypeAcc> = HashMap::new();

    for a in assets {
        // Invariant: amounts are validated on every write via parse_amount,
        // so by the time they reach summarise() they're well-formed decimal
        // strings. A panic here would point at a regression in a future
        // schema migration that forgot to backfill the formatting.
        let inv =
            Decimal::from_str(&a.invested_amount).expect("invested_amount is normalized on write");
        let cur =
            Decimal::from_str(&a.current_value).expect("current_value is normalized on write");
        total_invested += inv;
        total_current += cur;
        let entry = by_type.entry(a.asset_type.clone()).or_default();
        entry.current += cur;
        entry.count += 1;
    }

    let gain = total_current - total_invested;
    let return_pct = if total_invested > Decimal::ZERO {
        let pct = (gain * Decimal::new(100, 0)) / total_invested;
        format!("{pct:.2}")
    } else {
        String::new()
    };

    let mut allocation: Vec<AllocationSlice> = by_type
        .into_iter()
        .map(|(asset_type, acc)| {
            let share_pct = if total_current > Decimal::ZERO {
                let pct = (acc.current * Decimal::new(100, 0)) / total_current;
                format!("{pct:.2}")
            } else {
                "0.00".to_string()
            };
            AllocationSlice {
                asset_type,
                current_value: format!("{:.2}", acc.current),
                share_pct,
                asset_count: acc.count,
            }
        })
        .collect();
    allocation.sort_by(|a, b| {
        let av: Decimal = a.current_value.parse().unwrap_or(Decimal::ZERO);
        let bv: Decimal = b.current_value.parse().unwrap_or(Decimal::ZERO);
        bv.cmp(&av).then_with(|| a.asset_type.cmp(&b.asset_type))
    });

    InvestmentsSummary {
        asset_count: assets.len() as u32,
        total_invested: format!("{total_invested:.2}"),
        total_current_value: format!("{total_current:.2}"),
        unrealized_gain_loss: format!("{gain:.2}"),
        return_pct,
        allocation,
    }
}

fn parse_amount(raw: &str, label: &str) -> Result<Decimal, String> {
    let trimmed = raw.trim().replace(',', "");
    Decimal::from_str(&trimmed).map_err(|e| format!("{label} '{raw}' is not a decimal: {e}"))
}

fn load(state: &State<AppState>, user: &UserId, dek: &KeyBytes) -> Result<InvestmentsDoc, String> {
    if !state
        .storage
        .exists(user, FILE_PATH)
        .map_err(|e| e.to_string())?
    {
        return Ok(InvestmentsDoc::default());
    }
    let sealed = state
        .storage
        .read(user, FILE_PATH)
        .map_err(|e| e.to_string())?;
    let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
    let parsed: VersionedJson<InvestmentsDoc> =
        serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
    if parsed.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "investments file has unsupported schema version {}",
            parsed.schema_version
        ));
    }
    Ok(parsed.data)
}

fn save(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    doc: &InvestmentsDoc,
) -> Result<(), String> {
    let envelope = VersionedJson::new(SCHEMA_VERSION, doc);
    let plaintext = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
    let sealed = seal(dek, &plaintext).map_err(|e| e.to_string())?;
    state
        .storage
        .write(user, FILE_PATH, &sealed)
        .map_err(|e| e.to_string())
}

fn make_asset_id() -> String {
    let mut rnd = [0u8; 6];
    OsRng.fill_bytes(&mut rnd);
    format!(
        "inv:{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
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

    fn asset(asset_type: &str, name: &str, invested: &str, current: &str) -> InvestmentAsset {
        InvestmentAsset {
            id: format!("inv:{name}"),
            asset_type: asset_type.into(),
            asset_name: name.into(),
            invested_amount: invested.into(),
            current_value: current.into(),
            last_updated_at: "2026-05-28T00:00:00Z".into(),
            notes: None,
        }
    }

    #[test]
    fn empty_summary_returns_zeros() {
        let s = summarise(&[]);
        assert_eq!(s.asset_count, 0);
        assert_eq!(s.total_invested, "0.00");
        assert_eq!(s.total_current_value, "0.00");
        assert_eq!(s.unrealized_gain_loss, "0.00");
        assert!(s.return_pct.is_empty());
        assert!(s.allocation.is_empty());
    }

    #[test]
    fn summary_computes_totals_gain_and_return_pct() {
        let assets = vec![
            asset("Mutual Fund", "PPFAS Flexicap", "100000.00", "125000.00"),
            asset("Stock", "INFY", "50000.00", "45000.00"),
            asset("FD", "HDFC 1Y", "200000.00", "210000.00"),
        ];
        let s = summarise(&assets);
        assert_eq!(s.asset_count, 3);
        assert_eq!(s.total_invested, "350000.00");
        assert_eq!(s.total_current_value, "380000.00");
        assert_eq!(s.unrealized_gain_loss, "30000.00");
        // 30000 / 350000 * 100 = 8.5714...
        assert!(s.return_pct.starts_with("8.57"));
    }

    #[test]
    fn allocation_sorted_by_current_value_desc() {
        let assets = vec![
            asset("Stock", "A", "10.00", "15.00"),
            asset("FD", "B", "100.00", "100.00"),
            asset("Mutual Fund", "C", "50.00", "60.00"),
        ];
        let s = summarise(&assets);
        assert_eq!(s.allocation[0].asset_type, "FD");
        assert_eq!(s.allocation[1].asset_type, "Mutual Fund");
        assert_eq!(s.allocation[2].asset_type, "Stock");
    }

    #[test]
    fn share_pct_sums_to_100_when_value_is_positive() {
        let assets = vec![
            asset("Stock", "A", "0.00", "30.00"),
            asset("FD", "B", "0.00", "70.00"),
        ];
        let s = summarise(&assets);
        let sum: f64 = s
            .allocation
            .iter()
            .map(|a| a.share_pct.parse::<f64>().unwrap_or(0.0))
            .sum();
        assert!((sum - 100.0).abs() < 0.01);
    }

    #[test]
    fn return_pct_empty_when_invested_zero() {
        let assets = vec![asset("Stock", "Gift", "0.00", "100.00")];
        let s = summarise(&assets);
        assert_eq!(s.unrealized_gain_loss, "100.00");
        assert!(s.return_pct.is_empty());
    }

    #[test]
    fn parse_amount_accepts_comma_separated_indian_format() {
        assert_eq!(
            parse_amount("1,25,000.50", "x").unwrap(),
            Decimal::from_str("125000.50").unwrap()
        );
    }

    #[test]
    fn parse_amount_rejects_garbage() {
        assert!(parse_amount("not a number", "x").is_err());
    }
}
