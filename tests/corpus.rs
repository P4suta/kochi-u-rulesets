//! Corpus-wide regression + invariant checks over every ruleset PDF in `sources/`.
//!
//! Each parseable document gets a committed `insta` JSON snapshot (the durable
//! per-document fixture — review parse drift with `cargo insta review`) plus a
//! set of structural invariants that a snapshot alone would merely bless:
//! non-empty title, at least one article, and strictly increasing article
//! numbers (catches silent 目次-duplication or a broken TOC/body split).
//!
//! A scanned PDF with no text layer (`高知大学学生会館使用規則.pdf`) extracts to
//! whitespace only and is skipped — it cannot be parsed without OCR, which is
//! out of scope. Detection is by content, not an allowlist, so a future scraped
//! scan is handled the same way.

use kochi_u_rulesets::model::ArticleNumber;
use kochi_u_rulesets::{extract, structure};

fn has_text(pages: &[String]) -> bool {
    pages
        .iter()
        .flat_map(|p| p.chars())
        .any(|c| !c.is_whitespace())
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

fn assert_no_intra_cjk_space(text: &str, file: &str) {
    let chars: Vec<char> = text.chars().collect();
    for i in 1..chars.len().saturating_sub(1) {
        if chars[i] == ' ' && is_cjk(chars[i - 1]) && is_cjk(chars[i + 1]) {
            let ctx: String = chars[i.saturating_sub(6)..(i + 7).min(chars.len())]
                .iter()
                .collect();
            panic!("{file}: residual per-glyph space in prose near \"{ctx}\"");
        }
    }
}

fn assert_article_numbers_strictly_increase(numbers: &[ArticleNumber], file: &str) {
    for pair in numbers.windows(2) {
        assert!(
            pair[0] < pair[1],
            "{file}: article numbers must strictly increase in document order, but {} is not < {}",
            pair[0].labeled('条'),
            pair[1].labeled('条'),
        );
    }
}

#[test]
fn every_ruleset_pdf_parses_and_snapshots() {
    let sources = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sources");
    insta::glob!(&sources, "*.pdf", |path| {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let pages = extract::extract_pages(path)
            .unwrap_or_else(|e| panic!("{name}: extraction failed: {e:#}"));

        // Scanned PDF with no text layer: nothing to parse, skip (documented).
        if !has_text(&pages) {
            return;
        }

        // Full pipeline: text parse + coordinate-based 別表 grids.
        let doc =
            extract::parse_pdf(path).unwrap_or_else(|e| panic!("{name}: parse failed: {e:#}"));

        // --- Structural invariants (correctness floor) ---
        assert!(!doc.title.trim().is_empty(), "{name}: empty title");
        let articles = doc.all_articles();
        assert!(!articles.is_empty(), "{name}: no articles parsed");
        let numbers: Vec<ArticleNumber> = articles.iter().map(|a| a.number).collect();
        assert_article_numbers_strictly_increase(&numbers, &name);

        // Contamination lint: parsed prose must be free of the per-glyph spacing
        // that `pdf-extract` injects (a lone space between two CJK characters).
        // If a newly scraped PDF trips this, its extraction needs attention.
        for a in &articles {
            for p in &a.paragraphs {
                assert_no_intra_cjk_space(&p.text, &name);
                for it in &p.items {
                    assert_no_intra_cjk_space(&it.text, &name);
                    for sub in &it.subitems {
                        assert_no_intra_cjk_space(&sub.text, &name);
                    }
                }
            }
        }

        // --- TOC oracle (semantic check for documents carrying a 目次) ---
        // The 目次 independently lists which 条 a reader expects; every one must
        // appear in the parsed body (compared on base number, since a 目次 range
        // does not enumerate later-inserted branch articles).
        let lines = structure::flatten_lines(&pages);
        let split = structure::split(&lines);
        if !split.toc_lines.is_empty() {
            let expected: std::collections::BTreeSet<u32> =
                structure::toc::expected_article_numbers(split.toc_lines)
                    .into_iter()
                    .map(|n| n.main)
                    .collect();
            let actual: std::collections::BTreeSet<u32> = numbers.iter().map(|n| n.main).collect();
            let missing: Vec<u32> = expected.difference(&actual).copied().collect();
            assert!(
                missing.is_empty(),
                "{name}: articles listed in 目次 but missing from the body: {missing:?}",
            );
        }

        // Fallback integrity: an appendix must always carry its faithful
        // raw_text, so a (future) confident-but-wrong cell reconstruction can
        // never replace good content with garbage.
        for appx in &doc.appendices {
            assert!(
                !appx.raw_text.trim().is_empty(),
                "{name}: appendix {} has empty raw_text",
                appx.id
            );
        }

        // --- Regression snapshot (the committed per-document fixture) ---
        insta::assert_json_snapshot!(name.clone(), doc);
    });
}
