//! Cross-ruleset reference graph: which regulation cites which.
//!
//! The vocabulary is the manifest's 25 ruleset names. For every source document we
//! scan each article's prose and detect where another ruleset is mentioned by name,
//! recording an edge tagged with the citing article's label.
//!
//! Disambiguation is longest-match-first: names nest (高知大学学生準則 vs
//! 高知大学学生懲戒規則 share the prefix 高知大学学生), so at any position the
//! longest candidate wins. A trailing maximal-token guard then drops a match whose
//! following character is itself a name-constituent CJK/カタカナ character — that
//! would mean the matched name is only a prefix of a longer, different token.
//!
//! The guard favours precision over recall (the task's explicit v1 stance: "prefer
//! dropping ambiguous matches"). Hiragana is intentionally *not* treated as a
//! name-constituent, so common continuations like の/を/に/は after a citation pass.
//! The unavoidable cost is that a name directly followed by a Han character — a
//! conjunction (…学則及び…) or an article pointer (…規則第５条) — is dropped. That is
//! acceptable for v1; no exception list is added, keeping the guard simple.
//!
//! External laws (学校教育法 etc.) are out of vocabulary and are simply never
//! matched. Self-references (from == to) are excluded.
//!
//! Two kinds of edge, richest first:
//!   * **authority** (委任) — the 制定根拠 backbone, parent → child. Derived from each
//!     document's parsed `authorities`, so the delegation hierarchy rooted at 学則 is
//!     explicit even though the citation "学則第21条" is dropped by the name scan's
//!     maximal-token guard. This is where 「別に定める」 lands on the graph.
//!   * **reference** (参照) — a plain name mention, from → to, for cross-references
//!     that are *not* an enactment basis. A pair already joined by an authority edge
//!     (either direction) is suppressed here, so the two layers never restate each
//!     other.

use std::collections::{HashMap, HashSet};

use crate::model::Document;

#[derive(serde::Serialize)]
pub struct Node {
    pub code: String,
    pub name: String,
}

/// How two rulesets relate. Serialized snake_case (`"authority"` / `"reference"`).
#[derive(serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// 制定根拠: `from` (parent) delegates to `to` (child) — the 別に定める hierarchy.
    Authority,
    /// A plain by-name citation: `from` mentions `to`.
    Reference,
}

#[derive(serde::Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    /// For an authority edge, the parent 条 the delegation sits in ("第21条"); for a
    /// reference edge, the citing 条 in `from`.
    pub articles: Vec<String>,
    pub count: usize,
}

