use std::path::Path;

use anyhow::Context;

/// Extract per-page text from a PDF. The only place `pdf-extract` is called from —
/// swapping extraction crates means touching just this function.
pub fn extract_pages(path: impl AsRef<Path>) -> anyhow::Result<Vec<String>> {
    let path = path.as_ref();
    pdf_extract::extract_text_by_pages(path)
        .with_context(|| format!("failed to extract text from {}", path.display()))
}
