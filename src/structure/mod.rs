pub mod line_kind;
pub mod toc;

use crate::model::{
    Article, ArticleNumber, BodyNode, Document, Item, Paragraph, SupplementaryBlock,
};
use crate::numerals::{normalize_prose, normalize_title};
use line_kind::LineKind;

/// Splits page text into trimmed, non-empty lines, discarding page boundaries
/// entirely — a marker that happens to fall at a page break must not be missed
/// by a line-start check (confirmed against 第63条 during the extraction spike).
pub fn flatten_lines(pages: &[String]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|page| page.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(despace_letter_spaced)
        .collect()
}

/// Some PDFs are extracted by `pdf-extract` with a space between every glyph
/// ("第 ２ 条", "最 終 改 正"), which hides markers from the line classifiers.
/// On such lines a lone space is a per-glyph artifact (dropped), while a run of
/// two or more spaces is a genuine marker/body separator (collapsed to one).
/// Well-extracted lines have few single spaces and are returned unchanged, so
/// this never disturbs the documents that already parse correctly.
fn despace_letter_spaced(line: &str) -> String {
    if !is_letter_spaced(line) {
        return line.to_string();
    }
    let mut out = String::new();
    let mut spaces = 0usize;
    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
        } else {
            if spaces >= 2 {
                out.push(' '); // a real separator survives as a single space
            }
            // a lone interior space is dropped as an extraction artifact
            spaces = 0;
            out.push(ch);
        }
    }
    out
}

/// True when a line is dominated by single-space-separated glyphs — the
/// signature of per-character letter spacing rather than ordinary text.
fn is_letter_spaced(line: &str) -> bool {
    let mut interior_singles = 0usize;
    let mut glyphs = 0usize;
    let mut spaces = 0usize;
    let mut seen_glyph = false;
    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
        } else {
            if spaces == 1 && seen_glyph {
                interior_singles += 1;
            }
            spaces = 0;
            glyphs += 1;
            seen_glyph = true;
        }
    }
    interior_singles >= 3 && interior_singles * 3 >= glyphs
}

/// The preamble (title/enacted/last_amended) plus the 目次 and body line ranges.
pub struct Split<'a> {
    pub title: String,
    pub enacted: String,
    pub last_amended: String,
    pub toc_lines: &'a [String],
    pub body_lines: &'a [String],
}

/// True when a line marks the start of the document proper (its body or 目次) —
/// i.e. the preamble metadata has ended. `TitleAnnotation` is deliberately
/// *excluded*: a preamble may contain a "（…）" note, and a genuine article
/// caption is recovered by the caller's back-up rule instead.
fn is_body_boundary(line: &str) -> bool {
    line == "目次"
        || matches!(
            line_kind::classify(line),
            LineKind::Chapter { .. }
                | LineKind::Section { .. }
                | LineKind::ArticleHeading { .. }
                | LineKind::SupplementaryMarker { .. }
                | LineKind::AppendedTableMarker { .. }
        )
}

/// Separates the title-block preamble, the 目次 (table of contents), and the real
/// document body — depending on none of {最終改正 line, 目次, 章} being present, so
/// it generalizes across the whole ruleset corpus (short 章-less regulations, fee
/// rules with no 最終改正, and the chapter+目次 学則 alike).
pub fn split(lines: &[String]) -> Split<'_> {
    // A document with no extractable text (e.g. a scanned PDF) yields no lines;
    // never index blindly.
    let Some(title) = lines.first().cloned() else {
        return Split {
            title: String::new(),
            enacted: String::new(),
            last_amended: String::new(),
            toc_lines: &[],
            body_lines: &[],
        };
    };

    let mut boundary = (1..lines.len())
        .find(|&i| is_body_boundary(&lines[i]))
        .unwrap_or(lines.len());

    // Caption back-up: a chapter-less document opens with "（趣旨）" then "第１条".
    // The boundary lands on the article heading; step back one so the caption
    // starts the body (and becomes the article's title) rather than being buried
    // in the preamble.
    if boundary >= 2
        && matches!(
            line_kind::classify(&lines[boundary]),
            LineKind::ArticleHeading { .. }
        )
        && matches!(
            line_kind::classify(&lines[boundary - 1]),
            LineKind::TitleAnnotation { .. }
        )
    {
        boundary -= 1;
    }

    let preamble = &lines[1..boundary];
    let (enacted, last_amended) = match preamble.iter().position(|l| l.starts_with("最終改正"))
    {
        // The tail from 最終改正 onward also absorbs a "…の全部を改正する。" note
        // that some rules place immediately after the amendment citation.
        Some(k) => (preamble[..k].join(" "), preamble[k..].join(" ")),
        None => (preamble.join(" "), String::new()),
    };

    let after = &lines[boundary..];
    let (toc_lines, body_lines) = if after.first().map(String::as_str) == Some("目次") {
        // The 目次 repeats every chapter heading the body uses, so the real body
        // is identified by "第１章" appearing a *second* time (verified corpus-wide:
        // exactly the documents with a 目次 line contain 第１章 at least twice).
        let mut seen_chapter_one = false;
        let mut body_start = after.len();
        for (i, line) in after.iter().enumerate() {
            if let LineKind::Chapter { number: 1, .. } = line_kind::classify(line) {
                if seen_chapter_one {
                    body_start = i;
                    break;
                }
                seen_chapter_one = true;
            }
        }
        debug_assert!(
            body_start < after.len(),
            "a 目次 was present but 第１章 never appeared twice — the TOC/body split heuristic needs revisiting for this document"
        );
        (&after[..body_start], &after[body_start..])
    } else {
        (&[][..], after)
    };

    Split {
        title,
        enacted,
        last_amended,
        toc_lines,
        body_lines,
    }
}