/// An order-independent key for a pair of codes, so an authority link suppresses a
/// reference in either direction.
fn unordered(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

#[derive(serde::Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// A ruleset name is a name-constituent character iff a following occurrence of it
/// would extend the matched span into a longer, different token. Han ideographs and
/// katakana qualify; hiragana (particles) and everything else do not.
pub(crate) fn is_name_char(c: char) -> bool {
    matches!(c,
        '\u{3005}'                 // 々 iteration mark
        | '\u{3400}'..='\u{4DBF}'  // CJK Unified Ideographs Extension A
        | '\u{4E00}'..='\u{9FFF}'  // CJK Unified Ideographs
        | '\u{F900}'..='\u{FAFF}'  // CJK Compatibility Ideographs
        | '\u{30A0}'..='\u{30FF}'  // Katakana (incl. ・ー)
    )
}

/// Concatenates an article's prose — paragraphs, then their 号 items and イロハ
/// subitems — separated by newlines. The article title and 附則/別表 are excluded by
/// design. The newline separators also guarantee a non-Han character trails a name
/// that ends a prose piece, so such a name still satisfies the maximal-token guard.
fn article_prose(article: &crate::model::Article) -> String {
    let mut out = String::new();
    for paragraph in &article.paragraphs {
        out.push_str(&paragraph.text);
        out.push('\n');
        for item in &paragraph.items {
            out.push_str(&item.text);
            out.push('\n');
            for subitem in &item.subitems {
                out.push_str(&subitem.text);
                out.push('\n');
            }
        }
    }
    out
}

/// Builds the cross-reference graph. `vocab` supplies the node set — `(code, name)`
/// for all 25 rulesets. `docs` supplies the parseable sources — `(code, &Document)`.
pub fn build_graph(vocab: &[(String, String)], docs: &[(String, &Document)]) -> Graph {
    let nodes: Vec<Node> = vocab
        .iter()
        .map(|(code, name)| Node {
            code: code.clone(),
            name: name.clone(),
        })
        .collect();

    // Candidate names as char vectors, longest first so the longest match wins at
    // any scan position. Each carries the target code it resolves to.
    let mut candidates: Vec<(Vec<char>, &str)> = vocab
        .iter()
        .map(|(code, name)| (name.chars().collect::<Vec<char>>(), code.as_str()))
        .filter(|(chars, _)| !chars.is_empty())
        .collect();
    candidates.sort_by_key(|(chars, _)| std::cmp::Reverse(chars.len()));

    // Insertion-ordered edge accumulator: index by (from, to), keep builders in a Vec
    // so first-seen order is preserved and article labels accumulate in document order.
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut builders: Vec<Edge> = Vec::new();

    // 1. Authority (委任) edges — parent → child, from each document's 制定根拠.
    // `auth_pairs` records the unordered pairs so the reference pass can skip them.
    let mut auth_pairs: HashSet<(String, String)> = HashSet::new();
    for (child, doc) in docs {
        for a in &doc.authorities {
            let Some(parent) = a.rule_code.as_deref() else {
                continue;
            };
            if parent == child.as_str() {
                continue;
            }
            let key = (parent.to_string(), child.clone());
            let idx = *index.entry(key).or_insert_with(|| {
                builders.push(Edge {
                    from: parent.to_string(),
                    to: child.clone(),
                    kind: EdgeKind::Authority,
                    articles: Vec::new(),
                    count: 0,
                });
                builders.len() - 1
            });
            if let Some(label) = a.article.map(|n| n.labeled('条'))
                && !builders[idx].articles.contains(&label)
            {
                builders[idx].articles.push(label);
            }
            auth_pairs.insert(unordered(parent, child));
        }
    }

    // 2. Reference (参照) edges — name mentions, minus pairs already joined above.
    for (from, doc) in docs {
        for article in doc.all_articles() {
            let prose: Vec<char> = article_prose(article).chars().collect();
            let label = article.number.labeled('条');

            // Collect the distinct targets cited in this one article. Using a set
            // means a name appearing several times in the article yields the label
            // only once per (from, to).
            let mut hits: Vec<&str> = Vec::new();
            let mut i = 0usize;
            while i < prose.len() {
                let mut matched: Option<(usize, &str)> = None;
                for (name, code) in &candidates {
                    let end = i + name.len();
                    if end > prose.len() {
                        continue;
                    }
                    if prose[i..end] != name[..] {
                        continue;
                    }
                    // Maximal-token guard: reject when the very next character is a
                    // name-constituent char, i.e. the name is a strict prefix here.
                    if prose.get(end).is_some_and(|&c| is_name_char(c)) {
                        continue;
                    }
                    matched = Some((name.len(), *code));
                    break;
                }
                match matched {
                    Some((len, code)) => {
                        if code != from.as_str() && !hits.contains(&code) {
                            hits.push(code);
                        }
                        i += len;
                    }
                    None => i += 1,
                }
            }

            for to in hits {
                // Skip a mention when the pair is already an authority (委任) edge —
                // the two layers should never restate the same relationship.
                if auth_pairs.contains(&unordered(from, to)) {
                    continue;
                }
                let key = (from.clone(), to.to_string());
                let idx = *index.entry(key).or_insert_with(|| {
                    builders.push(Edge {
                        from: from.clone(),
                        to: to.to_string(),
                        kind: EdgeKind::Reference,
                        articles: Vec::new(),
                        count: 0,
                    });
                    builders.len() - 1
                });
                let edge = &mut builders[idx];
                if !edge.articles.contains(&label) {
                    edge.articles.push(label.clone());
                }
            }
        }
    }

    for edge in &mut builders {
        edge.count = edge.articles.len();
    }

    Graph {
        nodes,
        edges: builders,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::Enactment;
    use crate::model::{Article, BodyNode, BranchedNumber, Document, Paragraph};

    fn art(main: u32, text: &str) -> BodyNode {
        BodyNode::Article(Article {
            number: BranchedNumber { main, branch: None },
            title: None,
            paragraphs: vec![Paragraph {
                number: 1,
                text: text.to_string(),
                items: Vec::new(),
                refs: Vec::new(),
            }],
            subordinate_rules: Vec::new(),
        })
    }

    fn doc(articles: Vec<BodyNode>) -> Document {
        Document {
            title: "src".to_string(),
            enacted: Enactment {
                date: None,
                rule_number: None,
                raw: String::new(),
            },
            last_amended: None,
            full_amendment_note: None,
            body: articles,
            supplementary_provisions: Vec::new(),
            appendices: Vec::new(),
            authorities: Vec::new(),
        }
    }

    fn vocab() -> Vec<(String, String)> {
        vec![
            ("1".to_string(), "高知大学学則".to_string()),
            ("2".to_string(), "高知大学学生準則".to_string()),
            ("3".to_string(), "高知大学学生懲戒規則".to_string()),
        ]
    }

    fn edge<'a>(g: &'a Graph, from: &str, to: &str) -> Option<&'a Edge> {
        g.edges.iter().find(|e| e.from == from && e.to == to)
    }

    #[test]
    fn nested_name_longest_match_picks_the_correct_target() {
        // The prose contains the longer nested name; the shorter 高知大学学生準則
        // must NOT be matched inside 高知大学学生懲戒規則. A hiragana particle
        // follows so the maximal-token guard accepts the (correct) match.
        let d = doc(vec![art(
            5,
            "この件は高知大学学生懲戒規則の定めるところによる。",
        )]);
        let g = build_graph(&vocab(), &[("1".to_string(), &d)]);

        assert!(
            edge(&g, "1", "3").is_some(),
            "should cite 懲戒規則 (code 3)"
        );
        assert!(edge(&g, "1", "2").is_none(), "must not cite 準則 (code 2)");
        assert_eq!(edge(&g, "1", "3").unwrap().articles, vec!["第5条"]);
    }

    #[test]
    fn self_reference_is_excluded() {
        // Source is code 1 (高知大学学則); its own article mentions itself.
        let d = doc(vec![art(1, "高知大学学則に基づき、次のとおり定める。")]);
        let g = build_graph(&vocab(), &[("1".to_string(), &d)]);
        assert!(edge(&g, "1", "1").is_none());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn out_of_vocab_law_produces_no_edge() {
        let d = doc(vec![art(2, "学校教育法及び関係法令の定めるところによる。")]);
        let g = build_graph(&vocab(), &[("1".to_string(), &d)]);
        assert!(g.edges.is_empty());
    }

    #[test]
    fn edge_dedup_accumulates_article_labels_in_document_order() {
        let d = doc(vec![
            art(3, "高知大学学生準則の定めによる。"),
            art(7, "再び高知大学学生準則を参照する。"),
            art(9, "無関係な条文。"),
        ]);
        let g = build_graph(&vocab(), &[("1".to_string(), &d)]);

        let e = edge(&g, "1", "2").expect("edge to 準則");
        assert_eq!(e.articles, vec!["第3条", "第7条"]);
        assert_eq!(e.count, 2);
        // A single merged edge, not one per citing article.
        assert_eq!(g.edges.iter().filter(|x| x.to == "2").count(), 1);
    }

    #[test]
    fn authority_edge_forms_hierarchy_and_suppresses_duplicate_mention() {
        use crate::model::Authority;
        // Child (code 3) is enacted under 学則 (code 1) 第5条第2項, and *also* mentions
        // 学則 by name in its body. The 委任 edge must appear parent→child, and the
        // redundant name mention (child→parent) must be suppressed.
        let mut child = doc(vec![art(1, "この件は高知大学学則の定めるところによる。")]);
        child.authorities = vec![Authority {
            rule_code: Some("1".to_string()),
            rule_name: "高知大学学則".to_string(),
            article: Some(BranchedNumber {
                main: 5,
                branch: None,
            }),
            paragraph: Some(2),
            raw: "第5条第2項".to_string(),
        }];
        let g = build_graph(&vocab(), &[("3".to_string(), &child)]);

        assert_eq!(g.edges.len(), 1, "the mention is folded into the 委任 edge");
        let e = &g.edges[0];
        assert_eq!((e.from.as_str(), e.to.as_str()), ("1", "3"));
        assert!(matches!(e.kind, EdgeKind::Authority));
        assert_eq!(e.articles, vec!["第5条"]);
    }

    #[test]
    fn nodes_cover_every_vocab_entry() {
        let d = doc(vec![art(1, "内容なし。")]);
        let g = build_graph(&vocab(), &[("9".to_string(), &d)]);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.nodes[0].code, "1");
        assert_eq!(g.nodes[0].name, "高知大学学則");
    }
}
