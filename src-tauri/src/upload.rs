//! Upload Tauri commands — the seam between the UI's file picker and the
//! full fm-pdf → fm-parser → encrypted storage chain, plus the history /
//! detail / delete operations the Upload UI uses for past imports.

use crate::state::AppState;
use fm_categorize::{categorize, default_rules, RuleSet, UNCATEGORIZED};
use fm_core::UserId;
use fm_crypto::{open, seal, KeyBytes};
use fm_parser::{default_adapters, detect_adapter, RawTransaction};
use fm_pdf::PdfExtractor;
use fm_storage::{StorageRepository, VersionedJson};
use rand::rngs::OsRng;
use rand::RngCore;
use rust_decimal::Decimal;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use tauri::State;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const FILE_META_SCHEMA: u32 = 1;
const RAW_TXN_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    pub import_id: String,
    pub uploaded_at: String,
    pub source_file: String,
    pub source_sha256: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub page_count: u32,
    pub transaction_count: u32,
    #[serde(default)]
    pub debit_count: u32,
    #[serde(default)]
    pub credit_count: u32,
    #[serde(default = "zero_str")]
    pub total_debit: String,
    #[serde(default = "zero_str")]
    pub total_credit: String,
    /// Per-category aggregates, sorted by total_debit descending. Empty for
    /// pre-categorization imports (graceful default).
    #[serde(default)]
    pub category_breakdown: Vec<CategoryBreakdown>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBreakdown {
    pub category: String,
    pub debit_count: u32,
    pub credit_count: u32,
    pub total_debit: String,
    pub total_credit: String,
}

fn zero_str() -> String {
    "0.00".to_string()
}

/// Returned by [`upload_pdf`] and [`get_import`] alike. Same shape so the UI
/// renders both via one path.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub import_id: String,
    pub uploaded_at: String,
    pub source_file: String,
    pub source_sha256: String,
    pub adapter_id: String,
    pub page_count: u32,
    pub transaction_count: u32,
    pub debit_count: u32,
    pub credit_count: u32,
    pub total_debit: String,
    pub total_credit: String,
    pub category_breakdown: Vec<CategoryBreakdown>,
    pub transactions: Vec<RawTransaction>,
}

#[tauri::command]
pub fn upload_pdf(
    file_path: String,
    password: Option<String>,
    state: State<AppState>,
) -> Result<UploadResult, String> {
    let (user, dek) = session(&state)?;

    let extractor = PdfExtractor::new().map_err(|e| e.to_string())?;
    let extracted = extractor
        .extract(Path::new(&file_path), password.as_deref())
        .map_err(|e| e.to_string())?;

    // Refuse re-imports of the exact same file by content hash.
    if let Some(prev) = find_import_by_sha256(&state, &user, &dek, &extracted.source_sha256)? {
        return Err(format!(
            "This PDF was already imported on {} as {} ({} txns). Delete that import first if you want to re-upload.",
            prev.uploaded_at, prev.import_id, prev.transaction_count
        ));
    }

    let adapters = default_adapters();
    let adapter = detect_adapter(&adapters, &extracted)
        .ok_or_else(|| "no parser recognized this PDF format".to_string())?;

    let import_id = make_import_id();
    let mut rows = adapter
        .parse(&extracted, &import_id)
        .map_err(|e| e.to_string())?;

    // Categorize each row before persistence.
    let rules = default_rules();
    apply_categories(&mut rows, &rules);

    let summary = summarise(&rows);
    let category_breakdown = build_category_breakdown(&rows);

    let now = now_rfc3339();
    let meta = FileMeta {
        import_id: import_id.clone(),
        uploaded_at: now.clone(),
        source_file: extracted.source_file.clone(),
        source_sha256: extracted.source_sha256.clone(),
        adapter_id: adapter.id().to_string(),
        adapter_version: adapter.version().to_string(),
        page_count: extracted.pages.len() as u32,
        transaction_count: rows.len() as u32,
        debit_count: summary.debit_count,
        credit_count: summary.credit_count,
        total_debit: format!("{:.2}", summary.total_debit),
        total_credit: format!("{:.2}", summary.total_credit),
        category_breakdown,
    };

    write_encrypted_json(
        &state,
        &user,
        &dek,
        &upload_path(&import_id, "file-meta.json"),
        &VersionedJson::new(FILE_META_SCHEMA, &meta),
    )?;
    write_encrypted_json(
        &state,
        &user,
        &dek,
        &upload_path(&import_id, "raw-transactions.json"),
        &VersionedJson::new(RAW_TXN_SCHEMA, &rows),
    )?;

    Ok(into_upload_result(meta, rows))
}

/// Walk the user's `source/uploads/` tree and return one [`FileMeta`] per
/// import, newest first.
#[tauri::command]
pub fn list_imports(state: State<AppState>) -> Result<Vec<FileMeta>, String> {
    let (user, dek) = session(&state)?;
    list_imports_internal(&state, &user, &dek)
}