pub fn parse(pages: &[String]) -> anyhow::Result<Document> {
    let lines = flatten_lines(pages);
    let split = split(&lines);
    let (body, supplementary_provisions, appended_tables) = parse_body(split.body_lines)?;
    Ok(Document {
        title: split.title,
        enacted: split.enacted,
        last_amended: split.last_amended,
        body,
        supplementary_provisions,
        appended_tables,
    })
}

// --- Leaf builders (条/項/号): unchanged from the original 学則 parser. ---

struct ItemB {
    number: u32,
    text: Vec<String>,
}
struct ParagraphB {
    number: u32,
    text: Vec<String>,
    items: Vec<Item>,
    cur_item: Option<ItemB>,
}
struct ArticleB {
    number: ArticleNumber,
    title: Option<String>,
    paragraphs: Vec<Paragraph>,
    cur_paragraph: Option<ParagraphB>,
}

impl ItemB {
    fn finish(self) -> Item {
        Item {
            number: self.number,
            text: normalize_prose(self.text.join("").trim()),
        }
    }
}
impl ParagraphB {
    fn close_item(&mut self) {
        if let Some(item) = self.cur_item.take() {
            self.items.push(item.finish());
        }
    }
    fn finish(mut self) -> Paragraph {
        self.close_item();
        Paragraph {
            number: self.number,
            text: normalize_prose(self.text.join("").trim()),
            items: self.items,
        }
    }
}
impl ArticleB {
    fn close_paragraph(&mut self) {
        if let Some(p) = self.cur_paragraph.take() {
            self.paragraphs.push(p.finish());
        }
    }
    fn finish(mut self) -> Article {
        self.close_paragraph();
        Article {
            number: self.number,
            title: self.title,
            paragraphs: self.paragraphs,
        }
    }
    /// Appends a wrapped-line fragment to whichever unit is currently innermost
    /// open (item > paragraph body), joining without a separator since Japanese
    /// text wraps without spaces.
    fn append(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(p) = self.cur_paragraph.as_mut() {
            if let Some(item) = p.cur_item.as_mut() {
                item.text.push(text.to_string());
            } else {
                p.text.push(text.to_string());
            }
        }
    }
}

// --- Container frame stack: turns the 章/節 nesting into a uniform tree. ---

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerKind {
    Chapter,
    Section,
}

impl ContainerKind {
    /// Nesting depth: a new container closes every open frame at its rank or deeper.
    fn rank(self) -> u8 {
        match self {
            ContainerKind::Chapter => 1,
            ContainerKind::Section => 2,
        }
    }
}

/// One level of open structure. The bottom frame (`header == None`) is the
/// document root; its `children` become `Document.body`.
struct Frame {
    header: Option<(ContainerKind, u32, String)>,
    children: Vec<BodyNode>,
    cur_article: Option<ArticleB>,
}

