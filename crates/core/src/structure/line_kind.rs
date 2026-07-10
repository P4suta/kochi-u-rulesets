use std::sync::LazyLock;

use regex::Regex;

use crate::model::AppendixKind;
use crate::numerals::parse_digits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind<'a> {
    Chapter {
        number: u32,
        branch: Option<u32>,
        title_raw: &'a str,
    },
    Section {
        number: u32,
        branch: Option<u32>,
        title_raw: &'a str,
    },
    SupplementaryMarker {
        heading_raw: &'a str,
    },
    /// 別表 or 様式 heading, e.g. "別表第１（第５条関係）", "様式第１（第18条関係）".
    AppendixMarker {
        kind: AppendixKind,
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
    /// A 号 sub-item, e.g. "イ  …".
    SubItemMarker {
        label: &'a str,
        rest: &'a str,
    },
    Continuation(&'a str),
}

// Spaces are tolerated inside the 第N章/第N節/第N条 markers themselves because
// pdf-extract renders some documents' double-digit markers spread as "第 10 条".
// The load-bearing trailing `\s+` on ARTICLE (below) still guards against
// citations, so this only widens what counts as the marker token, not the
// heading-vs-citation decision. The optional `の(\d+)` branch handles 第N章の２ /
// 第N節の２ / 第N条の２.
static CHAPTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^第\s*([0-9０-９]+)\s*章(?:\s*の\s*([0-9０-９]+))?\s*(.*)$").unwrap()
});
static SECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^第\s*([0-9０-９]+)\s*節(?:\s*の\s*([0-9０-９]+))?\s*(.*)$").unwrap()
});
static SUPPLEMENTARY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^附\s*則(（.*）)?$").unwrap());
// 別表/様式 in every observed form (別表第N, 別表N, 別表, 様式第N, 別記様式第N号).
// The mandatory "…関係）" parenthetical is the discriminator that keeps this from
// matching in-prose citations like "別表第１及び…規則（令和…）" (whose paren holds
// a date, not a 関係 reference) that appear inside 附則 raw text.
static APPENDIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:別記)?(?:別表|様式)\s*第?\s*[0-9０-９]*\s*号?)\s*（([^）]*関係)）\s*.*$")
        .unwrap()
});
static TITLE_ANNOTATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^（(.+)）$").unwrap());
// `\s+` (not `\s*`) after the number/branch is mandatory: real headings are
// always followed by a separating space before body text, whereas a mid-sentence
// citation like "第49条の２の規定により" has no space and must fall through to
// Continuation instead of being misread as a new Article heading.
static ARTICLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^第\s*([0-9０-９]+)\s*条(\s*の\s*([0-9０-９]+))?\s+(.*)$").unwrap()
});
static PARAGRAPH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9０-９]+)\s+(.*)$").unwrap());
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(([0-9]+)\)\s*(.*)$").unwrap());
// A 号 sub-item marker: a single イ/ロ/ハ… followed by a separating space. The
// `\s+` guard keeps a prose line that merely starts with a lone katakana from
// being misread as a sub-item.
static SUBITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([イロハニホヘトチリヌルヲ])\s+(.*)$").unwrap());

/// Classifies a single trimmed, non-empty line. Order matters only in that each
/// pattern's required literal prefix/suffix (章/節/条/附則/別表/様式/parens/kana)
/// is distinct, so there is no real ambiguity between them — patterns are tried
/// in the order a reader would recognize them.
pub fn classify(line: &str) -> LineKind<'_> {
    if let Some(c) = CHAPTER.captures(line) {
        return LineKind::Chapter {
            number: parse_digits(&c[1]).unwrap(),
            branch: c.get(2).and_then(|m| parse_digits(m.as_str())),
            title_raw: c.get(3).unwrap().as_str(),
        };
    }
    if let Some(c) = SECTION.captures(line) {
        return LineKind::Section {
            number: parse_digits(&c[1]).unwrap(),
            branch: c.get(2).and_then(|m| parse_digits(m.as_str())),
            title_raw: c.get(3).unwrap().as_str(),
        };
    }
    if SUPPLEMENTARY.is_match(line) {
        return LineKind::SupplementaryMarker { heading_raw: line };
    }
    if let Some(c) = APPENDIX.captures(line) {
        let id_raw = c.get(1).unwrap().as_str();
        let kind = if id_raw.contains("様式") {
            AppendixKind::Style
        } else {
            AppendixKind::Table
        };
        return LineKind::AppendixMarker {
            kind,
            id_raw,
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
    if let Some(c) = SUBITEM.captures(line) {
        return LineKind::SubItemMarker {
            label: c.get(1).unwrap().as_str(),
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
                branch: None,
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
                branch: None,
                title_raw: "学年、学期及び休業日"
            }
        );
    }

    #[test]
    fn section_with_branch() {
        assert_eq!(
            classify("第１節の２  削除"),
            LineKind::Section {
                number: 1,
                branch: Some(2),
                title_raw: "削除"
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
    fn appendix_table_marker() {
        assert_eq!(
            classify("別表第１（第５条関係）"),
            LineKind::AppendixMarker {
                kind: AppendixKind::Table,
                id_raw: "別表第１",
                related_raw: "第５条関係"
            }
        );
    }

    #[test]
    fn appendix_table_marker_without_number() {
        assert_eq!(
            classify("別表（第16条関係）"),
            LineKind::AppendixMarker {
                kind: AppendixKind::Table,
                id_raw: "別表",
                related_raw: "第16条関係"
            }
        );
    }

    #[test]
    fn appendix_style_markers() {
        assert_eq!(
            classify("様式第１（第18条関係）"),
            LineKind::AppendixMarker {
                kind: AppendixKind::Style,
                id_raw: "様式第１",
                related_raw: "第18条関係"
            }
        );
        assert_eq!(
            classify("別記様式第３号（第８条関係）"),
            LineKind::AppendixMarker {
                kind: AppendixKind::Style,
                id_raw: "別記様式第３号",
                related_raw: "第８条関係"
            }
        );
    }

    #[test]
    fn appendix_regex_ignores_prose_table_citation() {
        // A 附則 sentence citing 別表 with a dated parenthetical (not a 関係
        // reference) must NOT be mistaken for an appendix heading.
        let line = "別表第１及び高知大学学則の一部を改正する規則（令和８年１月28日規則第58号）";
        assert_eq!(classify(line), LineKind::Continuation(line));
    }

    #[test]
    fn subitem_marker() {
        assert_eq!(
            classify("イ  国民の祝日に関する法律"),
            LineKind::SubItemMarker {
                label: "イ",
                rest: "国民の祝日に関する法律"
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
