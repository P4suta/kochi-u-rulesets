use std::fmt::Write;

use crate::dates::{Effective, Enactment};
use crate::model::{
    Appendix, AppendixKind, Article, BodyNode, Document, Paragraph, SupplProvision,
};

/// Renders a `Document` as a human-readable Markdown document, mirroring the
/// original reading order. The body is walked recursively, so a 章-less
/// regulation renders its articles at the top heading level with no empty
/// chapter scaffolding. 附則/別表/様式 raw blocks are fenced as code so their
/// (already messy) whitespace-based table layout isn't reinterpreted as
/// Markdown syntax.
pub fn render(doc: &Document) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# {}", doc.title);
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", enactment_line(&doc.enacted));
    if let Some(amended) = &doc.last_amended {
        // `raw` already begins with "最終改正".
        let _ = writeln!(out, "{}", enactment_line(amended));
    }
    if let Some(note) = &doc.full_amendment_note {
        let _ = writeln!(out, "\n> {note}");
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

    if !doc.appendices.is_empty() {
        let _ = writeln!(out, "\n## 別表・様式");
        for appendix in &doc.appendices {
            render_appendix(&mut out, appendix);
        }
    }

    out
}

/// The raw line (which already carries the date and rule number verbatim),
/// annotated with the resolved ISO date when one was parsed.
fn enactment_line(e: &Enactment) -> String {
    match &e.date {
        Some(d) => format!("{}（{}）", e.raw, d.iso),
        None => e.raw.clone(),
    }
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
            let _ = writeln!(
                out,
                "\n{} {}　{title}",
                "#".repeat(depth + 2),
                number.labeled('章')
            );
            for child in children {
                render_node(out, child, depth + 1);
            }
        }
        BodyNode::Section {
            number,
            title,
            children,
        } => {
            let _ = writeln!(
                out,
                "\n{} {}　{title}",
                "#".repeat(depth + 2),
                number.labeled('節')
            );
            for child in children {
                render_node(out, child, depth + 1);
            }
        }
        BodyNode::Article(article) => render_article(out, article, depth + 2),
    }
}

fn render_article(out: &mut String, article: &Article, level: usize) {
    let heading = "#".repeat(level);
    let number = article.number.labeled('条');
    match &article.title {
        Some(title) => {
            let _ = writeln!(out, "\n{heading} {number}（{title}）");
        }
        None => {
            let _ = writeln!(out, "\n{heading} {number}");
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
        for sub in &item.subitems {
            let _ = writeln!(out, "    {} {}", sub.label, sub.text);
        }
    }
}

fn render_supplementary(out: &mut String, block: &SupplProvision) {
    let _ = writeln!(out, "\n### {}", block.heading_raw);
    if let Some(d) = &block.promulgated {
        let _ = writeln!(out, "\n*公布: {}*", d.iso);
    }
    match &block.effective {
        Effective::Date(d) => {
            let _ = writeln!(out, "\n*施行: {}*", d.iso);
        }
        Effective::OnPromulgation => {
            let _ = writeln!(out, "\n*施行: 公布の日*");
        }
        Effective::Unspecified => {}
    }
    if !block.references_tables.is_empty() {
        let _ = writeln!(out, "\n*関連: {}*", block.references_tables.join("、"));
    }
    if !block.text.is_empty() {
        let _ = writeln!(out, "\n{}", block.text);
    }
}

fn render_appendix(out: &mut String, appendix: &Appendix) {
    let kind = match appendix.kind {
        AppendixKind::Table => "別表",
        AppendixKind::Style => "様式",
    };
    let _ = writeln!(
        out,
        "\n### {}（{}）　[{kind}]",
        appendix.id, appendix.related_article_raw
    );
    match &appendix.cells {
        Some(table) => render_table_grid(out, table),
        None => {
            let _ = writeln!(out, "\n```\n{}\n```", appendix.raw_text);
        }
    }
}

fn render_table_grid(out: &mut String, table: &crate::model::Table) {
    let _ = writeln!(out);
    for (i, row) in table.rows.iter().enumerate() {
        let _ = writeln!(out, "| {} |", row.join(" | "));
        if i + 1 == table.header_row_count {
            let sep: Vec<&str> = row.iter().map(|_| "---").collect();
            let _ = writeln!(out, "| {} |", sep.join(" | "));
        }
    }
}
