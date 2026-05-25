use anyhow::{Context, Result};
use clap::Parser;
use fm_pdf_parse_spike::{fmt_duration, Backend, Outcome, ParseAttempt};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(about = "OCR scanned PDFs via pdfium render + Tesseract CLI")]
struct Args {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long, default_value = "./output-ocr")]
    output: PathBuf,

    #[arg(long)]
    pdfium_dir: Option<PathBuf>,

    /// Path to the tesseract executable. Defaults to "tesseract" on PATH.
    #[arg(long, default_value = "tesseract")]
    tesseract: String,

    /// Render DPI. Higher = better OCR, slower.
    #[arg(long, default_value_t = 300)]
    dpi: u32,

    /// Language code for tesseract (e.g. "eng", "eng+hin").
    #[arg(long, default_value = "eng")]
    lang: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.output)?;

    verify_tesseract(&args.tesseract)?;
    let pdfium = load_pdfium(args.pdfium_dir.as_deref())?;
    let mut attempts: Vec<ParseAttempt> = Vec::new();

    for entry in WalkDir::new(&args.input).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) != Some("pdf".into()) {
            continue;
        }
        let file_stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        println!("\n--- {} ---", path.display());

        let (att, text) = ocr_pdf(&pdfium, path, &args.output, &file_stem, args.dpi, &args.tesseract, &args.lang);
        println!(
            "  [{:?}] outcome={:?} pages={:?} rows~={:?} time={}",
            att.backend, att.outcome, att.pages, att.rows_extracted, fmt_duration(att.elapsed_ms)
        );
        if let Some(text) = text {
            std::fs::write(args.output.join(format!("{file_stem}.ocr.txt")), text)?;
        }
        attempts.push(att);
    }

    let summary_path = args.output.join("summary.json");
    std::fs::write(&summary_path, serde_json::to_string_pretty(&attempts)?)?;
    println!("\nWrote summary -> {}", summary_path.display());
    Ok(())
}

fn verify_tesseract(bin: &str) -> Result<()> {
    let out = Command::new(bin).arg("--version").output()
        .with_context(|| format!("could not run tesseract at '{bin}'. Install it or pass --tesseract <path>"))?;
    let v = String::from_utf8_lossy(&out.stdout);
    println!("Tesseract: {}", v.lines().next().unwrap_or("(unknown)"));
    Ok(())
}

fn load_pdfium(pdfium_dir: Option<&Path>) -> Result<Pdfium> {
    let bindings = match pdfium_dir {
        Some(dir) => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))
            .context("failed to load pdfium from --pdfium-dir")?,
        None => Pdfium::bind_to_system_library()
            .context("could not find pdfium; place pdfium.dll next to the exe or pass --pdfium-dir")?,
    };
    Ok(Pdfium::new(bindings))
}

fn ocr_pdf(
    pdfium: &Pdfium,
    path: &Path,
    out_dir: &Path,
    stem: &str,
    dpi: u32,
    tesseract_bin: &str,
    lang: &str,
) -> (ParseAttempt, Option<String>) {
    ParseAttempt::timed(path.to_path_buf(), Backend::Tesseract, || {
        let doc = pdfium.load_pdf_from_file(path, None)
            .map_err(|e| anyhow::anyhow!("pdfium load failed: {e:?}"))?;

        let render_cfg = PdfRenderConfig::new()
            .scale_page_by_factor(dpi as f32 / 72.0);

        let pages = doc.pages();
        let page_count = pages.len() as u32;
        let mut combined = String::new();
        let scratch = out_dir.join(format!("_scratch_{stem}"));
        std::fs::create_dir_all(&scratch)?;

        for (i, page) in pages.iter().enumerate() {
            let img_path = scratch.join(format!("page_{:03}.png", i + 1));
            let bitmap = page.render_with_config(&render_cfg)
                .map_err(|e| anyhow::anyhow!("render failed page {}: {e:?}", i + 1))?;
            bitmap.as_image().save(&img_path)
                .map_err(|e| anyhow::anyhow!("save image failed: {e}"))?;

            let txt_stem = scratch.join(format!("page_{:03}", i + 1));
            let status = Command::new(tesseract_bin)
                .arg(&img_path)
                .arg(&txt_stem)
                .arg("-l").arg(lang)
                .arg("--psm").arg("6")
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("tesseract failed on page {}", i + 1));
            }
            let txt_path = scratch.join(format!("page_{:03}.txt", i + 1));
            let page_text = std::fs::read_to_string(&txt_path).unwrap_or_default();
            combined.push_str(&page_text);
            combined.push('\n');
        }

        let rows = combined.lines().filter(|l| l.chars().any(|c| c.is_ascii_digit())).count() as u32;
        let outcome = if combined.trim().is_empty() {
            Outcome::Empty
        } else if rows == 0 {
            Outcome::PartialOk
        } else {
            Outcome::Ok
        };
        Ok((combined, outcome, Some(page_count), Some(rows), vec![format!("dpi={dpi} lang={lang}")]))
    })
}
