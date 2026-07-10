use std::collections::BTreeSet;

use kochi_u_rulesets::model::{AppendixKind, ArticleNumber, BodyNode};
use kochi_u_rulesets::{extract, structure};

fn pdf_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sources/高知大学学則.pdf")
}

/// Counts the 節 directly under a chapter node.
fn section_count(node: &BodyNode) -> usize {
    match node {
        BodyNode::Chapter { children, .. } => children
            .iter()
            .filter(|c| matches!(c, BodyNode::Section { .. }))
            .count(),
        _ => panic!("expected a Chapter node"),
    }
}

#[test]
fn body_article_base_numbers_match_the_table_of_contents() {
    // Compared on base article number only, not the (article, branch) pair:
    // a "第N条の２" inserted by a later amendment is covered by its enclosing
    // section's range in 目次 (e.g. "第31条－第34条" implicitly includes a
    // later-inserted 第33条の２) without being cited as its own entry, so the
    // 目次 is not a reliable source for which articles carry a branch — only
    // for the base 1..85 sequence being complete and gap-free.
    let pages = extract::extract_pages(pdf_path()).unwrap();
    let lines = structure::flatten_lines(&pages);
    let split = structure::split(&lines);

    let expected: BTreeSet<u32> = structure::toc::expected_article_numbers(split.toc_lines)
        .into_iter()
        .map(|n| n.main)
        .collect();
    let doc = structure::parse(&pages).unwrap();
    let actual: BTreeSet<u32> = doc.article_numbers().into_iter().map(|n| n.main).collect();

    // Asserting equality (not just a literal count) means this test stays valid
    // even if the university amends the 学則 and the article count changes.
    assert_eq!(
        actual, expected,
        "body article base numbers must match every ref in 目次"
    );
}

#[test]
fn known_facts_about_the_real_document() {
    let doc = extract::parse_pdf(pdf_path()).unwrap();

    assert_eq!(doc.title, "高知大学学則");
    let chapters: Vec<&BodyNode> = doc.chapters().collect();
    assert_eq!(chapters.len(), 8);

    // 第1条 (目的) — title from the preceding annotation, 3 items in paragraph 1.
    let art1 = doc.all_articles()[0];
    assert_eq!(
        art1.number,
        ArticleNumber {
            main: 1,
            branch: None
        }
    );
    assert_eq!(art1.title.as_deref(), Some("目的"));
    assert_eq!(art1.paragraphs[0].items.len(), 3);

    // Section counts per chapter, from the 目次.
    assert_eq!(section_count(chapters[1]), 9); // 第2章 通則
    assert_eq!(section_count(chapters[2]), 5); // 第3章 学部
    assert_eq!(section_count(chapters[3]), 5); // 第4章 大学院

    // The seven "の" branch articles found during manual verification.
    let branches: BTreeSet<ArticleNumber> = doc
        .article_numbers()
        .into_iter()
        .filter(|n| n.branch.is_some())
        .collect();
    let expected_branches: BTreeSet<ArticleNumber> = [
        (33, 2),
        (49, 2),
        (58, 2),
        (64, 2),
        (64, 3),
        (77, 2),
        (84, 2),
    ]
    .into_iter()
    .map(|(main, branch)| ArticleNumber {
        main,
        branch: Some(branch),
    })
    .collect();
    assert_eq!(branches, expected_branches);

    // 別表第1〜4, in order, each carrying its raw "第N条関係" text.
    let tables: Vec<&kochi_u_rulesets::model::Appendix> = doc.tables().collect();
    assert_eq!(tables.len(), 4);
    let ids: Vec<&str> = tables.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["別表第１", "別表第２", "別表第３", "別表第４"]);
    for t in &tables {
        assert!(
            !t.raw_text.is_empty(),
            "{} should carry raw table text",
            t.id
        );
        assert_eq!(t.kind, AppendixKind::Table);
    }

    // 別表第１/第２ reconstruct into multi-column grids from glyph coordinates;
    // the 入学定員 (275) and 収容定員 (1,100) of the first faculty land in the
    // right columns. (別表第３/第４ are single-column lists and stay raw.)
    let table1 = tables[0]
        .cells
        .as_ref()
        .expect("別表第１ should reconstruct");
    assert!(table1.rows[0].len() >= 4, "別表第１ should be multi-column");
    let flat: String = table1.rows.iter().flatten().cloned().collect();
    assert!(flat.contains("275") && flat.contains("1,100"));
    assert!(tables[1].cells.is_some(), "別表第２ should reconstruct");

    // The founding 附則 block has no amendment citation and is ordinal 1.
    let founding = &doc.supplementary_provisions[0];
    assert_eq!(founding.ordinal, 1);
    assert!(founding.amendment.is_none());

    // Every subsequent 附則 block carries a dated amendment citation.
    for block in &doc.supplementary_provisions[1..] {
        assert!(
            block.amendment.is_some(),
            "amendment block {} should carry a dated citation: {:?}",
            block.ordinal,
            block.heading_raw
        );
    }
}

#[test]
fn output_round_trips_through_json() {
    let pages = extract::extract_pages(pdf_path()).unwrap();
    let doc = structure::parse(&pages).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("高知大学学則"));
}
