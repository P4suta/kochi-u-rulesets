use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;
use pdf_extract::{Document, MediaBox, OutputDev, OutputError, Transform, output_doc_page};

/// Extract per-page text from a PDF, dropping ruby (振り仮名) glyphs.
///
/// `pdf-extract`'s own `extract_text_by_pages` renders every glyph, so ruby —
/// set at roughly half the body font size and positioned as its own run —
/// lands inline and shreds words ("剽ひょう窃", "涵かん養"). We instead drive
/// the same page renderer with a custom [`OutputDev`] that reproduces
/// `PlainTextOutput`'s line/space reconstruction verbatim but skips any glyph
/// smaller than a per-document ruby threshold. The threshold is derived from the
/// modal (body) font size, so documents without ruby are unaffected.
///
/// This is the only place `pdf-extract` is driven from — swapping extraction
/// backends means touching just this module.
pub fn extract_pages(path: impl AsRef<Path>) -> anyhow::Result<Vec<String>> {
    let path = path.as_ref();
    let doc =
        Document::load(path).with_context(|| format!("failed to load PDF {}", path.display()))?;

    let ruby_threshold = modal_font_size(&doc).map(|body| body * 0.7);

    let mut pages = Vec::new();
    let mut page_num = 1;
    loop {
        let mut text = String::new();
        let mut out = RubyFilteredText::new(&mut text, ruby_threshold);
        match output_doc_page(&doc, &mut out, page_num) {
            Ok(()) => {
                pages.push(text);
                page_num += 1;
            }
            // The page loop ends when a page number no longer exists, mirroring
            // `extract_text_by_pages`.
            Err(_) => break,
        }
    }
    Ok(pages)
}

/// The most common glyph font size across the document — the body text size.
/// Returns `None` for a document with no extractable glyphs (a scanned PDF).
fn modal_font_size(doc: &Document) -> Option<f64> {
    let mut collector = FontSizeHistogram::default();
    let mut page_num = 1;
    while output_doc_page(doc, &mut collector, page_num).is_ok() {
        page_num += 1;
    }
    collector
        .hist
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(bucket, _)| bucket as f64 / 4.0)
}

/// `transform_vector((fs, fs))` collapsed to the square-equivalent size, matching
/// `PlainTextOutput`'s font-size calculation without the private euclid vector.
fn transformed_font_size(trm: &Transform, font_size: f64) -> f64 {
    let vx = font_size * trm.m11 + font_size * trm.m21;
    let vy = font_size * trm.m12 + font_size * trm.m22;
    (vx * vy).abs().sqrt()
}

/// Pass 1: tallies transformed font sizes (bucketed to 0.25) to find the body size.
#[derive(Default)]
struct FontSizeHistogram {
    hist: std::collections::HashMap<i64, u64>,
}

impl OutputDev for FontSizeHistogram {
    fn begin_page(
        &mut self,
        _page_num: u32,
        _media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        _width: f64,
        _spacing: f64,
        font_size: f64,
        _char: &str,
    ) -> Result<(), OutputError> {
        let size = transformed_font_size(trm, font_size);
        *self.hist.entry((size * 4.0).round() as i64).or_default() += 1;
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

/// Pass 2: reproduces `PlainTextOutput`'s reconstruction, skipping ruby glyphs.
struct RubyFilteredText<'a> {
    out: &'a mut String,
    ruby_threshold: Option<f64>,
    last_end: f64,
    last_y: f64,
    first_char: bool,
    flip_ctm: Transform,
}

impl<'a> RubyFilteredText<'a> {
    fn new(out: &'a mut String, ruby_threshold: Option<f64>) -> Self {
        Self {
            out,
            ruby_threshold,
            last_end: 100_000.,
            last_y: 0.,
            first_char: false,
            flip_ctm: Transform::identity(),
        }
    }
}

impl OutputDev for RubyFilteredText<'_> {
    fn begin_page(
        &mut self,
        _page_num: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.flip_ctm = Transform::row_major(1., 0., 0., -1., 0., media_box.ury - media_box.lly);
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        ch: &str,
    ) -> Result<(), OutputError> {
        let size = transformed_font_size(trm, font_size);
        // Drop ruby: a glyph well below the body size. Skipping it entirely
        // (no state update) keeps it from disturbing the surrounding body run's
        // line/space reconstruction.
        if self.ruby_threshold.is_some_and(|t| size < t) {
            return Ok(());
        }

        let position = trm.post_transform(&self.flip_ctm);
        let (x, y) = (position.m31, position.m32);
        if self.first_char {
            if (y - self.last_y).abs() > size * 1.5 {
                let _ = writeln!(self.out);
            }
            if x < self.last_end && (y - self.last_y).abs() > size * 0.5 {
                let _ = writeln!(self.out);
            }
            if x > self.last_end + size * 0.1 {
                let _ = write!(self.out, " ");
            }
        }
        let _ = write!(self.out, "{ch}");
        self.first_char = false;
        self.last_y = y;
        self.last_end = x + width * size;
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        self.first_char = true;
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}
