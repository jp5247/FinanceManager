use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfExtractError {
    #[error(
        "pdfium dynamic library not found — place pdfium.dll next to the executable or on PATH"
    )]
    PdfiumNotFound,

    #[error("PDF is password-protected; no password was supplied")]
    PasswordRequired,

    #[error("PDF password is incorrect")]
    WrongPassword,

    #[error("pdfium load failed: {0}")]
    LoadFailed(String),

    #[error("text extraction failed on page {0}: {1}")]
    PageExtractFailed(u32, String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
