pub mod line_kind;
pub mod toc;

use crate::dates;
use crate::model::{
    Appendix, AppendixKind, Article, BodyNode, BranchedNumber, Document, Item, Paragraph, Subitem,
    SupplProvision,
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

/// The preamble segments (each raw, structured later in `parse`), plus the 目次
/// and body line ranges.
pub struct Split<'a> {
    pub title: String,
    pub enacted_raw: String,
    pub last_amended_raw: String,
    /// A "…の全部を改正する。" note, separated from `last_amended_raw` (S8).
    pub full_amendment_raw: String,
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
                | LineKind::AppendixMarker { .. }
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
            enacted_raw: String::new(),
            last_amended_raw: String::new(),
            full_amendment_raw: String::new(),
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

    // Partition the preamble by three independent conditions so a "全部改正" note
    // never folds into 最終改正 (S8), and a document with the note but no 最終改正
    // line still separates them correctly.
    let mut enacted = Vec::new();
    let mut last_amended = Vec::new();
    let mut full_amendment = Vec::new();
    let mut seen_amended = false;
    for line in &lines[1..boundary] {
        if line.contains("の全部を改正する") {
            full_amendment.push(line.as_str());
        } else if line.starts_with("最終改正") || seen_amended {
            seen_amended = true;
            last_amended.push(line.as_str());
        } else {
            enacted.push(line.as_str());
        }
    }

    let after = &lines[boundary..];
    let (toc_lines, body_lines) = if after.first().map(String::as_str) == Some("目次") {
        // The 目次 repeats every chapter heading the body uses, so the real body
        // is identified by "第１章" appearing a *second* time (verified corpus-wide:
        // exactly the documents with a 目次 line contain 第１章 at least twice).
        let mut seen_chapter_one = false;
        let mut body_start = after.len();
        for (i, line) in after.iter().enumerate() {
            if let LineKind::Chapter {
                number: 1,
                branch: None,
                ..
            } = line_kind::classify(line)
            {
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
        enacted_raw: enacted.join(" "),
        last_amended_raw: last_amended.join(" "),
        full_amendment_raw: full_amendment.join(" "),
        toc_lines,
        body_lines,
    }
}

pub fn parse(pages: &[String]) -> anyhow::Result<Document> {
    let lines = flatten_lines(pages);
    let split = split(&lines);
    let (body, supplementary_provisions, appendices) = parse_body(split.body_lines)?;
    let full = split.full_amendment_raw.trim();
    Ok(Document {
        title: split.title,
        enacted: dates::parse_enactment(&split.enacted_raw),
        last_amended: dates::parse_amendment(&split.last_amended_raw),
        full_amendment_note: (!full.is_empty()).then(|| full.to_string()),
        body,
        supplementary_provisions,
        appendices,
        authorities: Vec::new(),
    })
}

// --- Leaf builders (条/項/号/イロハ). ---

struct SubitemB {
    label: String,
    text: Vec<String>,
}
struct ItemB {
    number: u32,
    text: Vec<String>,
    subitems: Vec<Subitem>,
    cur_subitem: Option<SubitemB>,
}
struct ParagraphB {
    number: u32,
    text: Vec<String>,
    items: Vec<Item>,
    cur_item: Option<ItemB>,
}
struct ArticleB {
    number: BranchedNumber,
    title: Option<String>,
    paragraphs: Vec<Paragraph>,
    cur_paragraph: Option<ParagraphB>,
}

impl SubitemB {
    fn finish(self) -> Subitem {
        Subitem {
            label: self.label,
            text: normalize_prose(self.text.join("").trim()),
            refs: Vec::new(),
        }
    }
}
impl ItemB {
    fn close_subitem(&mut self) {
        if let Some(s) = self.cur_subitem.take() {
            self.subitems.push(s.finish());
        }
    }
    fn finish(mut self) -> Item {
        self.close_subitem();
        Item {
            number: self.number,
            text: normalize_prose(self.text.join("").trim()),
            subitems: self.subitems,
            refs: Vec::new(),
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
            refs: Vec::new(),
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
            subordinate_rules: Vec::new(),
        }
    }
    /// Appends a wrapped-line fragment to whichever unit is currently innermost
    /// open (subitem > item > paragraph body), joining without a separator since
    /// Japanese text wraps without spaces.
    fn append(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(p) = self.cur_paragraph.as_mut() {
            if let Some(item) = p.cur_item.as_mut() {
                if let Some(sub) = item.cur_subitem.as_mut() {
                    sub.text.push(text.to_string());
                } else {
                    item.text.push(text.to_string());
                }
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
    header: Option<(ContainerKind, BranchedNumber, String)>,
    children: Vec<BodyNode>,
    cur_article: Option<ArticleB>,
    /// True from the moment this container opened until its first article /
    /// caption / paragraph — while true, continuation lines extend the heading
    /// title (recovers 2-line-wrapped 章/節 headings, S2).
    title_open: bool,
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
                title_open: false,
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

    fn open_container(&mut self, kind: ContainerKind, number: BranchedNumber, title: String) {
        self.top().close_article();
        let r = kind.rank();
        while self.stack.len() > 1 && self.top().rank() >= r {
            self.pop_frame();
        }
        self.stack.push(Frame {
            header: Some((kind, number, title)),
            children: Vec::new(),
            cur_article: None,
            title_open: true,
        });
    }

    fn set_pending_title(&mut self, title: String) {
        // A "（…）" line is a caption only when an article heading follows it. If
        // one was pending and something else arrives, this new "（…）" proves the
        // old one was inline body text, not a caption — flush it first.
        self.flush_pending_title();
        self.top().title_open = false;
        self.pending_title = Some(title);
    }

    /// A pending "（…）" that turned out not to precede an article is inline body
    /// text (a parenthetical clause on its own line); append it verbatim so it is
    /// not lost and does not wrongly become a later article's title.
    fn flush_pending_title(&mut self) {
        if let Some(t) = self.pending_title.take() {
            self.append_continuation(&format!("（{t}）"));
        }
    }

    fn open_article(&mut self, number: BranchedNumber, title: Option<String>, rest: &str) {
        let frame = self.top();
        frame.title_open = false;
        frame.close_article();
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
                    subitems: Vec::new(),
                    cur_subitem: None,
                });
                article.append(rest);
            }
            // Orphan 号 marker with no open 条/項: degrade to continuation text.
            _ => self.append_continuation(rest),
        }
    }

    fn open_subitem(&mut self, label: &str, rest: &str) {
        // A 号 sub-item (イ/ロ/ハ) requires an open 号; otherwise it is an orphan
        // and degrades to continuation text.
        let has_item = self
            .cur_article()
            .and_then(|a| a.cur_paragraph.as_ref())
            .is_some_and(|p| p.cur_item.is_some());
        if !has_item {
            self.append_continuation(rest);
            return;
        }
        let item = self
            .cur_article()
            .unwrap()
            .cur_paragraph
            .as_mut()
            .unwrap()
            .cur_item
            .as_mut()
            .unwrap();
        item.close_subitem();
        item.cur_subitem = Some(SubitemB {
            label: label.to_string(),
            text: Vec::new(),
        });
        self.cur_article().unwrap().append(rest);
    }

    fn append_continuation(&mut self, text: &str) {
        if self.cur_article().is_some() {
            self.cur_article().unwrap().append(text);
            return;
        }
        // No open article: if the current container's heading is still open,
        // this is a wrapped heading line — extend the title (S2). A line starting
        // with "（" is instead the start of an article caption, which ends the
        // heading (freeze; a lone wrapped caption before the first 条 is dropped,
        // as in the original parser). Otherwise drop.
        let pending_none = self.pending_title.is_none();
        let top = self.top();
        if top.title_open && pending_none {
            if text.starts_with('（') || text.starts_with('(') {
                top.title_open = false;
            } else if let Some((_, _, title)) = top.header.as_mut() {
                *title = normalize_title(&format!("{title}{text}"));
            }
        }
    }

    /// Drains every open frame into the root and returns the finished body tree.
    fn finish(mut self) -> Vec<BodyNode> {
        self.flush_pending_title();
        while self.stack.len() > 1 {
            self.pop_frame();
        }
        self.top().close_article();
        self.stack.pop().expect("root frame").children
    }
}

// --- Raw blocks (附則/別表/様式): accumulated then structured on flush. ---

enum RawKind {
    Supplementary,
    Appendix(AppendixKind),
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
) -> anyhow::Result<(Vec<BodyNode>, Vec<SupplProvision>, Vec<Appendix>)> {
    let mut supplementary: Vec<SupplProvision> = Vec::new();
    let mut appendices: Vec<Appendix> = Vec::new();
    let mut cur_raw: Option<RawB> = None;

    let flush_raw = |cur_raw: &mut Option<RawB>,
                     supplementary: &mut Vec<SupplProvision>,
                     appendices: &mut Vec<Appendix>| {
        if let Some(r) = cur_raw.take() {
            let text = r.text.join("\n").trim().to_string();
            match r.kind {
                RawKind::Supplementary => {
                    supplementary.push(SupplProvision {
                        ordinal: supplementary.len() as u32 + 1,
                        amendment: dates::parse_rule_number(&r.heading_raw),
                        promulgated: dates::parse_era_date(&r.heading_raw),
                        effective: dates::parse_effective(&text),
                        references_tables: find_table_refs(&text),
                        heading_raw: r.heading_raw,
                        text,
                    });
                }
                RawKind::Appendix(kind) => {
                    appendices.push(Appendix {
                        kind,
                        id: normalize_title(&r.heading_raw),
                        related_article_raw: r.related_raw,
                        raw_text: text,
                        cells: None,
                    });
                }
            }
        }
    };

    // The tree is finalized into `body` on the first 附則/別表/様式 marker
    // (everything after it is raw), or at end-of-input if no such marker appears.
    let mut body: Option<Vec<BodyNode>> = None;
    let take_tree = |tree: &mut Option<TreeBuilder>, body: &mut Option<Vec<BodyNode>>| {
        if let Some(t) = tree.take() {
            *body = Some(t.finish());
        }
    };
    let mut tree = Some(TreeBuilder::new());

    for line in lines {
        let kind = line_kind::classify(line);

        // Once we've entered raw-accumulation mode, only a new Supplementary /
        // Appendix marker can end the current block — every other line, however
        // it would otherwise classify, is raw text.
        if let Some(raw) = &mut cur_raw {
            match &kind {
                LineKind::SupplementaryMarker { heading_raw } => {
                    flush_raw(&mut cur_raw, &mut supplementary, &mut appendices);
                    cur_raw = Some(RawB {
                        kind: RawKind::Supplementary,
                        heading_raw: heading_raw.to_string(),
                        related_raw: String::new(),
                        text: Vec::new(),
                    });
                }
                LineKind::AppendixMarker {
                    kind,
                    id_raw,
                    related_raw,
                } => {
                    flush_raw(&mut cur_raw, &mut supplementary, &mut appendices);
                    cur_raw = Some(RawB {
                        kind: RawKind::Appendix(*kind),
                        heading_raw: id_raw.to_string(),
                        related_raw: related_raw.to_string(),
                        text: Vec::new(),
                    });
                }
                _ => raw.text.push(line.clone()),
            }
            continue;
        }

        // Structural lines feed the main-provision tree; a 附則/別表/様式 marker
        // instead finalizes the tree and switches to raw accumulation. `builder`
        // is borrowed only inside the tree-feeding arms so those arms don't clash
        // with `take_tree`'s borrow of `tree`.
        macro_rules! builder {
            () => {
                tree.as_mut()
                    .expect("tree is present until the first 附則/別表/様式")
            };
        }
        match kind {
            LineKind::Chapter {
                number,
                branch,
                title_raw,
            } => {
                let builder = builder!();
                builder.flush_pending_title();
                builder.open_container(
                    ContainerKind::Chapter,
                    BranchedNumber {
                        main: number,
                        branch,
                    },
                    normalize_title(title_raw),
                );
            }
            LineKind::Section {
                number,
                branch,
                title_raw,
            } => {
                let builder = builder!();
                builder.flush_pending_title();
                builder.open_container(
                    ContainerKind::Section,
                    BranchedNumber {
                        main: number,
                        branch,
                    },
                    normalize_title(title_raw),
                );
            }
            LineKind::TitleAnnotation { title_raw } => {
                builder!().set_pending_title(title_raw.to_string());
            }
            LineKind::ArticleHeading {
                number,
                branch,
                rest,
            } => {
                let builder = builder!();
                let title = builder.pending_title.take();
                builder.open_article(
                    BranchedNumber {
                        main: number,
                        branch,
                    },
                    title,
                    rest,
                );
            }
            LineKind::ParagraphMarker { number, rest } => {
                let builder = builder!();
                builder.flush_pending_title();
                builder.open_paragraph(number, rest);
            }
            LineKind::ItemMarker { number, rest } => {
                let builder = builder!();
                builder.flush_pending_title();
                builder.open_item(number, rest);
            }
            LineKind::SubItemMarker { label, rest } => {
                let builder = builder!();
                builder.flush_pending_title();
                builder.open_subitem(label, rest);
            }
            LineKind::Continuation(text) => {
                let builder = builder!();
                builder.flush_pending_title();
                builder.append_continuation(text);
            }
            LineKind::SupplementaryMarker { heading_raw } => {
                builder!().flush_pending_title();
                take_tree(&mut tree, &mut body);
                cur_raw = Some(RawB {
                    kind: RawKind::Supplementary,
                    heading_raw: heading_raw.to_string(),
                    related_raw: String::new(),
                    text: Vec::new(),
                });
            }
            LineKind::AppendixMarker {
                kind,
                id_raw,
                related_raw,
            } => {
                builder!().flush_pending_title();
                take_tree(&mut tree, &mut body);
                cur_raw = Some(RawB {
                    kind: RawKind::Appendix(kind),
                    heading_raw: id_raw.to_string(),
                    related_raw: related_raw.to_string(),
                    text: Vec::new(),
                });
            }
        }
    }

    take_tree(&mut tree, &mut body);
    flush_raw(&mut cur_raw, &mut supplementary, &mut appendices);

    Ok((body.unwrap_or_default(), supplementary, appendices))
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
    use crate::dates::Effective;
    use crate::model::BodyNode;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    fn chapter(node: &BodyNode) -> (BranchedNumber, &str, &[BodyNode]) {
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

    fn main_no(n: u32) -> BranchedNumber {
        BranchedNumber {
            main: n,
            branch: None,
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
        assert_eq!(number, main_no(1));
        assert_eq!(title, "総則");
        assert_eq!(children.len(), 1);

        let a = article(&children[0]);
        assert_eq!(a.number, main_no(1));
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
        let (body, _, _) = parse_body(&lines(&[
            "（趣旨）",
            "第１条  この規則は、必要な事項を定める。",
            "（対象）",
            "第２条  対象は、次のとおりとする。",
        ]))
        .unwrap();

        assert_eq!(body.len(), 2);
        assert_eq!(article(&body[0]).number, main_no(1));
        assert_eq!(article(&body[0]).title.as_deref(), Some("趣旨"));
        assert_eq!(article(&body[1]).number, main_no(2));
    }

    #[test]
    fn section_branch_is_kept_distinct(/* S3 */) {
        let (body, _, _) = parse_body(&lines(&[
            "第２章  免除",
            "第１節  経済的理由による免除",
            "第１条  本文一。",
            "第１節の２  削除",
            "第１節の３  大学等における修学の支援",
            "第２条  本文二。",
        ]))
        .unwrap();

        let (_, _, ch_children) = chapter(&body[0]);
        // Three distinct 節 nodes: 第1節 / 第1節の2 / 第1節の3.
        let sections: Vec<&BodyNode> = ch_children
            .iter()
            .filter(|n| matches!(n, BodyNode::Section { .. }))
            .collect();
        assert_eq!(sections.len(), 3);
        match sections[1] {
            BodyNode::Section { number, title, .. } => {
                assert_eq!(
                    *number,
                    BranchedNumber {
                        main: 1,
                        branch: Some(2)
                    }
                );
                assert_eq!(title, "削除");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn wrapped_section_heading_is_recovered(/* S2 */) {
        // The 2nd line of a wrapped 節 heading must extend the title, not vanish.
        let (body, _, _) = parse_body(&lines(&[
            "第２章  免除",
            "第１節の３  大学等における修学の支援に関する法律に基づく修学の支援を",
            "受ける者に対する授業料の減免",
            "第２条  本文。",
        ]))
        .unwrap();
        let (_, _, ch_children) = chapter(&body[0]);
        match &ch_children[0] {
            BodyNode::Section { title, .. } => assert_eq!(
                title,
                "大学等における修学の支援に関する法律に基づく修学の支援を受ける者に対する授業料の減免"
            ),
            _ => panic!("expected a Section"),
        }
    }

    #[test]
    fn subitems_nest_under_their_item(/* S7 */) {
        let (body, _, _) = parse_body(&lines(&[
            "（休館日）",
            "第10条  休館日は、次のとおりとする。",
            "(1)  中央館",
            "イ  国民の祝日に関する法律に規定する休日",
            "ロ  12月28日から翌年の１月４日まで",
            "ハ  その他館長が特に必要と認めた日",
        ]))
        .unwrap();

        let a = article(&body[0]);
        let item = &a.paragraphs[0].items[0];
        assert_eq!(item.text, "中央館");
        assert_eq!(item.subitems.len(), 3);
        assert_eq!(item.subitems[0].label, "イ");
        assert_eq!(
            item.subitems[0].text,
            "国民の祝日に関する法律に規定する休日"
        );
        assert_eq!(item.subitems[2].label, "ハ");
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
        assert_eq!(article(&children[0]).number, main_no(58));
        assert_eq!(
            article(&children[1]).number,
            BranchedNumber {
                main: 58,
                branch: Some(2)
            }
        );
    }

    #[test]
    fn appendix_tables_and_styles_are_separated(/* S1 + S4 */) {
        let (_, supplementary, appendices) = parse_body(&lines(&[
            "第85条  本文。",
            "附則",
            "この規則は、令和７年４月１日から施行する。",
            "別表（第16条関係）",
            "学位  専攻分野",
            "様式第１（第18条関係）",
            "学位記のレイアウト",
        ]))
        .unwrap();

        // The 附則 body no longer swallows the 別表/様式.
        assert_eq!(supplementary.len(), 1);
        assert_eq!(
            supplementary[0].text,
            "この規則は、令和７年４月１日から施行する。"
        );
        assert!(matches!(supplementary[0].effective, Effective::Date(_)));

        assert_eq!(appendices.len(), 2);
        assert_eq!(appendices[0].kind, AppendixKind::Table);
        assert_eq!(appendices[0].id, "別表");
        assert_eq!(appendices[0].related_article_raw, "第16条関係");
        assert_eq!(appendices[0].raw_text, "学位  専攻分野");
        assert_eq!(appendices[1].kind, AppendixKind::Style);
        assert_eq!(appendices[1].id, "様式第１");
    }

    #[test]
    fn founding_and_amendment_supplementary_blocks(/* structured 附則 */) {
        let (_, supplementary, _) = parse_body(&lines(&[
            "第85条  本文。",
            "附則",
            "この規則は、平成16年４月１日から施行する。",
            "附  則（令和７年３月25日規則第93号）",
            "この規則は、令和７年４月１日から施行する。",
        ]))
        .unwrap();

        assert_eq!(supplementary.len(), 2);
        assert!(supplementary[0].amendment.is_none()); // founding
        assert!(supplementary[0].promulgated.is_none());
        let amend = supplementary[1].amendment.as_ref().unwrap();
        assert_eq!(amend.number, 93);
        assert_eq!(
            supplementary[1].promulgated.as_ref().unwrap().iso,
            "2025-03-25"
        );
    }

    #[test]
    fn split_partitions_preamble_and_separates_full_amendment(/* S8 */) {
        let all_lines = lines(&[
            "高知大学学生交流規則",
            "平成25年３月27日",
            "規則第109号",
            "最終改正  令和８年２月13日規則第74号",
            "高知大学学生交流規則（平成16年規則第139号）の全部を改正する。",
            "（趣旨）",
            "第１条  本文。",
        ]);
        let split = split(&all_lines);
        assert_eq!(split.enacted_raw, "平成25年３月27日 規則第109号");
        assert_eq!(
            split.last_amended_raw,
            "最終改正  令和８年２月13日規則第74号"
        );
        assert_eq!(
            split.full_amendment_raw,
            "高知大学学生交流規則（平成16年規則第139号）の全部を改正する。"
        );
    }

    #[test]
    fn split_without_last_amended_keeps_the_rule_number_in_enacted() {
        let all_lines = lines(&[
            "高知大学国際交流会館料金規則",
            "平成16年４月１日",
            "規 則 第 152 号",
            "（趣旨）",
            "第１条  この規則は、必要な事項を定める。",
        ]);
        let split = split(&all_lines);
        assert_eq!(split.enacted_raw, "平成16年４月１日 規 則 第 152 号");
        assert_eq!(split.last_amended_raw, "");
        assert_eq!(split.body_lines[0], "（趣旨）");
        assert!(split.toc_lines.is_empty());
    }

    #[test]
    fn split_of_empty_input_does_not_panic() {
        let split = split(&[]);
        assert_eq!(split.title, "");
        assert!(split.body_lines.is_empty());
    }

    #[test]
    fn orphan_paragraph_and_item_markers_downgrade_without_panicking() {
        let (body, _, _) = parse_body(&lines(&[
            "２  宙に浮いた項マーカー。",
            "(1)  宙に浮いた号マーカー。",
            "第１条  実際の条文。",
        ]))
        .unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(article(&body[0]).number, main_no(1));
    }
}
