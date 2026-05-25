use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseAttempt {
    pub file: PathBuf,
    pub backend: Backend,
    pub outcome: Outcome,
    pub pages: Option<u32>,
    pub rows_extracted: Option<u32>,
    pub elapsed_ms: u128,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Backend {
    Pdfium,
    PdfExtract,
    Tesseract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    PartialOk,
    PasswordProtected,
    Empty,
    Error,
}

impl ParseAttempt {
    pub fn timed<F, T>(file: PathBuf, backend: Backend, f: F) -> (Self, Option<T>)
    where
        F: FnOnce() -> anyhow::Result<(T, Outcome, Option<u32>, Option<u32>, Vec<String>)>,
    {
        let start = std::time::Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        match result {
            Ok((value, outcome, pages, rows, notes)) => (
                ParseAttempt {
                    file,
                    backend,
                    outcome,
                    pages,
                    rows_extracted: rows,
                    elapsed_ms: elapsed.as_millis(),
                    notes,
                },
                Some(value),
            ),
            Err(err) => (
                ParseAttempt {
                    file,
                    backend,
                    outcome: Outcome::Error,
                    pages: None,
                    rows_extracted: None,
                    elapsed_ms: elapsed.as_millis(),
                    notes: vec![format!("error: {err}")],
                },
                None,
            ),
        }
    }
}

pub fn fmt_duration(ms: u128) -> String {
    let d = Duration::from_millis(ms as u64);
    if d.as_secs() >= 1 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}
