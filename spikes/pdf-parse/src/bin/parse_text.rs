use anyhow::{Context, Result};
use clap::Parser;
use fm_pdf_parse_spike::{fmt_duration, Backend, Outcome, ParseAttempt};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(about = "Extract text from PDF statements using pdfium + pdf-extract")]
struct Args {
    /// Folder containing PDF statements
    #[arg(short, long)]
    input: PathBuf,

    /// Folder to write per-file extracted text and a summary JSON
    #[arg(short, long, default_value = "./output")]
    output: PathBuf,

    /// Path to the pdfium dynamic library directory. If omitted, pdfium-render
    /// looks for pdfium in the same directory as the executable, then PATH.
    #[arg(long)]
    pdfium_dir: Option<PathBuf>,

    /// Optional PDF password to try for protected statements
    #[arg(long)]
    password: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.output)?;

    let pdfium = load_pdfium(args.pdfium_dir.as_deref())?;
    let mut attempts: Vec<ParseAttempt> = Vec::new();

    for entry in WalkDir::new(&args.input).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) != Some("pdf".into()) {
            continue;
        }

        let file_stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        println!("\n--- {} ---", path.display());

        let (att_pdfium, text_pdfium) = try_pdfium(&pdfium, path, args.password.as_deref());
        report(&att_pdfium);
        if let Some(text) = text_pdfium.as_ref() {
            let out_path = args.output.join(format!("{file_stem}.pdfium.txt"));
            std::fs::write(&out_path, text)?;
        }
        attempts.push(att_pdfium);

        let (att_extract, text_extract) = try_pdf_extract(path);
        report(&att_extract);
        if let Some(text) = text_extract.as_ref() {
            let out_path = args.output.join(format!("{file_stem}.pdf-extract.txt"));
            std::fs::write(&out_path, text)?;
        }
        attempts.push(att_extract);
    }

    let summary_path = args.output.join("summary.json");
    std::fs::write(&summary_path, serde_json::to_string_pretty(&attempts)?)?;
    println!("\nWrote summary -> {}", summary_path.display());
    Ok(())
}

fn load_pdfium(pdfium_dir: Option<&Path>) -> Result<Pdfium> {
    let bindings = match pdfium_dir {
        Some(dir) => Pdfium::bind_to_library(
            Pdfium::pdfium_platform_library_name_at_path(dir),
        )
        .context("failed to load pdfium from --pdfium-dir")?,
        None => Pdfium::bind_to_system_library()
            .context("could not find pdfium; place pdfium.dll next to the exe or pass --pdfium-dir")?,
    };
    Ok(Pdfium::new(bindings))
}

fn try_pdfium(
    pdfium: &Pdfium,
    path: &Path,
    password: Option<&str>,
) -> (ParseAttempt, Option<String>) {
    ParseAttempt::timed(path.to_path_buf(), Backend::Pdfium, || {
        let doc_result = match password {
            Some(pw) => pdfium.load_pdf_from_file(path, Some(pw)),
            None => pdfium.load_pdf_from_file(path, None),
        };

        let doc = match doc_result {
            Ok(d) => d,
            Err(PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordRequired)) => {
                return Ok((
                    String::new(),
                    Outcome::PasswordProtected,
                    None,
                    None,
                    vec!["password required".to_string()],
                ));
            }
            Err(e) => return Err(anyhow::anyhow!("pdfium load failed: {e:?}")),
        };

        let pages = doc.pages();
        let page_count = pages.len() as u32;
        let mut buf = String::new();
        let mut row_estimate: u32 = 0;
        for page in pages.iter() {
            let text = page.text().map(|t| t.all()).unwrap_or_default();
            row_estimate += text.lines().filter(|l| likely_txn_row(l)).count() as u32;
            buf.push_str(&text);
            buf.push('\n');
        }

        let outcome = if buf.trim().is_empty() {
            Outcome::Empty
        } else if row_estimate == 0 {
            Outcome::PartialOk
        } else {
            Outcome::Ok
        };

        Ok((buf, outcome, Some(page_count), Some(row_estimate), vec![]))
    })
}

fn try_pdf_extract(path: &Path) -> (ParseAttempt, Option<String>) {
    ParseAttempt::timed(path.to_path_buf(), Backend::PdfExtract, || {
        let text = pdf_extract::extract_text(path)
            .map_err(|e| anyhow::anyhow!("pdf-extract failed: {e}"))?;
        let row_estimate = text.lines().filter(|l| likely_txn_row(l)).count() as u32;
        let outcome = if text.trim().is_empty() {
            Outcome::Empty
        } else if row_estimate == 0 {
            Outcome::PartialOk
        } else {
            Outcome::Ok
        };
        Ok((text, outcome, None, Some(row_estimate), vec![]))
    })
}

fn likely_txn_row(line: &str) -> bool {
    // Heuristic only — a "transaction-looking" line usually has a date and an amount.
    let has_date = line.chars().filter(|c| c.is_ascii_digit()).count() >= 4
        && (line.contains('/') || line.contains('-') || line.contains(' '));
    let has_amount = line
        .split_whitespace()
        .any(|tok| tok.chars().any(|c| c.is_ascii_digit()) && (tok.contains('.') || tok.contains(',')));
    has_date && has_amount
}

fn report(a: &ParseAttempt) {
    println!(
        "  [{:?}] outcome={:?} pages={:?} rows~={:?} time={}",
        a.backend,
        a.outcome,
        a.pages,
        a.rows_extracted,
        fmt_duration(a.elapsed_ms)
    );
    for n in &a.notes {
        println!("    - {n}");
    }
}
