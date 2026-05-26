use crate::adapter::BankAdapter;
use crate::adapters::hdfc_cc::HdfcCreditCardAdapter;
use crate::adapters::hdfc_savings::HdfcSavingsAdapter;
use crate::extracted::ExtractedPdf;

/// The default set of adapters shipped with this build. Order matters only
/// in that detection returns the first match; adapters should be specific
/// enough that two never both detect the same PDF.
///
/// More specific adapters should come first — for example, `hdfc-cc` must
/// be tried before `hdfc-savings` so a CC statement doesn't accidentally
/// match savings detection.
pub fn default_adapters() -> Vec<Box<dyn BankAdapter + Send + Sync>> {
    vec![
        Box::new(HdfcCreditCardAdapter::new()),
        Box::new(HdfcSavingsAdapter::new()),
    ]
}

/// Run `detect()` on each adapter in turn and return the first that claims
/// the PDF. `None` if no adapter recognized it.
pub fn detect_adapter<'a>(
    adapters: &'a [Box<dyn BankAdapter + Send + Sync>],
    extracted: &ExtractedPdf,
) -> Option<&'a (dyn BankAdapter + Send + Sync)> {
    adapters
        .iter()
        .map(|a| a.as_ref())
        .find(|a| a.detect(extracted))
}
