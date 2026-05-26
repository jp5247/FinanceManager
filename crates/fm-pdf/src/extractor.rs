use crate::error::PdfExtractError;
use crate::hash::sha256_file;
use fm_parser::{ExtractedPdf, PageText, ParserBackend};
use pdfium_render::prelude::*;
use std::path::Path;

/// Owns a loaded pdfium library handle. Reuse one instance for many extracts —
/// loading pdfium.dll is non-trivial overhead.
pub struct PdfExtractor {
    pdfium: Pdfium,
}

impl PdfExtractor {
    /// Construct an extractor by loading pdfium from the executable's
    /// directory first, then falling back to system search paths.
    pub fn new() -> Result<Self, PdfExtractError> {
        let bindings = bind_pdfium()?;
        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }

    /// Extract every page's text plus provenance fields.
    ///
    /// `password` is supplied to pdfium verbatim. On wrong/missing passwords,
    /// pdfium reports a generic password error which we map to
    /// [`PdfExtractError::PasswordRequired`] / [`PdfExtractError::WrongPassword`]
    /// based on whether the caller supplied a password.
    pub fn extract(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<ExtractedPdf, PdfExtractError> {
        let doc = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(|e| classify_load_error(e, password.is_some()))?;

        let mut pages = Vec::with_capacity(doc.pages().len() as usize);
        for (idx, page) in doc.pages().iter().enumerate() {
            let text = page.text().map(|t| t.all()).map_err(|e| {
                PdfExtractError::PageExtractFailed((idx as u32) + 1, format!("{e:?}"))
            })?;
            pages.push(PageText {
                page_number: (idx as u32) + 1,
                text,
            });
        }

        let source_file = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown.pdf".to_string());
        let source_sha256 = sha256_file(path)?;

        Ok(ExtractedPdf {
            source_file,
            source_sha256,
            backend: ParserBackend::Pdfium,
            pages,
        })
    }
}

fn bind_pdfium() -> Result<Box<dyn PdfiumLibraryBindings>, PdfExtractError> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let lib_name = Pdfium::pdfium_platform_library_name_at_path(dir);
            if let Ok(b) = Pdfium::bind_to_library(&lib_name) {
                return Ok(b);
            }
        }
    }
    Pdfium::bind_to_system_library().map_err(|_| PdfExtractError::PdfiumNotFound)
}

fn classify_load_error(err: PdfiumError, password_supplied: bool) -> PdfExtractError {
    let msg = format!("{err:?}").to_lowercase();
    if msg.contains("password") {
        if password_supplied {
            PdfExtractError::WrongPassword
        } else {
            PdfExtractError::PasswordRequired
        }
    } else {
        PdfExtractError::LoadFailed(format!("{err:?}"))
    }
}
