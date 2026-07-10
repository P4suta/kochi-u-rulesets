use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;
use pdf_extract::{Document, MediaBox, OutputDev, OutputError, Transform, output_doc_page};

use crate::model::{AppendixKind, Document as RuleDocument, Table};
use crate::structure::line_kind::{self, LineKind};
use crate::table;

/// Parses a PDF into a structured [`RuleDocument`], loading the file once and
/// running both the text pass (章/条/…, 附則, dates) and the coordinate pass
/// (別表 grids). Reconstructed cells are attached to their appendix by document
/// order; `raw_text` is always kept, so a cell mismatch can never lose content.
pub fn parse_pdf(path: impl AsRef<Path>) -> anyhow::Result<RuleDocument> {
    let path = path.as_ref();
    let doc =
        Document::load(path).with_context(|| format!("failed to load PDF {}", path.display()))?;
    let pages = extract_pages_from_doc(&doc);
    let mut document = crate::structure::parse(&pages)?;

    let tables = appendix_tables(&doc);
    // Both passes enumerate the same 別表/様式 markers in document order; only
    // attach when the counts agree, otherwise fall back to raw for the document.
    if tables.len() == document.appendices.len() {
        for (appendix, table) in document.appendices.iter_mut().zip(tables) {
            appendix.cells = table;
        }
    }
    Ok(document)
}

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
    Ok(extract_pages_from_doc(&doc))
}

/// Per-page ruby-free text for an already-loaded document (so callers that also
/// need the coordinate pass load the PDF only once).
pub fn extract_pages_from_doc(doc: &Document) -> Vec<String> {
    let ruby_threshold = modal_font_size(doc).map(|body| body * 0.7);
    let mut pages = Vec::new();
    let mut page_num = 1;
    loop {
        let mut text = String::new();
        let mut out = RubyFilteredText::new(&mut text, ruby_threshold);
        match output_doc_page(doc, &mut out, page_num) {
            Ok(()) => {
                pages.push(text);
                page_num += 1;
            }
            // The page loop ends when a page number no longer exists, mirroring
            // `extract_text_by_pages`.
            Err(_) => break,
        }
    }
    pages
}

/// Reconstructs a grid for every 別表/様式 region, in document order, aligned to
/// the appendices produced by the text parser. `Some` only for 別表 regions whose
/// layout could be recovered with confidence; `None` for 様式 (single-column
/// forms) and for tables that don't cluster cleanly (the caller keeps raw_text).
pub fn appendix_tables(doc: &Document) -> Vec<Option<Table>> {
    let Some(body) = modal_font_size(doc) else {
        return Vec::new();
    };
    let mut collector = GlyphCollector::new(body * 0.7);
    let mut page_num = 1;
    while output_doc_page(doc, &mut collector, page_num).is_ok() {
        page_num += 1;
    }

    let rows = collector.into_visual_rows(body);
    let row_text =
        |r: &[PositionedGlyph]| -> String { r.iter().map(|g| g.text.as_str()).collect() };

    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if let LineKind::AppendixMarker { kind, .. } = line_kind::classify(&row_text(&rows[i])) {
            // The region runs until the next appendix marker (or end of document).
            let mut j = i + 1;
            while j < rows.len()
                && !matches!(
                    line_kind::classify(&row_text(&rows[j])),
                    LineKind::AppendixMarker { .. }
                )
            {
                j += 1;
            }
            let table = if kind == AppendixKind::Table {
                let region: Vec<table::Row> = rows[i + 1..j]
                    .iter()
                    .map(|r| {
                        r.iter()
                            .map(|g| table::Glyph {
                                x0: g.x,
                                x1: g.x1,
                                text: g.text.clone(),
                            })
                            .collect()
                    })
                    .collect();
                table::reconstruct(&region, body)
            } else {
                None
            };
            out.push(table);
            i = j;
        } else {
            i += 1;
        }
    }
    out
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

/// A single non-ruby glyph with its position and horizontal extent, for the
/// coordinate-based table reconstruction.
struct PositionedGlyph {
    page: u32,
    x: f64,
    x1: f64,
    y: f64,
    text: String,
}

/// Coordinate pass: collects every non-ruby glyph with its position, then groups
/// them into visual rows for [`appendix_tables`].
struct GlyphCollector {
    ruby_threshold: f64,
    page: u32,
    flip_ctm: Transform,
    glyphs: Vec<PositionedGlyph>,
}

impl GlyphCollector {
    fn new(ruby_threshold: f64) -> Self {
        Self {
            ruby_threshold,
            page: 0,
            flip_ctm: Transform::identity(),
            glyphs: Vec::new(),
        }
    }

    /// Sorts glyphs into reading order and groups them into visual rows, breaking
    /// on a page change or a vertical jump larger than `body * 0.6`.
    fn into_visual_rows(mut self, body: f64) -> Vec<Vec<PositionedGlyph>> {
        self.glyphs.sort_by(|a, b| {
            a.page
                .cmp(&b.page)
                .then(a.y.partial_cmp(&b.y).unwrap())
                .then(a.x.partial_cmp(&b.x).unwrap())
        });
        let mut rows: Vec<Vec<PositionedGlyph>> = Vec::new();
        let mut cur: Vec<PositionedGlyph> = Vec::new();
        let (mut last_y, mut last_page) = (f64::MIN, u32::MAX);
        for g in self.glyphs {
            if !cur.is_empty() && (g.page != last_page || (g.y - last_y).abs() > body * 0.6) {
                rows.push(std::mem::take(&mut cur));
            }
            (last_y, last_page) = (g.y, g.page);
            cur.push(g);
        }
        if !cur.is_empty() {
            rows.push(cur);
        }
        rows
    }
}

impl OutputDev for GlyphCollector {
    fn begin_page(
        &mut self,
        page_num: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.page = page_num;
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
        if size < self.ruby_threshold {
            return Ok(());
        }
        let position = trm.post_transform(&self.flip_ctm);
        let x = position.m31;
        self.glyphs.push(PositionedGlyph {
            page: self.page,
            x,
            x1: x + width * size,
            y: position.m32,
            text: ch.to_string(),
        });
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
