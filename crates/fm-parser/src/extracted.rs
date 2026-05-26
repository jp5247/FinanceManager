use serde::{Deserialize, Serialize};

/// Which extraction backend produced the text. Recorded on every emitted
/// [`RawTransaction`](crate::RawTransaction) so backend-disagreement debugging
/// is mechanical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParserBackend {
    Pdfium,
    PdfExtract,
    OcrTesseract,
}

/// Text extracted from one page of a source PDF.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageText {
    /// 1-based page number within the original PDF.
    pub page_number: u32,
    pub text: String,
}

/// All the text extracted from one PDF, plus the provenance fields a parser
/// stamps onto every transaction it emits.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedPdf {
    pub source_file: String,
    pub source_sha256: String,
    pub backend: ParserBackend,
    pub pages: Vec<PageText>,
}
