//! Inline references and 制定根拠 (enactment authorities).
//!
//! A post-parse pass over a `Document`: it never runs inside the PDF parser, so
//! `structure` stays about *shape* and this stays about *cross-references*. Three
//! products, in dependency order:
//!   1. [`parse_authorities`] — a document's 制定根拠, from its 第1条 "…の規定に
//!      基づき" citation (which parent 条項 it is enacted under).
//!   2. [`build_subordinate_index`] — the reverse of (1) across the whole corpus:
//!      parent clause → child rulesets. This is what resolves an anonymous
//!      「別に定める」 to the rule that actually carries the delegated provision.
//!   3. [`annotate`] — inline [`TextRef`]s in every body text (前項/前条/第N条/
//!      別表第N/other rulesets/別に定める) plus each 条's `subordinate_rules`.
//!
//! Offsets in emitted `TextRef`s are **character** (Unicode scalar) indices, matching
//! the frontend's `Array.from(text)` slicing — never byte offsets.

use std::collections::HashMap;

use crate::graph::is_name_char;
use crate::model::{Article, Authority, BodyNode, BranchedNumber, Document, RefTarget, TextRef};
use crate::numerals::parse_digits;

// ── Shared little scanners over a `&[char]` ──────────────────────────────────────

/// Ruleset names as char vectors, longest first so the longest candidate wins at any
/// scan position (names nest: 高知大学学生準則 ⊂ 高知大学学生懲戒規則's prefix).
pub fn candidates(vocab: &[(String, String)]) -> Vec<(Vec<char>, String)> {
    let mut c: Vec<(Vec<char>, String)> = vocab
        .iter()
        .map(|(code, name)| (name.chars().collect::<Vec<char>>(), code.clone()))
        .filter(|(chars, _)| !chars.is_empty())
        .collect();
    c.sort_by_key(|(chars, _)| std::cmp::Reverse(chars.len()));
    c
}

/// Reads a run of half/full-width digits at `chars[i..]`, returning `(value, len)`.
fn read_number(chars: &[char], i: usize) -> Option<(u32, usize)> {
    let mut j = i;
    while j < chars.len() && (chars[j].is_ascii_digit() || ('０'..='９').contains(&chars[j])) {
        j += 1;
    }
    if j == i {
        return None;
    }
    let s: String = chars[i..j].iter().collect();
    parse_digits(&s).map(|n| (n, j - i))
}

fn matches_at(chars: &[char], i: usize, pat: &[char]) -> bool {
    chars.len() >= i + pat.len() && chars[i..i + pat.len()] == *pat
}

/// Longest vocab name whose chars sit at `chars[i..]`. Returns `(len, code)`.
/// No maximal-token guard: callers that need one (inline prose) apply it themselves;
/// citation contexts (a name followed by 第N条) rely on longest-first ordering.
fn match_vocab(chars: &[char], i: usize, cands: &[(Vec<char>, String)]) -> Option<(usize, String)> {
    for (name, code) in cands {
        let end = i + name.len();
        if end <= chars.len() && chars[i..end] == name[..] {
            return Some((name.len(), code.clone()));
        }
    }
    None
}

/// `第N条(のM)?(第K項)?` at `chars[i..]` → `(article, end, paragraph, raw)`.
fn match_article_citation(
    chars: &[char],
    i: usize,
) -> Option<(BranchedNumber, usize, Option<u32>, String)> {
    if chars.get(i) != Some(&'第') {
        return None;
    }
    let (main, nlen) = read_number(chars, i + 1)?;
    let mut j = i + 1 + nlen;
    if chars.get(j) != Some(&'条') {
        return None;
    }
    j += 1;
    let mut branch = None;
    if chars.get(j) == Some(&'の')
        && let Some((b, blen)) = read_number(chars, j + 1)
    {
        branch = Some(b);
        j += 1 + blen;
    }
    let article = BranchedNumber { main, branch };
    let mut paragraph = None;
    if chars.get(j) == Some(&'第')
        && let Some((p, plen)) = read_number(chars, j + 1)
        && chars.get(j + 1 + plen) == Some(&'項')
    {
        paragraph = Some(p);
        j += 1 + plen + 1;
    }
    let raw: String = chars[i..j].iter().collect();
    Some((article, j, paragraph, raw))
}

