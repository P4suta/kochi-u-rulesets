use std::collections::BTreeSet;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Document {
    pub title: String,
    /// Raw "平成20年3月26日 規則第74号"-shaped text; era-date parsing is out of scope for v1.
    pub enacted: String,
    pub last_amended: String,
    pub chapters: Vec<Chapter>,
    /// 附則, in document order.
    pub supplementary_provisions: Vec<SupplementaryBlock>,
    /// 別表, in document order.
    pub appended_tables: Vec<AppendedTable>,
}

impl Document {
    /// Every article number appearing in the body, used to cross-check against
    /// `structure::toc::expected_article_numbers` in tests.
    pub fn article_numbers(&self) -> BTreeSet<ArticleNumber> {
        self.chapters
            .iter()
            .flat_map(|c| {
                c.articles
                    .iter()
                    .chain(c.sections.iter().flat_map(|s| s.articles.iter()))
            })
            .map(|a| a.number)
            .collect()
    }
}

#[derive(Debug, Serialize)]
pub struct Chapter {
    pub number: u32,
    pub title: String,
    /// Empty when the chapter holds articles directly (chapters 5-8 have no 節).
    pub sections: Vec<Section>,
    /// Articles directly under the chapter; empty when `sections` is non-empty.
    pub articles: Vec<Article>,
}

#[derive(Debug, Serialize)]
pub struct Section {
    pub number: u32,
    pub title: String,
    pub articles: Vec<Article>,
}

/// A 条 number, with an optional 号 (の２, の３...) branch suffix.
/// Kept structured rather than a formatted string so it sorts/compares correctly
/// (64 < 64の2 < 64の3 < 65) and survives future documents with different branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ArticleNumber {
    pub article: u32,
    pub branch: Option<u32>,
}

impl std::fmt::Display for ArticleNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "第{}条", self.article)?;
        if let Some(b) = self.branch {
            write!(f, "の{b}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct Article {
    pub number: ArticleNumber,
    /// From the "（タイトル）" line immediately preceding the article heading, if any.
    pub title: Option<String>,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Serialize)]
pub struct Paragraph {
    /// 1 for the unnumbered first paragraph, 2.. for subsequent numbered ones.
    pub number: u32,
    pub text: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub number: u32,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SupplementaryBlock {
    /// 1-based position among 附則 blocks, in document order.
    pub ordinal: u32,
    /// "附則" for the founding block, "附則（令和８年１月28日規則第58号）" for amendments.
    pub heading_raw: String,
    pub raw_text: String,
    /// "別表第N" tokens found in `raw_text`, so a block referencing a table
    /// stays cross-linkable without parsing the table itself.
    pub references_tables: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AppendedTable {
    /// "別表第１"
    pub id: String,
    /// "第５条関係", kept as raw text.
    pub related_article_raw: String,
    pub raw_text: String,
}
