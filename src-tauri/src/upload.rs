//! Upload Tauri commands — the seam between the UI's file picker and the
//! full fm-pdf → fm-parser → encrypted storage chain, plus the history /
//! detail / delete operations the Upload UI uses for past imports.

use crate::llm;
use crate::llm_config;
use crate::merchant_cache;
use crate::state::AppState;
use crate::user_rules::{append_rule, load_rules, NewRuleSpec};
use fm_categorize::{
    build_rules, categorize, compile_stored, extract_merchant, RuleSet, UNCATEGORIZED,
};
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
    /// Surface a non-fatal note about external categorization: disabled,
    /// no key, network error, etc. The upload itself always succeeds —
    /// affected rows just stay uncategorized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookup_warning: Option<String>,
    /// Count of rows whose category came from the LLM in this run. Lets
    /// the UI surface "X categorized via Gemini" or similar.
    #[serde(default)]
    pub llm_categorized_count: u32,
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

    // Categorize: user rules first (overrides curated), then curated.
    let user_rules = compile_stored(&load_rules(&state, &user, &dek)?.rules);
    let rules = build_rules(user_rules);
    apply_categories(&mut rows, &rules);

    // External lookup: for rows still uncategorized, hit the merchant cache
    // then optionally the LLM. Best-effort — never fails the upload.
    let lookup = apply_external_lookup(&state, &user, &dek, &mut rows);

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

    let mut result = into_upload_result(meta, rows);
    result.lookup_warning = lookup.warning;
    result.llm_categorized_count = lookup.llm_count;
    Ok(result)
}

/// Walk the user's `source/uploads/` tree and return one [`FileMeta`] per
/// import, newest first.
#[tauri::command]
pub fn list_imports(state: State<AppState>) -> Result<Vec<FileMeta>, String> {
    let (user, dek) = session(&state)?;
    list_imports_internal(&state, &user, &dek)
}

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

#[tauri::command]
pub fn recategorize_transaction(
    import_id: String,
    row_number: u32,
    category: String,
    save_as_rule: Option<NewRuleSpec>,
    state: State<AppState>,
) -> Result<UploadResult, String> {
    let (user, dek) = session(&state)?;

    let meta_rel = upload_path(&import_id, "file-meta.json");
    let txn_rel = upload_path(&import_id, "raw-transactions.json");

    let meta_doc: VersionedJson<FileMeta> = read_encrypted_json(&state, &user, &dek, &meta_rel)?;
    let txn_doc: VersionedJson<Vec<RawTransaction>> =
        read_encrypted_json(&state, &user, &dek, &txn_rel)?;

    if meta_doc.schema_version != FILE_META_SCHEMA {
        return Err(format!("import {import_id} has unsupported meta schema"));
    }
    if txn_doc.schema_version != RAW_TXN_SCHEMA {
        return Err(format!("import {import_id} has unsupported txn schema"));
    }

    let mut rows = txn_doc.data;
    let row = rows
        .iter_mut()
        .find(|r| r.row_number == row_number)
        .ok_or_else(|| format!("row {row_number} not found in {import_id}"))?;

    let trimmed = category.trim();
    if trimmed.is_empty() {
        row.category = None;
        row.category_rule_id = None;
    } else {
        row.category = Some(trimmed.to_string());
        row.category_rule_id = Some("manual".to_string());
    }

    if let Some(spec) = save_as_rule {
        let spec = NewRuleSpec {
            match_type: spec.match_type,
            match_value: spec.match_value,
            category: if spec.category.trim().is_empty() {
                trimmed.to_string()
            } else {
                spec.category
            },
        };
        append_rule(&state, &user, &dek, spec)?;
    }

    let breakdown = build_category_breakdown(&rows);
    let mut meta = meta_doc.data;
    meta.category_breakdown = breakdown;

    write_encrypted_json(
        &state,
        &user,
        &dek,
        &meta_rel,
        &VersionedJson::new(FILE_META_SCHEMA, &meta),
    )?;
    write_encrypted_json(
        &state,
        &user,
        &dek,
        &txn_rel,
        &VersionedJson::new(RAW_TXN_SCHEMA, &rows),
    )?;

    Ok(into_upload_result(meta, rows))
}

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