/// `第K項` at `chars[i..]` → `(paragraph, end, raw)`.
fn match_paragraph_citation(chars: &[char], i: usize) -> Option<(u32, usize, String)> {
    if chars.get(i) != Some(&'第') {
        return None;
    }
    let (p, plen) = read_number(chars, i + 1)?;
    let end = i + 1 + plen;
    if chars.get(end) != Some(&'項') {
        return None;
    }
    let raw: String = chars[i..end + 1].iter().collect();
    Some((p, end + 1, raw))
}

/// `別表第N` at `chars[i..]` → `(len, table_number)`.
fn match_table_ref(chars: &[char], i: usize) -> Option<(usize, u32)> {
    let prefix = ['別', '表', '第'];
    if !matches_at(chars, i, &prefix) {
        return None;
    }
    let (num, nlen) = read_number(chars, i + prefix.len())?;
    Some((prefix.len() + nlen, num))
}

/// The 「別に定め」/「別途定め」 delegation stem at `chars[i..]` → span length. The
/// short stem is deliberate: it prefixes every form (別に定める／別に定めのある／
/// 別に定めるところにより／別に定めるもののほか), and marking it is enough.
fn match_separately(chars: &[char], i: usize) -> Option<usize> {
    for stem in [['別', 'に', '定', 'め'], ['別', '途', '定', 'め']] {
        if matches_at(chars, i, &stem) {
            return Some(stem.len());
        }
    }
    None
}

// ── 1. Enactment authorities (制定根拠) ──────────────────────────────────────────

