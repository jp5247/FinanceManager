use crate::entry::{AuditEntry, EventInput};
use crate::error::AuditError;
use sha2::{Digest, Sha256};
use std::fmt::Write as FmtWrite;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// 64-char hex string used as the `prev_hash` of the very first entry.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const UNIT_SEP: u8 = 0x1F;

/// Wall-clock source. Injected so tests can use a fixed clock and produce
/// deterministic chains.
pub trait Clock {
    fn now_rfc3339(&self) -> String;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    }
}

/// Append-only writer over a single JSONL audit file.
///
/// Opens the file in `O_APPEND` mode, scans existing lines to recover the
/// chain head, and verifies the chain at open time. Once open, every
/// [`append`](Self::append) call writes one sealed entry.
#[derive(Debug)]
pub struct AuditAppender {
    file: File,
    last_hash: String,
}

impl AuditAppender {
    pub fn open(path: &Path) -> Result<Self, AuditError> {
        let parent = path.parent().ok_or(AuditError::InvalidPath)?;
        std::fs::create_dir_all(parent)?;
        let last_hash = if path.exists() {
            let (count, last) = scan_chain(path)?;
            let _ = count;
            last
        } else {
            GENESIS_HASH.to_string()
        };
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file, last_hash })
    }

    pub fn last_hash(&self) -> &str {
        &self.last_hash
    }

    pub fn append(
        &mut self,
        input: EventInput,
        clock: &dyn Clock,
    ) -> Result<AuditEntry, AuditError> {
        let ts = clock.now_rfc3339();
        let this_hash = compute_hash(
            &self.last_hash,
            &ts,
            &input.user_id,
            &input.action,
            input.entity_id.as_deref(),
            &input.details,
        );
        let entry = AuditEntry {
            ts,
            user_id: input.user_id,
            action: input.action,
            entity_id: input.entity_id,
            details: input.details,
            prev_hash: self.last_hash.clone(),
            this_hash: this_hash.clone(),
        };
        let line = serde_json::to_string(&entry)?;
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.last_hash = this_hash;
        Ok(entry)
    }
}

/// Validate the chain in `path` end-to-end. Returns the number of sealed
/// entries on success.
pub fn verify_chain(path: &Path) -> Result<usize, AuditError> {
    if !path.exists() {
        return Ok(0);
    }
    Ok(scan_chain(path)?.0)
}

/// Walks the file, returning `(entry_count, last_hash)`. The last hash is
/// `GENESIS_HASH` if the file is empty or contains only blank lines.
fn scan_chain(path: &Path) -> Result<(usize, String), AuditError> {
    let reader = BufReader::new(File::open(path)?);
    let mut prev = GENESIS_HASH.to_string();
    let mut count: usize = 0;

    for (line_idx, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: AuditEntry = serde_json::from_str(&line)
            .map_err(|e| AuditError::ParseError(line_idx, e.to_string()))?;

        if entry.prev_hash != prev {
            return Err(AuditError::ChainBreak(line_idx));
        }
        let expected = compute_hash(
            &prev,
            &entry.ts,
            &entry.user_id,
            &entry.action,
            entry.entity_id.as_deref(),
            &entry.details,
        );
        if expected != entry.this_hash {
            return Err(AuditError::TamperDetected(line_idx));
        }
        prev = entry.this_hash;
        count += 1;
    }
    Ok((count, prev))
}

fn compute_hash(
    prev: &str,
    ts: &str,
    user_id: &str,
    action: &str,
    entity_id: Option<&str>,
    details: &serde_json::Value,
) -> String {
    let mut h = Sha256::new();
    h.update(prev.as_bytes());
    h.update([UNIT_SEP]);
    h.update(ts.as_bytes());
    h.update([UNIT_SEP]);
    h.update(user_id.as_bytes());
    h.update([UNIT_SEP]);
    h.update(action.as_bytes());
    h.update([UNIT_SEP]);
    h.update(entity_id.unwrap_or("").as_bytes());
    h.update([UNIT_SEP]);
    // serde_json output is deterministic for a given Value (preserves
    // insertion order in objects, fixed encoding for scalars).
    let details_bytes = serde_json::to_vec(details).expect("Value always serializes");
    h.update(&details_bytes);

    let mut hex = String::with_capacity(64);
    for b in h.finalize() {
        write!(hex, "{b:02x}").expect("write to String never fails");
    }
    hex
}
