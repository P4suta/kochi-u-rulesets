pub mod line_kind;
pub mod toc;

use crate::model::{
    Article, ArticleNumber, Chapter, Document, Item, Paragraph, Section, SupplementaryBlock,
};
use crate::numerals::normalize_title;
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
        .map(str::to_string)
        .collect()
}

/// The preamble (title/enacted/last_amended) plus the 目次 and body line ranges.
pub struct Split<'a> {
    pub title: String,
    pub enacted: String,
    pub last_amended: String,
    pub toc_lines: &'a [String],
    pub body_lines: &'a [String],
}

/// Separates the title-block preamble, the 目次 (table of contents), and the real
/// document body. The 目次 repeats every chapter/section heading the body uses,
/// so it can't just be classified alongside the body — it's identified by
/// scanning for "第１章" appearing a *second* time (the TOC lists chapter 1
/// once, the body restarts numbering from chapter 1 again).
pub fn split(lines: &[String]) -> Split<'_> {
    let title = lines.first().cloned().unwrap_or_default();

    let last_amended_idx = lines
        .iter()
        .position(|l| l.starts_with("最終改正"))
        .unwrap_or(1);
    let enacted = lines[1..last_amended_idx].join(" ");
    let last_amended = lines[last_amended_idx].clone();

    let after_preamble = &lines[last_amended_idx + 1..];
    let mut seen_chapter_one = false;
    let mut body_start_in_rest = 0;
    for (i, line) in after_preamble.iter().enumerate() {
        if let LineKind::Chapter { number: 1, .. } = line_kind::classify(line) {
            if seen_chapter_one {
                body_start_in_rest = i;
                break;
            }
            seen_chapter_one = true;
        }
    }

    let toc_lines = &after_preamble[..body_start_in_rest];
    let body_lines = &after_preamble[body_start_in_rest..];

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
    let (chapters, supplementary_provisions, appended_tables) = parse_body(split.body_lines)?;
    Ok(Document {
        title: split.title,
        enacted: split.enacted,
        last_amended: split.last_amended,
        chapters,
        supplementary_provisions,
        appended_tables,
    })
}

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
struct SectionB {
    number: u32,
    title: String,
    articles: Vec<Article>,
    cur_article: Option<ArticleB>,
}
struct ChapterB {
    number: u32,
    title: String,
    sections: Vec<Section>,
    cur_section: Option<SectionB>,
    articles: Vec<Article>,
    cur_article: Option<ArticleB>,
}

