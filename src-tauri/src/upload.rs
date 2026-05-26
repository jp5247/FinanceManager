//! `upload_pdf` Tauri command — the seam between the UI's file picker and
//! the full fm-pdf → fm-parser → encrypted storage chain.

use crate::state::AppState;
use fm_core::UserId;
use fm_crypto::{seal, KeyBytes};
use fm_parser::{default_adapters, detect_adapter, RawTransaction};
use fm_pdf::PdfExtractor;
use fm_storage::{StorageRepository, VersionedJson};
use rand::rngs::OsRng;
use rand::RngCore;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
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
    // Per-import summary so the UI can show counts/totals without
    // re-reading the encrypted raw-transactions file. Added with serde
    // defaults so v1-format imports (without these fields) still load.
    #[serde(default)]
    pub debit_count: u32,
    #[serde(default)]
    pub credit_count: u32,
    /// Sum of all debit amounts, as a decimal string (e.g. "274712.52").
    #[serde(default = "zero_str")]
    pub total_debit: String,
    #[serde(default = "zero_str")]
    pub total_credit: String,
}

fn zero_str() -> String {
    "0.00".to_string()
}

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
    /// Up to the first 50 parsed rows so the UI can render a preview without
    /// re-reading the encrypted file.
    pub preview: Vec<RawTransaction>,
}

/// Run an unlocked-session-required PDF upload end-to-end.
#[tauri::command]
pub fn upload_pdf(
    file_path: String,
    password: Option<String>,
    state: State<AppState>,
) -> Result<UploadResult, String> {
    let (user, dek) = {
        let guard = state.session.lock().map_err(|e| e.to_string())?;
        let s = guard
            .as_ref()
            .ok_or_else(|| "no profile is unlocked".to_string())?;
        (s.user_id().clone(), s.key().clone())
    };

    let extractor = PdfExtractor::new().map_err(|e| e.to_string())?;
    let extracted = extractor
        .extract(Path::new(&file_path), password.as_deref())
        .map_err(|e| e.to_string())?;

    let adapters = default_adapters();
    let adapter = detect_adapter(&adapters, &extracted)
        .ok_or_else(|| "no parser recognized this PDF format".to_string())?;

    let import_id = make_import_id();
    let rows = adapter
        .parse(&extracted, &import_id)
        .map_err(|e| e.to_string())?;

    let summary = summarise(&rows);

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

    let preview = rows.iter().take(50).cloned().collect();

    Ok(UploadResult {
        import_id,
        uploaded_at: now,
        source_file: extracted.source_file,
        source_sha256: extracted.source_sha256,
        adapter_id: adapter.id().to_string(),
        page_count: extracted.pages.len() as u32,
        transaction_count: rows.len() as u32,
        debit_count: summary.debit_count,
        credit_count: summary.credit_count,
        total_debit: format!("{:.2}", summary.total_debit),
        total_credit: format!("{:.2}", summary.total_credit),
        preview,
    })
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

/// Walk the user's `source/uploads/` tree and return one [`FileMeta`] per
/// import, newest first.
#[tauri::command]
pub fn list_imports(state: State<AppState>) -> Result<Vec<FileMeta>, String> {
    let (user, dek) = {
        let guard = state.session.lock().map_err(|e| e.to_string())?;
        let s = guard
            .as_ref()
            .ok_or_else(|| "no profile is unlocked".to_string())?;
        (s.user_id().clone(), s.key().clone())
    };

    let uploads_dir = state
        .data_root
        .profile(&user)
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
                let Ok(true) = state.storage.exists(&user, &meta_rel) else {
                    continue;
                };
                let Ok(sealed) = state.storage.read(&user, &meta_rel) else {
                    continue;
                };
                let Ok(plaintext) = fm_crypto::open(&dek, &sealed) else {
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

fn upload_path(import_id: &str, file_name: &str) -> String {
    // import_id format: imp-YYYY-MM-DD-RRRRRR ; we recover Y/M from it.
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
        // imp-YYYY-MM-DD-RRRRRR == 4 + 4 + 1 + 2 + 1 + 2 + 1 + 6 = 21 chars
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
}
