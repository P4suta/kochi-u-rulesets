use std::collections::BTreeSet;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Document {
    pub title: String,
    /// Raw "平成20年3月26日 規則第74号"-shaped text; era-date parsing is out of scope for v1.
    pub enacted: String,
    /// The "最終改正 …" line, verbatim; empty when the source carries no such line.
    pub last_amended: String,
    /// The main provision (本則), as a uniform tree of structural nodes. A document
    /// with 章 holds `Chapter` nodes at the root; a chapter-less document holds
    /// `Article` nodes directly at the root — there is no special case, only a
    /// different shape of the same tree.
    pub body: Vec<BodyNode>,
    /// 附則, in document order.
    pub supplementary_provisions: Vec<SupplementaryBlock>,
    /// 別表, in document order.
    pub appended_tables: Vec<AppendedTable>,
}

/// A node of the main provision tree: either a structural division (章/節) or a
/// leaf 条. Serialized with an internal `"type"` tag so each node is
/// self-describing, following the hierarchy of the Japanese government's
/// 法令標準XMLスキーマ (e-Gov): 章 Chapter → 節 Section → 条 Article.
///
/// Only 章 and 節 are modeled because only they appear in this corpus. The deeper
/// standard levels (編/款/目) would be additional variants here if a future
/// document needed them — they are intentionally omitted rather than added as
/// untested, never-emitted variants.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BodyNode {
    /// 章
    Chapter {
        number: u32,
        title: String,
        children: Vec<BodyNode>,
    },
    /// 節
    Section {
        number: u32,
        title: String,
        children: Vec<BodyNode>,
    },
    /// 条
    Article(Article),
}

impl BodyNode {
    /// Recursively visits every `Article` in this subtree, in document order.
    pub fn walk_articles<'a>(&'a self, out: &mut Vec<&'a Article>) {
        match self {
            BodyNode::Chapter { children, .. } | BodyNode::Section { children, .. } => {
                for child in children {
                    child.walk_articles(out);
                }
            }
            BodyNode::Article(article) => out.push(article),
        }
    }
}

impl Document {
    /// Every article, in document order, regardless of how deeply it is nested.
    pub fn all_articles(&self) -> Vec<&Article> {
        let mut out = Vec::new();
        for node in &self.body {
            node.walk_articles(&mut out);
        }
        out
    }

    /// Every article number appearing in the body, used to cross-check against
    /// `structure::toc::expected_article_numbers` in tests.
    pub fn article_numbers(&self) -> BTreeSet<ArticleNumber> {
        self.all_articles().iter().map(|a| a.number).collect()
    }

    /// The top-level 章 nodes, for tests that assert on chapter structure.
    pub fn chapters(&self) -> impl Iterator<Item = &BodyNode> {
        self.body
            .iter()
            .filter(|n| matches!(n, BodyNode::Chapter { .. }))
    }
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