impl Frame {
    fn rank(&self) -> u8 {
        self.header.as_ref().map_or(0, |(k, _, _)| k.rank())
    }
    fn close_article(&mut self) {
        if let Some(a) = self.cur_article.take() {
            self.children.push(BodyNode::Article(a.finish()));
        }
    }
    fn into_node(mut self) -> BodyNode {
        self.close_article();
        let (kind, number, title) = self.header.expect("root frame is never turned into a node");
        match kind {
            ContainerKind::Chapter => BodyNode::Chapter {
                number,
                title,
                children: self.children,
            },
            ContainerKind::Section => BodyNode::Section {
                number,
                title,
                children: self.children,
            },
        }
    }
}

/// Accumulates the main-provision tree while walking the body lines.
struct TreeBuilder {
    stack: Vec<Frame>,
    pending_title: Option<String>,
}

impl TreeBuilder {
    fn new() -> Self {
        Self {
            stack: vec![Frame {
                header: None,
                children: Vec::new(),
                cur_article: None,
            }],
            pending_title: None,
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("root frame is always present")
    }

    fn cur_article(&mut self) -> Option<&mut ArticleB> {
        self.top().cur_article.as_mut()
    }

    /// Finalizes the innermost frame into its parent's children.
    fn pop_frame(&mut self) {
        let frame = self.stack.pop().expect("never pops the root frame");
        let node = frame.into_node();
        self.top().children.push(node);
    }

    fn open_container(&mut self, kind: ContainerKind, number: u32, title: String) {
        self.top().close_article();
        let r = kind.rank();
        while self.stack.len() > 1 && self.top().rank() >= r {
            self.pop_frame();
        }
        self.stack.push(Frame {
            header: Some((kind, number, title)),
            children: Vec::new(),
            cur_article: None,
        });
    }

    fn open_article(&mut self, number: ArticleNumber, title: Option<String>, rest: &str) {
        self.top().close_article();
        let mut article = ArticleB {
            number,
            title,
            paragraphs: Vec::new(),
            cur_paragraph: Some(ParagraphB {
                number: 1,
                text: Vec::new(),
                items: Vec::new(),
                cur_item: None,
            }),
        };
        article.append(rest);
        self.top().cur_article = Some(article);
    }

    fn open_paragraph(&mut self, number: u32, rest: &str) {
        match self.cur_article() {
            Some(article) => {
                article.close_paragraph();
                article.cur_paragraph = Some(ParagraphB {
                    number,
                    text: Vec::new(),
                    items: Vec::new(),
                    cur_item: None,
                });
                article.append(rest);
            }
            // A 項 marker with no open 条 is an orphan (e.g. a numbered line inside
            // an amendment block that fell into the body): keep its text rather
            // than panicking or dropping the whole file.
            None => self.append_continuation(rest),
        }
    }

    fn open_item(&mut self, number: u32, rest: &str) {
        match self.cur_article() {
            Some(article) if article.cur_paragraph.is_some() => {
                let p = article.cur_paragraph.as_mut().unwrap();
                p.close_item();
                p.cur_item = Some(ItemB {
                    number,
                    text: Vec::new(),
                });
                article.append(rest);
            }
            // Orphan 号 marker with no open 条/項: degrade to continuation text.
            _ => self.append_continuation(rest),
        }
    }

    fn append_continuation(&mut self, text: &str) {
        if let Some(article) = self.cur_article() {
            article.append(text);
        }
        // Continuation with no open article (before the first 条) is dropped,
        // matching the original parser's behavior.
    }

    /// Drains every open frame into the root and returns the finished body tree.
    fn finish(mut self) -> Vec<BodyNode> {
        while self.stack.len() > 1 {
            self.pop_frame();
        }
        self.top().close_article();
        self.stack.pop().expect("root frame").children
    }
}

// --- Raw blocks (附則/別表): unchanged accumulation, separate from the tree. ---

enum RawKind {
    Supplementary,
    Table,
}
struct RawB {
    kind: RawKind,
    heading_raw: String,
    related_raw: String,
    text: Vec<String>,
}

#[allow(clippy::type_complexity)]
fn parse_body(
    lines: &[String],
) -> anyhow::Result<(
    Vec<BodyNode>,
    Vec<SupplementaryBlock>,
    Vec<crate::model::AppendedTable>,
)> {
    let mut supplementary: Vec<SupplementaryBlock> = Vec::new();
    let mut appended_tables: Vec<crate::model::AppendedTable> = Vec::new();
    let mut cur_raw: Option<RawB> = None;

    let flush_raw = |cur_raw: &mut Option<RawB>,
                     supplementary: &mut Vec<SupplementaryBlock>,
                     appended_tables: &mut Vec<crate::model::AppendedTable>| {
        if let Some(r) = cur_raw.take() {
            let text = r.text.join("\n").trim().to_string();
            match r.kind {
                RawKind::Supplementary => {
                    let references_tables = find_table_refs(&text);
                    supplementary.push(SupplementaryBlock {
                        ordinal: supplementary.len() as u32 + 1,
                        heading_raw: r.heading_raw,
                        raw_text: text,
                        references_tables,
                    });
                }
                RawKind::Table => {
                    appended_tables.push(crate::model::AppendedTable {
                        id: r.heading_raw,
                        related_article_raw: r.related_raw,
                        raw_text: text,
                    });
                }
            }
        }
    };

    // The tree is finalized into `body` on the first 附則/別表 marker (everything
    // after it is raw), or at end-of-input if no such marker appears.
    let mut body: Option<Vec<BodyNode>> = None;
    let take_tree = |tree: &mut Option<TreeBuilder>, body: &mut Option<Vec<BodyNode>>| {
        if let Some(t) = tree.take() {
            *body = Some(t.finish());
        }
    };
    let mut tree = Some(TreeBuilder::new());

    for line in lines {
        let kind = line_kind::classify(line);

        // Once we've entered 附則/別表 raw-accumulation mode, only a new
        // Supplementary/AppendedTable marker can end the current block — every
        // other line, however it would otherwise classify, is raw text.
        if let Some(raw) = &mut cur_raw {
            match &kind {
                LineKind::SupplementaryMarker { heading_raw } => {
                    flush_raw(&mut cur_raw, &mut supplementary, &mut appended_tables);
                    cur_raw = Some(RawB {
                        kind: RawKind::Supplementary,
                        heading_raw: heading_raw.to_string(),
                        related_raw: String::new(),
                        text: Vec::new(),
                    });
                }
                LineKind::AppendedTableMarker {
                    id_raw,
                    related_raw,
                } => {
                    flush_raw(&mut cur_raw, &mut supplementary, &mut appended_tables);
                    cur_raw = Some(RawB {
                        kind: RawKind::Table,
                        heading_raw: id_raw.to_string(),
                        related_raw: related_raw.to_string(),
                        text: Vec::new(),
                    });
                }
                _ => raw.text.push(line.clone()),
            }
            continue;
        }

        // Structural lines feed the main-provision tree; a 附則/別表 marker instead
        // finalizes the tree and switches to raw accumulation. `builder` is
        // borrowed only inside the tree-feeding arms so those arms don't clash
        // with `take_tree`'s borrow of `tree`.
        macro_rules! builder {
            () => {
                tree.as_mut()
                    .expect("tree is present until the first 附則/別表")
            };
        }
        match kind {
            LineKind::Chapter { number, title_raw } => {
                builder!().open_container(
                    ContainerKind::Chapter,
                    number,
                    normalize_title(title_raw),
                );
            }
            LineKind::Section { number, title_raw } => {
                builder!().open_container(
                    ContainerKind::Section,
                    number,
                    normalize_title(title_raw),
                );
            }
            LineKind::TitleAnnotation { title_raw } => {
                builder!().pending_title = Some(title_raw.to_string());
            }
            LineKind::ArticleHeading {
                number,
                branch,
                rest,
            } => {
                let builder = builder!();
                let title = builder.pending_title.take();
                builder.open_article(
                    ArticleNumber {
                        article: number,
                        branch,
                    },
                    title,
                    rest,
                );
            }
            LineKind::ParagraphMarker { number, rest } => {
                builder!().open_paragraph(number, rest);
            }
            LineKind::ItemMarker { number, rest } => {
                builder!().open_item(number, rest);
            }
            LineKind::Continuation(text) => {
                builder!().append_continuation(text);
            }
            LineKind::SupplementaryMarker { heading_raw } => {
                take_tree(&mut tree, &mut body);
                cur_raw = Some(RawB {
                    kind: RawKind::Supplementary,
                    heading_raw: heading_raw.to_string(),
                    related_raw: String::new(),
                    text: Vec::new(),
                });
            }
            LineKind::AppendedTableMarker {
                id_raw,
                related_raw,
            } => {
                take_tree(&mut tree, &mut body);
                cur_raw = Some(RawB {
                    kind: RawKind::Table,
                    heading_raw: id_raw.to_string(),
                    related_raw: related_raw.to_string(),
                    text: Vec::new(),
                });
            }
        }
    }

    take_tree(&mut tree, &mut body);
    flush_raw(&mut cur_raw, &mut supplementary, &mut appended_tables);

    Ok((body.unwrap_or_default(), supplementary, appended_tables))
}

fn find_table_refs(text: &str) -> Vec<String> {
    use std::sync::LazyLock;
    static TABLE_REF: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"別表第[0-9０-９]+").unwrap());
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for m in TABLE_REF.find_iter(text) {
        let s = m.as_str().to_string();
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn chapter(node: &BodyNode) -> (u32, &str, &[BodyNode]) {
        match node {
            BodyNode::Chapter {
                number,
                title,
                children,
            } => (*number, title.as_str(), children.as_slice()),
            _ => panic!("expected a Chapter node"),
        }
    }

    fn article(node: &BodyNode) -> &Article {
        match node {
            BodyNode::Article(a) => a,
            _ => panic!("expected an Article node"),
        }
    }

    #[test]
    fn article_with_title_paragraphs_and_items() {
        let (body, _, _) = parse_body(&lines(&[
            "第１章  総  則",
            "（目的）",
            "第１条  高知大学の学部においては、次の理念を掲げる。",
            "(1)  広範な教養と高度な専門知識",
            "を培い、人材を育成する。",
            "(2)  諸科学の基礎と応用",
            "２  本学大学院においては、教授研究する。",
        ]))
        .unwrap();

        assert_eq!(body.len(), 1);
        let (number, title, children) = chapter(&body[0]);
        assert_eq!(number, 1);
        assert_eq!(title, "総則");
        assert_eq!(children.len(), 1);

        let a = article(&children[0]);
        assert_eq!(
            a.number,
            ArticleNumber {
                article: 1,
                branch: None
            }
        );
        assert_eq!(a.title.as_deref(), Some("目的"));
        assert_eq!(a.paragraphs.len(), 2);

        let p1 = &a.paragraphs[0];
        assert_eq!(p1.number, 1);
        assert_eq!(p1.text, "高知大学の学部においては、次の理念を掲げる。");
        assert_eq!(p1.items.len(), 2);
        assert_eq!(
            p1.items[0].text,
            "広範な教養と高度な専門知識を培い、人材を育成する。"
        );
        assert_eq!(p1.items[1].text, "諸科学の基礎と応用");

        let p2 = &a.paragraphs[1];
        assert_eq!(p2.number, 2);
        assert_eq!(p2.text, "本学大学院においては、教授研究する。");
    }

    #[test]
    fn chapter_less_document_puts_articles_at_the_root() {
        // A short regulation with no 章/節 and no 目次: the articles hang directly
        // off the body root, with no synthetic chapter.
        let (body, _, _) = parse_body(&lines(&[
            "（趣旨）",
            "第１条  この規則は、必要な事項を定める。",
            "（対象）",
            "第２条  対象は、次のとおりとする。",
        ]))
        .unwrap();

        assert_eq!(body.len(), 2);
        let a1 = article(&body[0]);
        assert_eq!(a1.number.article, 1);
        assert_eq!(a1.title.as_deref(), Some("趣旨"));
        let a2 = article(&body[1]);
        assert_eq!(a2.number.article, 2);
        assert_eq!(a2.title.as_deref(), Some("対象"));
    }

    #[test]
    fn nested_section_under_chapter() {
        let (body, _, _) = parse_body(&lines(&[
            "第２章  通則",
            "第１節  学年",
            "第２条  学年は、４月１日に始まる。",
        ]))
        .unwrap();

        let (_, _, ch_children) = chapter(&body[0]);
        assert_eq!(ch_children.len(), 1);
        match &ch_children[0] {
            BodyNode::Section {
                number,
                title,
                children,
            } => {
                assert_eq!(*number, 1);
                assert_eq!(title, "学年");
                assert_eq!(article(&children[0]).number.article, 2);
            }
            _ => panic!("expected a Section node"),
        }
    }

    #[test]
    fn branch_article_is_a_sibling_with_a_distinct_number() {
        let (body, _, _) = parse_body(&lines(&[
            "第４章  大学院",
            "第58条  専門職学位課程に入学することのできる者は、次のとおりとする。",
            "第58条の２ 専門職学位課程に入学することのできる者は、前条による。",
        ]))
        .unwrap();

        let (_, _, children) = chapter(&body[0]);
        assert_eq!(children.len(), 2);
        assert_eq!(
            article(&children[0]).number,
            ArticleNumber {
                article: 58,
                branch: None
            }
        );
        assert_eq!(
            article(&children[1]).number,
            ArticleNumber {
                article: 58,
                branch: Some(2)
            }
        );
    }

    #[test]
    fn orphan_paragraph_and_item_markers_downgrade_without_panicking() {
        // A 項/号 marker with no open 条 must not panic; its text degrades to
        // continuation (dropped here, since nothing is open before the article).
        let (body, _, _) = parse_body(&lines(&[
            "２  宙に浮いた項マーカー。",
            "(1)  宙に浮いた号マーカー。",
            "第１条  実際の条文。",
        ]))
        .unwrap();

        assert_eq!(body.len(), 1);
        assert_eq!(article(&body[0]).number.article, 1);
    }

    #[test]
    fn supplementary_and_table_blocks_stay_raw_and_ordered() {
        let (body, supplementary, tables) = parse_body(&lines(&[
            "第８章  雑則",
            "第85条  雑則本文。",
            "附則",
            "この規則は、施行日から施行する。",
            "附  則（令和８年１月28日規則第58号）",
            "１  改正後の別表第１に定める定員を加える。",
            "別表第１（第５条関係）",
            "学部  収容定員",
            "医学部  660",
            "別表第２（第56条関係）",
            "研究科  収容定員",
        ]))
        .unwrap();

        let (_, _, children) = chapter(&body[0]);
        assert_eq!(children.len(), 1);

        assert_eq!(supplementary.len(), 2);
        assert_eq!(supplementary[0].ordinal, 1);
        assert_eq!(supplementary[0].heading_raw, "附則");
        assert_eq!(
            supplementary[0].raw_text,
            "この規則は、施行日から施行する。"
        );
        assert_eq!(supplementary[0].references_tables, Vec::<String>::new());

        assert_eq!(supplementary[1].ordinal, 2);
        assert_eq!(
            supplementary[1].heading_raw,
            "附  則（令和８年１月28日規則第58号）"
        );
        assert_eq!(
            supplementary[1].references_tables,
            vec!["別表第１".to_string()]
        );

        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].id, "別表第１");
        assert_eq!(tables[0].related_article_raw, "第５条関係");
        assert_eq!(tables[0].raw_text, "学部  収容定員\n医学部  660");
        assert_eq!(tables[1].id, "別表第２");
        assert_eq!(tables[1].related_article_raw, "第56条関係");
    }

    #[test]
    fn split_excludes_toc_from_the_body() {
        let all_lines = lines(&[
            "高知大学学則",
            "平成20年3月26日",
            "最終改正 令和8年1月28日規則第58号",
            "目次",
            "第１章  総則（第１条）",
            "第１章  総  則",
            "（目的）",
            "第１条  本文。",
        ]);
        let split = split(&all_lines);
        assert_eq!(split.toc_lines, &["目次", "第１章  総則（第１条）"]);
        assert_eq!(split.body_lines[0], "第１章  総  則");
        assert_eq!(split.enacted, "平成20年3月26日");
        assert_eq!(split.last_amended, "最終改正 令和8年1月28日規則第58号");

        let doc = parse(&all_lines).unwrap();
        assert_eq!(doc.chapters().count(), 1);
    }

    #[test]
    fn split_without_last_amended_keeps_the_rule_number_in_enacted() {
        // A fee rule: title / 制定日 / 規則番号 / （趣旨）第１条 …, no 最終改正 line.
        let all_lines = lines(&[
            "高知大学国際交流会館料金規則",
            "平成16年４月１日",
            "規 則 第 152 号",
            "（趣旨）",
            "第１条  この規則は、必要な事項を定める。",
        ]);
        let split = split(&all_lines);
        assert_eq!(split.enacted, "平成16年４月１日 規 則 第 152 号");
        assert_eq!(split.last_amended, "");
        // The caption back-up rule keeps （趣旨） in the body as the article title.
        assert_eq!(split.body_lines[0], "（趣旨）");
        assert!(split.toc_lines.is_empty());
    }

    #[test]
    fn split_of_empty_input_does_not_panic() {
        let split = split(&[]);
        assert_eq!(split.title, "");
        assert!(split.body_lines.is_empty());
    }
}