pub(crate) fn session(state: &State<AppState>) -> Result<(UserId, KeyBytes), String> {
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
        lookup_warning: None,
        llm_categorized_count: 0,
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
struct LookupOutcome {
    warning: Option<String>,
    llm_count: u32,
}

struct Pending {
    row_idx: usize,
    merchant: String,
    direction: llm::Direction,
    cache_key: String,
}

fn collect_pending(rows: &[RawTransaction]) -> Vec<Pending> {
    let mut out = Vec::new();
    for (idx, r) in rows.iter().enumerate() {
        // Only consider rows that are explicitly Uncategorized — anything
        // with a real category was matched by user/curated rules and we
        // don't want to second-guess that.
        if r.category.as_deref() != Some(UNCATEGORIZED) {
            continue;
        }
        let extracted = extract_merchant(&r.description);
        if extracted.name.is_empty() {
            continue;
        }
        let direction = if r.debit.is_some() {
            llm::Direction::Debit
        } else if r.credit.is_some() {
            llm::Direction::Credit
        } else {
            continue;
        };
        let key = merchant_cache::cache_key(&extracted.name);
        out.push(Pending {
            row_idx: idx,
            merchant: extracted.name,
            direction,
            cache_key: key,
        });
    }
    out
}

fn apply_external_lookup(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    rows: &mut [RawTransaction],
) -> LookupOutcome {
    let pending = collect_pending(rows);
    if pending.is_empty() {
        return LookupOutcome::default();
    }

    // Step 1 — cache hits.
    let mut cache = match merchant_cache::load(state, user, dek) {
        Ok(c) => c,
        Err(e) => {
            return LookupOutcome {
                warning: Some(format!("merchant cache load failed: {e}")),
                llm_count: 0,
            };
        }
    };
    let mut still_pending: Vec<Pending> = Vec::new();
    for p in pending {
        if let Some(entry) = cache.entries.get(&p.cache_key) {
            if entry.category != UNCATEGORIZED {
                rows[p.row_idx].category = Some(entry.category.clone());
                rows[p.row_idx].category_rule_id = Some(format!("cache:{}", entry.source));
            }
            // Skip even when cached as Uncategorized — we've already asked
            // about this merchant; don't re-pay.
        } else {
            still_pending.push(p);
        }
    }

    if still_pending.is_empty() {
        return LookupOutcome::default();
    }

    // Step 2 — LLM.
    let cfg = match llm_config::load(state, user, dek) {
        Ok(c) => c,
        Err(e) => {
            return LookupOutcome {
                warning: Some(format!("llm config load failed: {e}")),
                llm_count: 0,
            };
        }
    };
    if !cfg.enabled {
        return LookupOutcome {
            warning: Some(format!(
                "LLM categorization disabled — {} merchant(s) left uncategorized",
                still_pending.len()
            )),
            llm_count: 0,
        };
    }
    if cfg.api_key.is_empty() {
        return LookupOutcome {
            warning: Some(
                "LLM enabled but no API key configured — open Settings to add one".to_string(),
            ),
            llm_count: 0,
        };
    }

    // Deduplicate by cache_key before sending.
    let mut by_key: BTreeMap<String, (String, llm::Direction)> = BTreeMap::new();
    for p in &still_pending {
        by_key
            .entry(p.cache_key.clone())
            .or_insert_with(|| (p.merchant.clone(), p.direction));
    }
    let items: Vec<llm::LookupItem> = by_key
        .values()
        .map(|(merchant, direction)| llm::LookupItem {
            merchant: merchant.clone(),
            direction: *direction,
        })
        .collect();

    let lookup_results = match llm::categorize_via_gemini(&cfg.api_key, &cfg.model, &items) {
        Ok(r) => r,
        Err(e) => {
            return LookupOutcome {
                warning: Some(format!("LLM lookup failed: {e}")),
                llm_count: 0,
            };
        }
    };

    let now = merchant_cache::now_rfc3339();
    let mut by_key_result: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for r in &lookup_results {
        let key = merchant_cache::cache_key(&r.merchant);
        by_key_result.insert(key.clone(), r.category.clone());
        cache.entries.insert(
            key,
            merchant_cache::MerchantCacheEntry {
                category: r.category.clone(),
                source: "gemini".to_string(),
                looked_up_at: now.clone(),
            },
        );
    }

    let mut llm_count: u32 = 0;
    for p in still_pending {
        if let Some(cat) = by_key_result.get(&p.cache_key) {
            if cat != UNCATEGORIZED {
                rows[p.row_idx].category = Some(cat.clone());
                rows[p.row_idx].category_rule_id = Some("llm:gemini".to_string());
                llm_count += 1;
            }
        }
    }

    if let Err(e) = merchant_cache::save(state, user, dek, &cache) {
        return LookupOutcome {
            warning: Some(format!("merchant cache save failed: {e}")),
            llm_count,
        };
    }

    LookupOutcome {
        warning: None,
        llm_count,
    }
}

fn build_category_breakdown(rows: &[RawTransaction]) -> Vec<CategoryBreakdown> {
    #[derive(Default)]
    struct CatTotals {
        debit_count: u32,
        credit_count: u32,
        total_debit: Decimal,
        total_credit: Decimal,
    }
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
