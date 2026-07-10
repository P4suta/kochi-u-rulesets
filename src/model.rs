use std::collections::BTreeSet;

use serde::Serialize;

use crate::dates::{Effective, Enactment, EraDate, RuleNumber};

#[derive(Debug, Serialize)]
pub struct Document {
    pub title: String,
    /// The enactment line ("平成20年３月26日 規則第74号"), structured with the raw
    /// text kept as a fallback.
    pub enacted: Enactment,
    /// The "最終改正 …" line; `None` when the source carries no such line.
    pub last_amended: Option<Enactment>,
    /// A "…の全部を改正する。" note, separated from `last_amended` so the amendment
    /// citation stays clean (S8).
    pub full_amendment_note: Option<String>,
    /// The main provision (本則), as a uniform tree of structural nodes. A document
    /// with 章 holds `Chapter` nodes at the root; a chapter-less document holds
    /// `Article` nodes directly at the root — there is no special case, only a
    /// different shape of the same tree.
    pub body: Vec<BodyNode>,
    /// 附則, in document order.
    pub supplementary_provisions: Vec<SupplProvision>,
    /// 別表 and 様式, unified and kept in document order (they interleave).
    pub appendices: Vec<Appendix>,
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
        number: BranchedNumber,
        title: String,
        children: Vec<BodyNode>,
    },
    /// 節
    Section {
        number: BranchedNumber,
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
    pub fn article_numbers(&self) -> BTreeSet<BranchedNumber> {
        self.all_articles().iter().map(|a| a.number).collect()
    }

    /// The top-level 章 nodes, for tests that assert on chapter structure.
    pub fn chapters(&self) -> impl Iterator<Item = &BodyNode> {
        self.body
            .iter()
            .filter(|n| matches!(n, BodyNode::Chapter { .. }))
    }

    /// The 別表 appendices (excludes 様式).
    pub fn tables(&self) -> impl Iterator<Item = &Appendix> {
        self.appendices
            .iter()
            .filter(|a| a.kind == AppendixKind::Table)
    }
}

/// A branch-suffixed structural number, shared by 章/節/条 (第N条の２, 第N節の２).
/// Kept structured rather than a formatted string so it sorts/compares correctly
/// (64 < 64の2 < 64の3 < 65).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BranchedNumber {
    pub main: u32,
    pub branch: Option<u32>,
}

impl BranchedNumber {
    /// Renders with the given unit character, e.g. `labeled('条')` → "第64条の２".
    pub fn labeled(&self, unit: char) -> String {
        let mut s = format!("第{}{unit}", self.main);
        if let Some(b) = self.branch {
            s.push_str(&format!("の{b}"));
        }
        s
    }
}

/// Alias kept for readability where a number is specifically an 条 number.
pub type ArticleNumber = BranchedNumber;

#[derive(Debug, Serialize)]
pub struct Article {
    pub number: BranchedNumber,
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
    /// 号 sub-items ("イ"/"ロ"/"ハ"…); empty for the common case.
    pub subitems: Vec<Subitem>,
}

/// An "イ"/"ロ"/"ハ" sub-item under a 号.
#[derive(Debug, Serialize)]
pub struct Subitem {
    pub label: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SupplProvision {
    /// 1-based position among 附則 blocks, in document order.
    pub ordinal: u32,
    /// "附則" for the founding block, "附則（令和８年１月28日規則第58号）" for amendments.
    pub heading_raw: String,
    /// The amending 規則 number from the heading; `None` for the founding block.
    pub amendment: Option<RuleNumber>,
    /// The promulgation date from the heading parenthetical.
    pub promulgated: Option<EraDate>,
    /// When the provision takes effect, from the "…から施行する" body clause.
    pub effective: Effective,
    /// The remaining body text (after effective-date extraction).
    pub text: String,
    /// "別表第N" tokens found in `text`, so a block referencing a table
    /// stays cross-linkable without parsing the table itself.
    pub references_tables: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppendixKind {
    /// 別表
    Table,
    /// 様式 / 別記様式
    Style,
}

/// A 別表 or 様式 appended after the main provision. `raw_text` is always present
/// as a faithful fallback; `cells` is populated only when the table grid could be
/// reconstructed with confidence (filled by the coordinate-based table module).
#[derive(Debug, Serialize)]
pub struct Appendix {
    pub kind: AppendixKind,
    /// "別表第１" / "別表" / "様式第１"
    pub id: String,
    /// "第５条関係", kept as raw text.
    pub related_article_raw: String,
    pub raw_text: String,
    pub cells: Option<Table>,
}

/// A reconstructed table grid. Vertical cell merges are resolved by fill-down;
/// there is no rowspan/colspan (the Markdown pipe-table target cannot express it).
#[derive(Debug, Serialize)]
pub struct Table {
    pub header_row_count: usize,
    pub rows: Vec<Vec<String>>,
}
