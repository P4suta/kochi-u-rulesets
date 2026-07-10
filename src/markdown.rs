use std::fmt::Write;

use crate::model::{AppendedTable, Article, BodyNode, Document, Paragraph, SupplementaryBlock};

/// Renders a `Document` as a human-readable Markdown document, mirroring the
/// original reading order. The body is walked recursively, so a 章-less
/// regulation renders its articles at the top heading level with no empty
/// chapter scaffolding. 附則/別表 raw blocks are fenced as code so their
/// (already messy) whitespace-based table layout isn't reinterpreted as
/// Markdown syntax.
pub fn render(doc: &Document) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# {}", doc.title);
    if !doc.enacted.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", doc.enacted);
    }
    if !doc.last_amended.is_empty() {
        let _ = writeln!(out, "{}", doc.last_amended);
    }

    for node in &doc.body {
        render_node(&mut out, node, 0);
    }

    if !doc.supplementary_provisions.is_empty() {
        let _ = writeln!(out, "\n## 附則");
        for block in &doc.supplementary_provisions {
            render_supplementary(&mut out, block);
        }
    }

    if !doc.appended_tables.is_empty() {
        let _ = writeln!(out, "\n## 別表");
        for table in &doc.appended_tables {
            render_table(&mut out, table);
        }
    }

    out
}

/// `depth` is the structural nesting depth (0 at the body root). Headings start
/// at level 2 (`##`) so 章 → `##`, 節 → `###`, and articles sit one level below
/// their enclosing container — or at `##` when there is no container at all.
fn render_node(out: &mut String, node: &BodyNode, depth: usize) {
    match node {
        BodyNode::Chapter {
            number,
            title,
            children,
        } => {
            let _ = writeln!(out, "\n{} 第{number}章　{title}", "#".repeat(depth + 2));
            for child in children {
                render_node(out, child, depth + 1);
            }
        }
        BodyNode::Section {
            number,
            title,
            children,
        } => {
            let _ = writeln!(out, "\n{} 第{number}節　{title}", "#".repeat(depth + 2));
            for child in children {
                render_node(out, child, depth + 1);
            }
        }
        BodyNode::Article(article) => render_article(out, article, depth + 2),
    }
}

fn render_article(out: &mut String, article: &Article, level: usize) {
    let heading = "#".repeat(level);
    match &article.title {
        Some(title) => {
            let _ = writeln!(out, "\n{heading} {}（{title}）", article.number);
        }
        None => {
            let _ = writeln!(out, "\n{heading} {}", article.number);
        }
    }
    for paragraph in &article.paragraphs {
        render_paragraph(out, paragraph);
    }
}

fn render_paragraph(out: &mut String, paragraph: &Paragraph) {
    if paragraph.number == 1 {
        let _ = writeln!(out, "\n{}", paragraph.text);
    } else {
        let _ = writeln!(out, "\n{}. {}", paragraph.number, paragraph.text);
    }
    for item in &paragraph.items {
        let _ = writeln!(out, "{}. {}", item.number, item.text);
    }
}

fn render_supplementary(out: &mut String, block: &SupplementaryBlock) {
    let _ = writeln!(out, "\n### {}", block.heading_raw);
    if !block.references_tables.is_empty() {
        let _ = writeln!(out, "\n*関連: {}*", block.references_tables.join("、"));
    }
    let _ = writeln!(out, "\n```\n{}\n```", block.raw_text);
}

fn render_table(out: &mut String, table: &AppendedTable) {
    let _ = writeln!(out, "\n### {}（{}）", table.id, table.related_article_raw);
    let _ = writeln!(out, "\n```\n{}\n```", table.raw_text);
}
