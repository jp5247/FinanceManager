use crate::adapter::BankAdapter;
use crate::adapters::hdfc_cc::HdfcCreditCardAdapter;
use crate::extracted::ExtractedPdf;

/// The default set of adapters shipped with this build. Order matters only
/// in that detection returns the first match; adapters should be specific
/// enough that two never both detect the same PDF.
pub fn default_adapters() -> Vec<Box<dyn BankAdapter + Send + Sync>> {
    vec![Box::new(HdfcCreditCardAdapter::new())]
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