impl ItemB {
    fn finish(self) -> Item {
        Item {
            number: self.number,
            text: self.text.join("").trim().to_string(),
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
            text: self.text.join("").trim().to_string(),
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
}
impl SectionB {
    fn close_article(&mut self) {
        if let Some(mut a) = self.cur_article.take() {
            a.close_paragraph();
            self.articles.push(a.finish());
        }
    }
    fn finish(mut self) -> Section {
        self.close_article();
        Section {
            number: self.number,
            title: self.title,
            articles: self.articles,
        }
    }
}
impl ChapterB {
    fn close_article(&mut self) {
        if let Some(mut a) = self.cur_article.take() {
            a.close_paragraph();
            self.articles.push(a.finish());
        }
    }
    fn close_section(&mut self) {
        self.close_article();
        if let Some(s) = self.cur_section.take() {
            self.sections.push(s.finish());
        }
    }
    fn finish(mut self) -> Chapter {
        self.close_section();
        Chapter {
            number: self.number,
            title: self.title,
            sections: self.sections,
            articles: self.articles,
        }
    }
}

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

fn parse_body(
    lines: &[String],
) -> anyhow::Result<(
    Vec<Chapter>,
    Vec<SupplementaryBlock>,
    Vec<crate::model::AppendedTable>,
)> {
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut cur_chapter: Option<ChapterB> = None;
    let mut pending_title: Option<String> = None;

    let mut supplementary: Vec<SupplementaryBlock> = Vec::new();
    let mut appended_tables: Vec<crate::model::AppendedTable> = Vec::new();
    let mut cur_raw: Option<RawB> = None;

    let flush_chapter = |cur_chapter: &mut Option<ChapterB>, chapters: &mut Vec<Chapter>| {
        if let Some(c) = cur_chapter.take() {
            chapters.push(c.finish());
        }
    };
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

        match kind {
            LineKind::Chapter { number, title_raw } => {
                flush_chapter(&mut cur_chapter, &mut chapters);
                cur_chapter = Some(ChapterB {
                    number,
                    title: normalize_title(title_raw),
                    sections: Vec::new(),
                    cur_section: None,
                    articles: Vec::new(),
                    cur_article: None,
                });
            }
            LineKind::Section { number, title_raw } => {
                let chapter = cur_chapter.as_mut().expect("section outside a chapter");
                chapter.close_section();
                chapter.cur_section = Some(SectionB {
                    number,
                    title: normalize_title(title_raw),
                    articles: Vec::new(),
                    cur_article: None,
                });
            }
            LineKind::TitleAnnotation { title_raw } => {
                pending_title = Some(title_raw.to_string());
            }
            LineKind::ArticleHeading {
                number,
                branch,
                rest,
            } => {
                let chapter = cur_chapter.as_mut().expect("article outside a chapter");
                let title = pending_title.take();
                let new_article = ArticleB {
                    number: ArticleNumber {
                        article: number,
                        branch,
                    },
                    title,
                    paragraphs: Vec::new(),
                    cur_paragraph: Some(ParagraphB {
                        number: 1,
                        text: Vec::new(),
                        items: Vec::new(),
                        cur_item: None,
                    }),
                };
                if let Some(section) = chapter.cur_section.as_mut() {
                    section.close_article();
                    section.cur_article = Some(new_article);
                } else {
                    chapter.close_article();
                    chapter.cur_article = Some(new_article);
                }
                append_to_current(chapter, rest);
            }
            LineKind::ParagraphMarker { number, rest } => {
                let chapter = cur_chapter.as_mut().expect("paragraph outside an article");
                let article = current_article_mut(chapter).expect("paragraph outside an article");
                article.close_paragraph();
                article.cur_paragraph = Some(ParagraphB {
                    number,
                    text: Vec::new(),
                    items: Vec::new(),
                    cur_item: None,
                });
                append_to_current(chapter, rest);
            }
            LineKind::ItemMarker { number, rest } => {
                let chapter = cur_chapter.as_mut().expect("item outside a paragraph");
                let article = current_article_mut(chapter).expect("item outside a paragraph");
                let paragraph = article
                    .cur_paragraph
                    .as_mut()
                    .expect("item outside a paragraph");
                paragraph.close_item();
                paragraph.cur_item = Some(ItemB {
                    number,
                    text: Vec::new(),
                });
                append_to_current(chapter, rest);
            }
            LineKind::Continuation(text) => {
                if let Some(chapter) = cur_chapter.as_mut() {
                    append_to_current(chapter, text);
                }
                // Continuation lines before the first chapter (shouldn't happen in
                // this document) are silently dropped rather than panicking.
            }
            LineKind::SupplementaryMarker { heading_raw } => {
                flush_chapter(&mut cur_chapter, &mut chapters);
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
                flush_chapter(&mut cur_chapter, &mut chapters);
                cur_raw = Some(RawB {
                    kind: RawKind::Table,
                    heading_raw: id_raw.to_string(),
                    related_raw: related_raw.to_string(),
                    text: Vec::new(),
                });
            }
        }
    }

    flush_chapter(&mut cur_chapter, &mut chapters);
    flush_raw(&mut cur_raw, &mut supplementary, &mut appended_tables);

    Ok((chapters, supplementary, appended_tables))
}

fn current_article_mut(chapter: &mut ChapterB) -> Option<&mut ArticleB> {
    if let Some(section) = chapter.cur_section.as_mut() {
        section.cur_article.as_mut()
    } else {
        chapter.cur_article.as_mut()
    }
}

/// Appends a line fragment to whichever unit is currently innermost open
/// (item > paragraph body), joining wrapped lines without a separator since
/// Japanese text wraps without spaces.
fn append_to_current(chapter: &mut ChapterB, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(article) = current_article_mut(chapter)
        && let Some(p) = article.cur_paragraph.as_mut()
    {
        if let Some(item) = p.cur_item.as_mut() {
            item.text.push(text.to_string());
        } else {
            p.text.push(text.to_string());
        }
    }
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

    #[test]
    fn article_with_title_paragraphs_and_items() {
        let (chapters, _, _) = parse_body(&lines(&[
            "第１章  総  則",
            "（目的）",
            "第１条  高知大学の学部においては、次の理念を掲げる。",
            "(1)  広範な教養と高度な専門知識",
            "を培い、人材を育成する。",
            "(2)  諸科学の基礎と応用",
            "２  本学大学院においては、教授研究する。",
        ]))
        .unwrap();

        assert_eq!(chapters.len(), 1);
        let ch = &chapters[0];
        assert_eq!(ch.title, "総則");
        assert_eq!(ch.articles.len(), 1);

        let a = &ch.articles[0];
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
        // Wrapped continuation line joins without a separator.
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
    fn branch_article_is_a_sibling_with_a_distinct_number() {
        let (chapters, _, _) = parse_body(&lines(&[
            "第４章  大学院",
            "第58条  専門職学位課程に入学することのできる者は、次のとおりとする。",
            "第58条の２ 専門職学位課程に入学することのできる者は、前条による。",
        ]))
        .unwrap();

        let articles = &chapters[0].articles;
        assert_eq!(articles.len(), 2);
        assert_eq!(
            articles[0].number,
            ArticleNumber {
                article: 58,
                branch: None
            }
        );
        assert_eq!(
            articles[1].number,
            ArticleNumber {
                article: 58,
                branch: Some(2)
            }
        );
    }

    #[test]
    fn supplementary_and_table_blocks_stay_raw_and_ordered() {
        let (chapters, supplementary, tables) = parse_body(&lines(&[
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

        assert_eq!(chapters[0].articles.len(), 1);

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
    fn toc_lines_are_excluded_from_the_body() {
        // "目次" then a TOC-style single-line chapter entry (with a trailing
        // article-range parenthetical, unlike the real body heading) that must
        // not be mistaken for the real chapter — only the second "第１章" starts
        // real parsing.
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

        let doc = parse(&all_lines).unwrap();
        assert_eq!(doc.chapters.len(), 1);
        assert_eq!(doc.chapters[0].title, "総則");
    }
}
