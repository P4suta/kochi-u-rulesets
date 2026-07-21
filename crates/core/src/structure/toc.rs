use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::ArticleNumber;
use crate::numerals::parse_digits;

// Only used as a test oracle, never for the shipped `Document`. The TOC line-noise
// is real (see the module doc below), so this is deliberately more permissive
// (optional whitespace around digit runs) than the body's `line_kind` regexes.
static TOC_REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"第\s*([0-9０-９]+)\s*条(?:の\s*([0-9０-９]+))?(?:\s*[－\-]\s*第\s*([0-9０-９]+)\s*条)?",
    )
    .unwrap()
});

/// Derives the set of article numbers a reader would expect to exist, purely from
/// the 目次 (table of contents) text — independent of the body parser, so it
/// serves as a cross-check oracle rather than a hardcoded article count.
///
/// Handles "第２条－第４条" ranges (expanded to every article in between; ranges
/// never carry a branch suffix in this document) and "第19条・第20条" pairs
/// (each ref matches independently since "・" isn't a recognized range separator).
pub fn expected_article_numbers(toc_lines: &[String]) -> BTreeSet<ArticleNumber> {
    let joined = toc_lines.join(" ");
    let mut out = BTreeSet::new();
    for c in TOC_REF.captures_iter(&joined) {
        let start = parse_digits(&c[1]).unwrap();
        let branch = c.get(2).and_then(|m| parse_digits(m.as_str()));
        if let Some(end_m) = c.get(3) {
            let end = parse_digits(end_m.as_str()).unwrap();
            for main in start..=end {
                out.insert(ArticleNumber { main, branch: None });
            }
        } else {
            out.insert(ArticleNumber {
                main: start,
                branch,
            });
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
    fn expands_range() {
        let toc = lines(&["第１節　学年、学期及び休業日（第２条－第４条）"]);
        let got = expected_article_numbers(&toc);
        let want: BTreeSet<_> = [2, 3, 4]
            .into_iter()
            .map(|main| ArticleNumber { main, branch: None })
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn splits_dot_joined_pair() {
        let toc = lines(&["第６節　賞罰（第19条・第20条）"]);
        let got = expected_article_numbers(&toc);
        let want: BTreeSet<_> = [19, 20]
            .into_iter()
            .map(|main| ArticleNumber { main, branch: None })
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn handles_spaced_branch_reference() {
        let toc = lines(&["第７章　特別の課程（第 84 条の２）"]);
        let got = expected_article_numbers(&toc);
        assert_eq!(
            got,
            BTreeSet::from([ArticleNumber {
                main: 84,
                branch: Some(2)
            }])
        );
    }

    #[test]
    fn handles_single_article_no_range() {
        let toc = lines(&["第１章　総則（第１条）"]);
        let got = expected_article_numbers(&toc);
        assert_eq!(
            got,
            BTreeSet::from([ArticleNumber {
                main: 1,
                branch: None
            }])
        );
    }
}