/// Load one import's metadata + every parsed transaction. Used when the user
/// clicks a row in the Previous imports list.
#[tauri::command]
pub fn get_import(import_id: String, state: State<AppState>) -> Result<UploadResult, String> {
    let (user, dek) = session(&state)?;
    let meta_rel = upload_path(&import_id, "file-meta.json");
    if !state
        .storage
        .exists(&user, &meta_rel)
        .map_err(|e| e.to_string())?
    {
        return Err(format!("import {import_id} not found"));
    }
    let meta_doc: VersionedJson<FileMeta> = read_encrypted_json(&state, &user, &dek, &meta_rel)?;
    if meta_doc.schema_version != FILE_META_SCHEMA {
        return Err(format!("import {import_id} has unsupported schema version"));
    }
    let txn_rel = upload_path(&import_id, "raw-transactions.json");
    let txn_doc: VersionedJson<Vec<RawTransaction>> =
        read_encrypted_json(&state, &user, &dek, &txn_rel)?;
    if txn_doc.schema_version != RAW_TXN_SCHEMA {
        return Err(format!("import {import_id} has unsupported txn schema"));
    }
    Ok(into_upload_result(meta_doc.data, txn_doc.data))
}

/// Remove an import's directory entirely. Idempotent — missing dir is OK.
#[tauri::command]
pub fn delete_import(import_id: String, state: State<AppState>) -> Result<(), String> {
    let (user, _dek) = session(&state)?;
    let (year, month) =
        parse_year_month(&import_id).ok_or_else(|| format!("malformed import id: {import_id}"))?;
    let rel_dir = format!("source/uploads/{year}/{month}/{import_id}");
    let abs = state
        .data_root
        .profile(&user)
        .resolve(&rel_dir)
        .map_err(|e| e.to_string())?;
    if abs.exists() {
        std::fs::remove_dir_all(&abs).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn session(state: &State<AppState>) -> Result<(UserId, KeyBytes), String> {
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    let s = guard
        .as_ref()
        .ok_or_else(|| "no profile is unlocked".to_string())?;
    Ok((s.user_id().clone(), s.key().clone()))
}

fn list_imports_internal(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
) -> Result<Vec<FileMeta>, String> {
    let uploads_dir = state
        .data_root
        .profile(user)
        .as_path()
        .join("source")
        .join("uploads");
    if !uploads_dir.exists() {
        return Ok(Vec::new());
    }

    let mut metas: Vec<FileMeta> = Vec::new();
    for year_entry in std::fs::read_dir(&uploads_dir).map_err(|e| e.to_string())? {
        let year_entry = year_entry.map_err(|e| e.to_string())?;
        if !year_entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        for month_entry in std::fs::read_dir(year_entry.path()).map_err(|e| e.to_string())? {
            let month_entry = month_entry.map_err(|e| e.to_string())?;
            if !month_entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            for import_entry in std::fs::read_dir(month_entry.path()).map_err(|e| e.to_string())? {
                let import_entry = import_entry.map_err(|e| e.to_string())?;
                if !import_entry
                    .file_type()
                    .map_err(|e| e.to_string())?
                    .is_dir()
                {
                    continue;
                }
                let import_id_name = import_entry.file_name().to_string_lossy().to_string();
                let meta_rel = upload_path(&import_id_name, "file-meta.json");
                let Ok(true) = state.storage.exists(user, &meta_rel) else {
                    continue;
                };
                let Ok(sealed) = state.storage.read(user, &meta_rel) else {
                    continue;
                };
                let Ok(plaintext) = open(dek, &sealed) else {
                    continue;
                };
                let Ok(doc): Result<VersionedJson<FileMeta>, _> =
                    serde_json::from_slice(&plaintext)
                else {
                    continue;
                };
                if doc.schema_version != FILE_META_SCHEMA {
                    continue;
                }
                metas.push(doc.data);
            }
        }
    }

    metas.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
    Ok(metas)
}

fn find_import_by_sha256(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    sha256: &str,
) -> Result<Option<FileMeta>, String> {
    Ok(list_imports_internal(state, user, dek)?
        .into_iter()
        .find(|m| m.source_sha256 == sha256))
}

fn into_upload_result(meta: FileMeta, transactions: Vec<RawTransaction>) -> UploadResult {
    UploadResult {
        import_id: meta.import_id,
        uploaded_at: meta.uploaded_at,
        source_file: meta.source_file,
        source_sha256: meta.source_sha256,
        adapter_id: meta.adapter_id,
        page_count: meta.page_count,
        transaction_count: meta.transaction_count,
        debit_count: meta.debit_count,
        credit_count: meta.credit_count,
        total_debit: meta.total_debit,
        total_credit: meta.total_credit,
        category_breakdown: meta.category_breakdown,
        transactions,
    }
}

struct Summary {
    debit_count: u32,
    credit_count: u32,
    total_debit: Decimal,
    total_credit: Decimal,
}

fn summarise(rows: &[RawTransaction]) -> Summary {
    let mut s = Summary {
        debit_count: 0,
        credit_count: 0,
        total_debit: Decimal::ZERO,
        total_credit: Decimal::ZERO,
    };
    for r in rows {
        if let Some(d) = &r.debit {
            s.debit_count += 1;
            s.total_debit += d.as_decimal();
        }
        if let Some(c) = &r.credit {
            s.credit_count += 1;
            s.total_credit += c.as_decimal();
        }
    }
    s
}

fn apply_categories(rows: &mut [RawTransaction], rules: &RuleSet) {
    for r in rows {
        if let Some(hit) = categorize(rules, &r.description) {
            r.category = Some(hit.category);
            r.category_rule_id = Some(hit.rule_id.to_string());
        } else {
            r.category = Some(UNCATEGORIZED.to_string());
        }
    }
}

#[derive(Default)]
struct CatTotals {
    debit_count: u32,
    credit_count: u32,
    total_debit: Decimal,
    total_credit: Decimal,
}

fn build_category_breakdown(rows: &[RawTransaction]) -> Vec<CategoryBreakdown> {
    let mut by_cat: BTreeMap<String, CatTotals> = BTreeMap::new();
    for r in rows {
        let cat = r
            .category
            .clone()
            .unwrap_or_else(|| UNCATEGORIZED.to_string());
        let entry = by_cat.entry(cat).or_default();
        if let Some(d) = &r.debit {
            entry.debit_count += 1;
            entry.total_debit += d.as_decimal();
        }
        if let Some(c) = &r.credit {
            entry.credit_count += 1;
            entry.total_credit += c.as_decimal();
        }
    }

    let mut out: Vec<CategoryBreakdown> = by_cat
        .into_iter()
        .map(|(category, t)| CategoryBreakdown {
            category,
            debit_count: t.debit_count,
            credit_count: t.credit_count,
            total_debit: format!("{:.2}", t.total_debit),
            total_credit: format!("{:.2}", t.total_credit),
        })
        .collect();
    // Sort by total_debit DESC primarily; credit-only categories fall to the end.
    out.sort_by(|a, b| {
        let ad: Decimal = a.total_debit.parse().unwrap_or(Decimal::ZERO);
        let bd: Decimal = b.total_debit.parse().unwrap_or(Decimal::ZERO);
        bd.cmp(&ad).then_with(|| {
            let ac: Decimal = a.total_credit.parse().unwrap_or(Decimal::ZERO);
            let bc: Decimal = b.total_credit.parse().unwrap_or(Decimal::ZERO);
            bc.cmp(&ac)
        })
    });
    out
}

fn write_encrypted_json<T: Serialize>(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    rel_path: &str,
    doc: &VersionedJson<T>,
) -> Result<(), String> {
    let plaintext = serde_json::to_vec(doc).map_err(|e| e.to_string())?;
    let sealed = seal(dek, &plaintext).map_err(|e| e.to_string())?;
    state
        .storage
        .write(user, rel_path, &sealed)
        .map_err(|e| e.to_string())
}

fn read_encrypted_json<T: DeserializeOwned>(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    rel_path: &str,
) -> Result<T, String> {
    let sealed = state
        .storage
        .read(user, rel_path)
        .map_err(|e| e.to_string())?;
    let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
    serde_json::from_slice(&plaintext).map_err(|e| e.to_string())
}

fn upload_path(import_id: &str, file_name: &str) -> String {
    let (year, month) = parse_year_month(import_id).unwrap_or(("0000", "00"));
    format!("source/uploads/{year}/{month}/{import_id}/{file_name}")
}

fn parse_year_month(import_id: &str) -> Option<(&str, &str)> {
    let after_prefix = import_id.strip_prefix("imp-")?;
    let year = after_prefix.get(0..4)?;
    let month = after_prefix.get(5..7)?;
    Some((year, month))
}

fn make_import_id() -> String {
    let now = OffsetDateTime::now_utc();
    let mut rnd = [0u8; 3];
    OsRng.fill_bytes(&mut rnd);
    format!(
        "imp-{:04}-{:02}-{:02}-{:02x}{:02x}{:02x}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        rnd[0],
        rnd[1],
        rnd[2]
    )
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_id_shape() {
        let id = make_import_id();
        assert!(id.starts_with("imp-"), "id={id}");
        assert_eq!(id.len(), 21, "unexpected length for {id}");
    }

    #[test]
    fn upload_path_uses_year_month_from_id() {
        let p = upload_path("imp-2026-05-26-abcdef", "file-meta.json");
        assert_eq!(
            p,
            "source/uploads/2026/05/imp-2026-05-26-abcdef/file-meta.json"
        );
    }

    #[test]
    fn parse_year_month_rejects_bad_id() {
        assert!(parse_year_month("not-an-import-id").is_none());
        assert!(parse_year_month("imp-2026-99-99-abcdef").is_some());
    }
}
