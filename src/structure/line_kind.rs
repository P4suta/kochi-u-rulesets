use std::sync::LazyLock;

use regex::Regex;

use crate::numerals::parse_digits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind<'a> {
    Chapter {
        number: u32,
        title_raw: &'a str,
    },
    Section {
        number: u32,
        title_raw: &'a str,
    },
    SupplementaryMarker {
        heading_raw: &'a str,
    },
    AppendedTableMarker {
        id_raw: &'a str,
        related_raw: &'a str,
    },
    /// A full line consisting entirely of "（...）" — an article title, when it
    /// immediately precedes an ArticleHeading.
    TitleAnnotation {
        title_raw: &'a str,
    },
    ArticleHeading {
        number: u32,
        branch: Option<u32>,
        rest: &'a str,
    },
    ParagraphMarker {
        number: u32,
        rest: &'a str,
    },
    ItemMarker {
        number: u32,
        rest: &'a str,
    },
    Continuation(&'a str),
}

static CHAPTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^第([0-9０-９]+)章\s*(.*)$").unwrap());
static SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^第([0-9０-９]+)節\s*(.*)$").unwrap());
static SUPPLEMENTARY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^附\s*則(（.*）)?$").unwrap());
static APPENDED_TABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(別表第[0-9０-９]+)（(.*)）$").unwrap());
static TITLE_ANNOTATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^（(.+)）$").unwrap());
// `\s+` (not `\s*`) after the number/branch is mandatory: real headings are
// always followed by a separating space before body text, whereas a mid-sentence
// citation like "第49条の２の規定により" has no space and must fall through to
// Continuation instead of being misread as a new Article heading.
static ARTICLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^第([0-9０-９]+)条(の([0-9０-９]+))?\s+(.*)$").unwrap());
static PARAGRAPH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9０-９]+)\s+(.*)$").unwrap());
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(([0-9]+)\)\s*(.*)$").unwrap());

/// Classifies a single trimmed, non-empty line. Order matters only in that each
/// pattern's required literal prefix/suffix (章/節/条/附則/別表第/parens) is
/// distinct, so there is no real ambiguity between them — patterns are tried
/// in the order a reader would recognize them.
pub fn classify(line: &str) -> LineKind<'_> {
    if let Some(c) = CHAPTER.captures(line) {
        return LineKind::Chapter {
            number: parse_digits(&c[1]).unwrap(),
            title_raw: c.get(2).unwrap().as_str(),
        };
    }
    if let Some(c) = SECTION.captures(line) {
        return LineKind::Section {
            number: parse_digits(&c[1]).unwrap(),
            title_raw: c.get(2).unwrap().as_str(),
        };
    }
    if SUPPLEMENTARY.is_match(line) {
        return LineKind::SupplementaryMarker { heading_raw: line };
    }
    if let Some(c) = APPENDED_TABLE.captures(line) {
        return LineKind::AppendedTableMarker {
            id_raw: c.get(1).unwrap().as_str(),
            related_raw: c.get(2).unwrap().as_str(),
        };
    }
    if let Some(c) = TITLE_ANNOTATION.captures(line) {
        return LineKind::TitleAnnotation {
            title_raw: c.get(1).unwrap().as_str(),
        };
    }
    if let Some(c) = ARTICLE.captures(line) {
        return LineKind::ArticleHeading {
            number: parse_digits(&c[1]).unwrap(),
            branch: c.get(3).and_then(|m| parse_digits(m.as_str())),
            rest: c.get(4).unwrap().as_str(),
        };
    }
    if let Some(c) = PARAGRAPH.captures(line) {
        return LineKind::ParagraphMarker {
            number: parse_digits(&c[1]).unwrap(),
            rest: c.get(2).unwrap().as_str(),
        };
    }
    if let Some(c) = ITEM.captures(line) {
        return LineKind::ItemMarker {
            number: c[1].parse().unwrap(),
            rest: c.get(2).unwrap().as_str(),
        };
    }
    LineKind::Continuation(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_heading() {
        assert_eq!(
            classify("第１章  総  則"),
            LineKind::Chapter {
                number: 1,
                title_raw: "総  則"
            }
        );
    }

    #[test]
    fn section_heading() {
        assert_eq!(
            classify("第１節  学年、学期及び休業日"),
            LineKind::Section {
                number: 1,
                title_raw: "学年、学期及び休業日"
            }
        );
    }

    #[test]
    fn founding_supplementary_marker() {
        assert_eq!(
            classify("附則"),
            LineKind::SupplementaryMarker {
                heading_raw: "附則"
            }
        );
    }

    #[test]
    fn dated_supplementary_marker() {
        let line = "附  則（令和８年１月28日規則第58号）";
        assert_eq!(
            classify(line),
            LineKind::SupplementaryMarker { heading_raw: line }
        );
    }

    #[test]
    fn appended_table_marker() {
        assert_eq!(
            classify("別表第１（第５条関係）"),
            LineKind::AppendedTableMarker {
                id_raw: "別表第１",
                related_raw: "第５条関係"
            }
        );
    }

    #[test]
    fn title_annotation() {
        assert_eq!(
            classify("（目的）"),
            LineKind::TitleAnnotation {
                title_raw: "目的"
            }
        );
    }

    #[test]
    fn article_heading_no_branch_halfwidth_number() {
        assert_eq!(
            classify("第10条  病気その他の理由により"),
            LineKind::ArticleHeading {
                number: 10,
                branch: None,
                rest: "病気その他の理由により"
            }
        );
    }

    #[test]
    fn article_heading_with_branch() {
        assert_eq!(
            classify("第64条の３ 専門職学位課程においては"),
            LineKind::ArticleHeading {
                number: 64,
                branch: Some(3),
                rest: "専門職学位課程においては"
            }
        );
    }

    #[test]
    fn paragraph_marker() {
        assert_eq!(
            classify("２  本学大学院においては"),
            LineKind::ParagraphMarker {
                number: 2,
                rest: "本学大学院においては"
            }
        );
    }

    #[test]
    fn inline_article_citation_is_not_an_article_heading() {
        // "第49条の２の規定により修得したとみなす..." — a mid-sentence citation of
        // Article 49-2, not a new heading (no space between "２" and "の規定").
        // Regressed real parsing (duplicate 第49条の２) before requiring `\s+`.
        let line = "第49条の２の規定により修得したとみなすものとする単位は、30単位を超えないものと";
        assert_eq!(classify(line), LineKind::Continuation(line));
    }

    #[test]
    fn digit_immediately_followed_by_kanji_is_not_a_paragraph_marker() {
        // "３月31日に終わる" — no space after the digit, so this must fall through
        // to Continuation rather than being misread as paragraph 3.
        assert_eq!(
            classify("３月31日に終わる。"),
            LineKind::Continuation("３月31日に終わる。")
        );
    }

    #[test]
    fn item_marker_double_digit() {
        assert_eq!(
            classify("(10)  本学大学院において"),
            LineKind::ItemMarker {
                number: 10,
                rest: "本学大学院において"
            }
        );
    }

    #[test]
    fn plain_continuation() {
        assert_eq!(
            classify("を培い、人類の健全な発展に積極的に貢献する人材を育成する。"),
            LineKind::Continuation("を培い、人類の健全な発展に積極的に貢献する人材を育成する。")
        );
    }
}