/// Parses the 第1条 "…の規定に基づき" citation into the parent 条項 this ruleset is
/// enacted under. A single clause may cite several parents and several 条項, so the
/// result is a `Vec`. Returns empty for a root document (学則) or when no such clause
/// exists.
pub fn parse_authorities(doc: &Document, vocab: &[(String, String)]) -> Vec<Authority> {
    let Some(first) = doc.all_articles().into_iter().next() else {
        return Vec::new();
    };
    let Some(para) = first.paragraphs.first() else {
        return Vec::new();
    };
    // The citation region is everything up to the governing "の規定に基づ(き|く)".
    let Some(byte_idx) = para.text.find("の規定に基づ") else {
        return Vec::new();
    };
    let region: Vec<char> = para.text[..byte_idx].chars().collect();
    let cands = candidates(vocab);

    let mut out: Vec<Authority> = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_code: Option<String> = None;
    let mut cur_article: Option<BranchedNumber> = None;

    let mut i = 0usize;
    while i < region.len() {
        // A parent name: vocab (resolves a code), else a bare name run that governs a
        // following 第…条 (out-of-corpus parent — external law, unlisted 規程).
        if let Some((len, code)) = match_vocab(&region, i, &cands) {
            cur_name = Some(region[i..i + len].iter().collect());
            cur_code = Some(code);
            cur_article = None;
            i += len;
            continue;
        }
        if let Some((len, name)) = match_governing_name(&region, i) {
            cur_name = Some(name);
            cur_code = None;
            cur_article = None;
            i += len;
            continue;
        }
        // 第N条(のM)(第K項) — pins the article (and maybe paragraph).
        if let Some((article, end, paragraph, raw)) = match_article_citation(&region, i) {
            cur_article = Some(article);
            out.push(Authority {
                rule_code: cur_code.clone(),
                rule_name: cur_name.clone().unwrap_or_default(),
                article: Some(article),
                paragraph,
                raw,
            });
            i = end;
            continue;
        }
        // Standalone 第K項 ("…第2項及び第3項…") — attaches to the current 条.
        if let Some((paragraph, end, raw)) = match_paragraph_citation(&region, i) {
            if let Some(article) = cur_article {
                out.push(Authority {
                    rule_code: cur_code.clone(),
                    rule_name: cur_name.clone().unwrap_or_default(),
                    article: Some(article),
                    paragraph: Some(paragraph),
                    raw,
                });
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

/// A maximal run of name-constituent chars (plus an optional following （…）
/// parenthetical) at `chars[i..]`, but only when it governs a 第…条 citation — i.e.
/// the run/paren is immediately followed by `第<digits>条`. Returns `(len, name)`,
/// where `name` excludes the parenthetical. Used to capture out-of-corpus parents
/// like 職業安定法 or 学位規則（昭和28年文部省令第9号）.
fn match_governing_name(chars: &[char], i: usize) -> Option<(usize, String)> {
    let mut j = i;
    while j < chars.len() && is_name_char(chars[j]) {
        j += 1;
    }
    if j == i {
        return None;
    }
    let name: String = chars[i..j].iter().collect();
    // Skip an optional （…） group before the article pointer.
    let mut k = j;
    if chars.get(k) == Some(&'（') {
        let mut depth = 0usize;
        while k < chars.len() {
            match chars[k] {
                '（' => depth += 1,
                '）' => {
                    depth -= 1;
                    k += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                _ => {}
            }
            k += 1;
        }
        if depth != 0 {
            return None;
        }
    }
    // Governance test: the run must point at an article.
    if match_article_citation(chars, k).is_some() {
        Some((k - i, name))
    } else {
        None
    }
}

// ── 2. Subordinate index (parent clause → child rulesets) ────────────────────────

/// The reverse of every document's [`Authority`] list: given a parent 条 (or 条項),
/// which child rulesets are enacted under it. Populated from the corpus, then queried
/// while annotating each parent's 別に定める tokens and 条 panels.
pub struct SubordinateIndex {
    /// (parent, 条, 項) → children whose 制定根拠 pins that exact 項.
    clause: HashMap<(String, BranchedNumber, u32), Vec<String>>,
    /// (parent, 条) → children whose 制定根拠 pins the 条 but no 項.
    article_nopar: HashMap<(String, BranchedNumber), Vec<String>>,
    /// (parent, 条) → every child under that 条, regardless of 項 (for the panel).
    article_all: HashMap<(String, BranchedNumber), Vec<String>>,
}

fn push_unique(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

pub fn build_subordinate_index(docs: &[(String, Document)]) -> SubordinateIndex {
    let mut clause: HashMap<(String, BranchedNumber, u32), Vec<String>> = HashMap::new();
    let mut article_nopar: HashMap<(String, BranchedNumber), Vec<String>> = HashMap::new();
    let mut article_all: HashMap<(String, BranchedNumber), Vec<String>> = HashMap::new();

    for (child, doc) in docs {
        for a in &doc.authorities {
            let (Some(parent), Some(article)) = (a.rule_code.as_ref(), a.article) else {
                continue;
            };
            push_unique(
                article_all.entry((parent.clone(), article)).or_default(),
                child,
            );
            match a.paragraph {
                Some(p) => push_unique(
                    clause.entry((parent.clone(), article, p)).or_default(),
                    child,
                ),
                None => push_unique(
                    article_nopar.entry((parent.clone(), article)).or_default(),
                    child,
                ),
            }
        }
    }
    SubordinateIndex {
        clause,
        article_nopar,
        article_all,
    }
}

impl SubordinateIndex {
    /// Children a 別に定める in this exact clause resolves to: the 項-pinned children
    /// for this 項, plus the article-only-pinned children (which can sit in any 項 of
    /// the 条). Precise 項 matches never leak into a different 項.
    fn inline_children(&self, code: &str, article: BranchedNumber, paragraph: u32) -> Vec<String> {
        let mut v = self
            .clause
            .get(&(code.to_string(), article, paragraph))
            .cloned()
            .unwrap_or_default();
        if let Some(extra) = self.article_nopar.get(&(code.to_string(), article)) {
            for c in extra {
                push_unique(&mut v, c);
            }
        }
        v
    }

    /// Every child ruleset enacted under this 条 (the 条-level panel).
    fn panel_children(&self, code: &str, article: BranchedNumber) -> Vec<String> {
        self.article_all
            .get(&(code.to_string(), article))
            .cloned()
            .unwrap_or_default()
    }
}

// ── 3. Inline annotation ─────────────────────────────────────────────────────────

/// Document-wide context for scanning one text span.
struct DocCtx<'a> {
    self_code: &'a str,
    self_articles: &'a std::collections::BTreeSet<BranchedNumber>,
    /// 別表 number → its appendix id (the scroll anchor), for 別表第N resolution.
    tables: HashMap<u32, String>,
    cands: Vec<(Vec<char>, String)>,
    index: &'a SubordinateIndex,
}

/// Position of the text within its 条, for relative-reference resolution.
struct TextCtx {
    article: BranchedNumber,
    prev_article: Option<BranchedNumber>,
    para_number: u32,
    para_count: u32,
}

/// Fills `refs`/`subordinate_rules` throughout `doc`. `vocab` supplies the ruleset
/// name vocabulary; `index` supplies the 別に定める → child resolution.
pub fn annotate(
    self_code: &str,
    doc: &mut Document,
    vocab: &[(String, String)],
    index: &SubordinateIndex,
) {
    let self_articles = doc.article_numbers();
    let mut tables: HashMap<u32, String> = HashMap::new();
    for ap in &doc.appendices {
        if let Some((_, num)) = match_table_ref(&ap.id.chars().collect::<Vec<_>>(), 0) {
            tables.entry(num).or_insert_with(|| ap.id.clone());
        }
    }
    let ctx = DocCtx {
        self_code,
        self_articles: &self_articles,
        tables,
        cands: candidates(vocab),
        index,
    };

    let mut prev: Option<BranchedNumber> = None;
    annotate_nodes(&mut doc.body, &mut prev, &ctx);
}

fn annotate_nodes(nodes: &mut [BodyNode], prev: &mut Option<BranchedNumber>, ctx: &DocCtx) {
    for node in nodes {
        match node {
            BodyNode::Chapter { children, .. } | BodyNode::Section { children, .. } => {
                annotate_nodes(children, prev, ctx)
            }
            BodyNode::Article(a) => {
                annotate_article(a, *prev, ctx);
                *prev = Some(a.number);
            }
        }
    }
}

fn annotate_article(a: &mut Article, prev_article: Option<BranchedNumber>, ctx: &DocCtx) {
    let article = a.number;
    let para_count = a.paragraphs.len() as u32;
    for p in &mut a.paragraphs {
        let tc = TextCtx {
            article,
            prev_article,
            para_number: p.number,
            para_count,
        };
        p.refs = scan(&p.text, ctx, &tc);
        for item in &mut p.items {
            item.refs = scan(&item.text, ctx, &tc);
            for sub in &mut item.subitems {
                sub.refs = scan(&sub.text, ctx, &tc);
            }
        }
    }
    a.subordinate_rules = ctx.index.panel_children(ctx.self_code, article);
}

/// The one-pass, left-to-right scanner. At each position the first matching pattern
/// wins and consumes its span; order encodes precedence (別表第 before 別に定め;
/// 第N条 before 第N項). Every emitted offset is a char index into `text`.
fn scan(text: &str, ctx: &DocCtx, tc: &TextCtx) -> Vec<TextRef> {
    let chars: Vec<char> = text.chars().collect();
    let mut refs: Vec<TextRef> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        // Another ruleset by name — with the graph's maximal-token guard (drop when
        // the next char would extend the name into a different, longer token).
        if let Some((len, code)) = match_vocab(&chars, i, &ctx.cands) {
            let end = i + len;
            let extends = chars.get(end).is_some_and(|&c| is_name_char(c));
            if !extends {
                if code != ctx.self_code {
                    refs.push(TextRef {
                        start: i,
                        end,
                        target: RefTarget::Rule { code },
                    });
                }
                i = end;
                continue;
            }
        }
        // 別表第N → the appendix, when it exists in this document.
        if let Some((len, num)) = match_table_ref(&chars, i) {
            if let Some(id) = ctx.tables.get(&num) {
                refs.push(TextRef {
                    start: i,
                    end: i + len,
                    target: RefTarget::Table {
                        appendix_id: id.clone(),
                    },
                });
            }
            i += len;
            continue;
        }
        // 別に定め — the delegation marker; resolved to child rulesets when the reverse
        // index has any for this clause, else an unlinked marker.
        if let Some(len) = match_separately(&chars, i) {
            let rules = ctx
                .index
                .inline_children(ctx.self_code, tc.article, tc.para_number);
            refs.push(TextRef {
                start: i,
                end: i + len,
                target: RefTarget::SeparatelyProvided { rules },
            });
            i += len;
            continue;
        }
        // 前項 → the preceding 項 in this 条.
        if matches_at(&chars, i, &['前', '項']) {
            if tc.para_number >= 2 {
                refs.push(TextRef {
                    start: i,
                    end: i + 2,
                    target: RefTarget::Paragraph {
                        paragraph: tc.para_number - 1,
                    },
                });
            }
            i += 2;
            continue;
        }
        // 前条 → the preceding 条 in this document.
        if matches_at(&chars, i, &['前', '条']) {
            if let Some(prev) = tc.prev_article {
                refs.push(TextRef {
                    start: i,
                    end: i + 2,
                    target: RefTarget::Article {
                        article: prev,
                        paragraph: None,
                    },
                });
            }
            i += 2;
            continue;
        }
        // 第N条(のM)(第K項) — an in-document 条, unless the number belongs to another
        // rule/law (guarded by a preceding name char) or is not present here.
        if let Some((article, end, paragraph, _raw)) = match_article_citation(&chars, i) {
            let preceded_by_name = i > 0 && is_name_char(chars[i - 1]);
            if !preceded_by_name && ctx.self_articles.contains(&article) {
                refs.push(TextRef {
                    start: i,
                    end,
                    target: RefTarget::Article { article, paragraph },
                });
            }
            i = end;
            continue;
        }
        // 第K項 — an in-article 項 (only within this 条's range).
        if let Some((p, end, _raw)) = match_paragraph_citation(&chars, i) {
            let preceded_by_name = i > 0 && is_name_char(chars[i - 1]);
            if !preceded_by_name && p >= 1 && p <= tc.para_count {
                refs.push(TextRef {
                    start: i,
                    end,
                    target: RefTarget::Paragraph { paragraph: p },
                });
            }
            i = end;
            continue;
        }
        i += 1;
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::Enactment;
    use crate::model::{Appendix, AppendixKind, BodyNode, Item, Paragraph, RefTarget, Subitem};

    fn bn(main: u32) -> BranchedNumber {
        BranchedNumber { main, branch: None }
    }

    fn para(number: u32, text: &str) -> Paragraph {
        Paragraph {
            number,
            text: text.to_string(),
            items: Vec::new(),
            refs: Vec::new(),
        }
    }

    fn article(main: u32, paras: Vec<Paragraph>) -> BodyNode {
        BodyNode::Article(Article {
            number: bn(main),
            title: None,
            paragraphs: paras,
            subordinate_rules: Vec::new(),
        })
    }

    fn doc(title: &str, body: Vec<BodyNode>) -> Document {
        Document {
            title: title.to_string(),
            enacted: Enactment {
                date: None,
                rule_number: None,
                raw: String::new(),
            },
            last_amended: None,
            full_amendment_note: None,
            body,
            supplementary_provisions: Vec::new(),
            appendices: Vec::new(),
            authorities: Vec::new(),
        }
    }

    fn vocab() -> Vec<(String, String)> {
        vec![
            ("210001".to_string(), "高知大学学則".to_string()),
            ("210002".to_string(), "高知大学学位規則".to_string()),
            ("220009".to_string(), "高知大学研究生規則".to_string()),
        ]
    }

    // --- Authorities (制定根拠) ---

    #[test]
    fn authority_single_clause() {
        let d = doc(
            "高知大学研究生規則",
            vec![article(
                1,
                vec![para(
                    1,
                    "この規則は、高知大学学則第21条第２項の規定に基づき、研究生に関し必要な事項を定める。",
                )],
            )],
        );
        let a = parse_authorities(&d, &vocab());
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].rule_code.as_deref(), Some("210001"));
        assert_eq!(a[0].article, Some(bn(21)));
        assert_eq!(a[0].paragraph, Some(2));
    }

    #[test]
    fn authority_multiple_clauses_and_external_law() {
        // 学位規則: an out-of-corpus 省令 (第13条, no code) + 学則 two clauses.
        let d = doc(
            "高知大学学位規則",
            vec![article(
                1,
                vec![para(
                    1,
                    "この規則は、学位規則（昭和28年文部省令第９号）第13条、高知大学学則第54条第２項及び第76条第２項の規定に基づき、学位に関し必要な事項を定める。",
                )],
            )],
        );
        let a = parse_authorities(&d, &vocab());
        assert_eq!(a.len(), 3);
        // External 省令: name captured, no code, paren skipped so 第9号 is not parsed.
        assert_eq!(a[0].rule_code, None);
        assert_eq!(a[0].rule_name, "学位規則");
        assert_eq!(a[0].article, Some(bn(13)));
        assert_eq!(a[0].paragraph, None);
        // 学則 第54条第2項 and 第76条第2項.
        assert_eq!(a[1].rule_code.as_deref(), Some("210001"));
        assert_eq!((a[1].article, a[1].paragraph), (Some(bn(54)), Some(2)));
        assert_eq!(a[2].rule_code.as_deref(), Some("210001"));
        assert_eq!((a[2].article, a[2].paragraph), (Some(bn(76)), Some(2)));
    }

    #[test]
    fn authority_absent_when_no_basis_clause() {
        let d = doc(
            "高知大学学則",
            vec![article(
                1,
                vec![para(1, "本学は、教育基本法の精神にのっとり設置する。")],
            )],
        );
        assert!(parse_authorities(&d, &vocab()).is_empty());
    }

    // --- Reverse index + 別に定める resolution ---

    #[test]
    fn separately_provided_resolves_to_child_via_reverse_index() {
        let mut parent = doc(
            "高知大学学則",
            vec![article(
                21,
                vec![
                    para(1, "第一項"),
                    para(2, "研究生に関する事項は、別に定める。"),
                ],
            )],
        );
        let mut child = doc(
            "高知大学研究生規則",
            vec![article(
                1,
                vec![para(
                    1,
                    "この規則は、高知大学学則第21条第２項の規定に基づき定める。",
                )],
            )],
        );
        let v = vocab();
        child.authorities = parse_authorities(&child, &v);
        let docs = vec![
            ("210001".to_string(), parent),
            ("220009".to_string(), child),
        ];
        let index = build_subordinate_index(&docs);

        // Re-take ownership of the parent to annotate it.
        parent = docs.into_iter().next().unwrap().1;
        annotate("210001", &mut parent, &v, &index);

        let art21 = parent
            .all_articles()
            .into_iter()
            .find(|a| a.number == bn(21))
            .unwrap();
        assert_eq!(art21.subordinate_rules, vec!["220009"]);
        // The 別に定め token in 第2項 links to the child.
        let p2 = &art21.paragraphs[1];
        let sep = p2
            .refs
            .iter()
            .find(|r| matches!(r.target, RefTarget::SeparatelyProvided { .. }))
            .expect("a 別に定め marker");
        match &sep.target {
            RefTarget::SeparatelyProvided { rules } => {
                assert_eq!(rules, &vec!["220009".to_string()])
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn separately_provided_unresolved_is_an_empty_marker() {
        let v = vocab();
        let index = build_subordinate_index(&[]);
        let mut d = doc(
            "高知大学学則",
            vec![article(9, vec![para(1, "取扱いについては、別に定める。")])],
        );
        annotate("210001", &mut d, &v, &index);
        let refs = &d.all_articles()[0].paragraphs[0].refs;
        match &refs[0].target {
            RefTarget::SeparatelyProvided { rules } => assert!(rules.is_empty()),
            _ => panic!("expected an unresolved 別に定め marker"),
        }
    }

    // --- Inline structural + named references ---

    #[test]
    fn inline_relative_and_absolute_references() {
        let v = vocab();
        let index = build_subordinate_index(&[]);
        let mut d = doc(
            "高知大学学則",
            vec![
                article(4, vec![para(1, "内容。")]),
                article(
                    5,
                    vec![
                        para(1, "第一項。"),
                        para(2, "前項及び前条並びに第4条の定めによる。"),
                    ],
                ),
            ],
        );
        annotate("210001", &mut d, &v, &index);
        let art5 = &d.all_articles()[1];
        let refs = &art5.paragraphs[1].refs;
        // 前項 → 項1; 前条 → 第4条; 第4条 → 第4条.
        assert!(matches!(
            refs[0].target,
            RefTarget::Paragraph { paragraph: 1 }
        ));
        assert!(matches!(refs[1].target, RefTarget::Article { article, .. } if article == bn(4)));
        assert!(matches!(refs[2].target, RefTarget::Article { article, .. } if article == bn(4)));
        // The offsets slice back to the exact tokens.
        let chars: Vec<char> = art5.paragraphs[1].text.chars().collect();
        assert_eq!(
            chars[refs[0].start..refs[0].end].iter().collect::<String>(),
            "前項"
        );
        assert_eq!(
            chars[refs[1].start..refs[1].end].iter().collect::<String>(),
            "前条"
        );
    }

    #[test]
    fn named_rule_links_but_not_self_and_not_external_article() {
        let v = vocab();
        let index = build_subordinate_index(&[]);
        let mut d = doc(
            "高知大学研究生規則",
            vec![article(
                3,
                vec![para(
                    1,
                    "学位の授与は、高知大学学位規則の定めるところによる。",
                )],
            )],
        );
        annotate("220009", &mut d, &v, &index);
        let refs = &d.all_articles()[0].paragraphs[0].refs;
        assert_eq!(refs.len(), 1);
        assert!(matches!(&refs[0].target, RefTarget::Rule { code } if code == "210002"));
    }

    #[test]
    fn table_reference_resolves_to_existing_appendix() {
        let v = vocab();
        let index = build_subordinate_index(&[]);
        let mut d = doc(
            "高知大学学則",
            vec![article(
                2,
                vec![para(1, "収容定員は、別表第１のとおりとする。")],
            )],
        );
        d.appendices.push(Appendix {
            kind: AppendixKind::Table,
            id: "別表第１".to_string(),
            related_article_raw: String::new(),
            raw_text: String::new(),
            cells: None,
        });
        annotate("210001", &mut d, &v, &index);
        let refs = &d.all_articles()[0].paragraphs[0].refs;
        assert!(
            matches!(&refs[0].target, RefTarget::Table { appendix_id } if appendix_id == "別表第１")
        );
    }

    #[test]
    fn item_and_subitem_text_is_annotated() {
        let v = vocab();
        let index = build_subordinate_index(&[]);
        let item = Item {
            number: 1,
            text: "前項に定める書類".to_string(),
            subitems: vec![Subitem {
                label: "イ".to_string(),
                text: "前項の願書".to_string(),
                refs: Vec::new(),
            }],
            refs: Vec::new(),
        };
        let mut p = para(2, "本文");
        p.items.push(item);
        let mut d = doc("高知大学学則", vec![article(7, vec![p])]);
        annotate("210001", &mut d, &v, &index);
        let art = &d.all_articles()[0];
        assert!(matches!(
            art.paragraphs[0].items[0].refs[0].target,
            RefTarget::Paragraph { paragraph: 1 }
        ));
        assert!(matches!(
            art.paragraphs[0].items[0].subitems[0].refs[0].target,
            RefTarget::Paragraph { paragraph: 1 }
        ));
    }
}
