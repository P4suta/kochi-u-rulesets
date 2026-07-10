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

use std::collections::HashMap;

use crate::model::Document;

#[derive(serde::Serialize)]
pub struct Node {
    pub code: String,
    pub name: String,
}

#[derive(serde::Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub articles: Vec<String>,
    pub count: usize,
}

#[derive(serde::Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// A ruleset name is a name-constituent character iff a following occurrence of it
/// would extend the matched span into a longer, different token. Han ideographs and
/// katakana qualify; hiragana (particles) and everything else do not.
fn is_name_char(c: char) -> bool {
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
    candidates.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    // Insertion-ordered edge accumulator: index by (from, to), keep builders in a Vec
    // so first-seen order is preserved and article labels accumulate in document order.
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut builders: Vec<Edge> = Vec::new();

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
                let key = (from.clone(), to.to_string());
                let idx = *index.entry(key).or_insert_with(|| {
                    builders.push(Edge {
                        from: from.clone(),
                        to: to.to_string(),
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
            }],
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
        let d = doc(vec![art(5, "この件は高知大学学生懲戒規則の定めるところによる。")]);
        let g = build_graph(&vocab(), &[("1".to_string(), &d)]);

        assert!(edge(&g, "1", "3").is_some(), "should cite 懲戒規則 (code 3)");
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
    fn nodes_cover_every_vocab_entry() {
        let d = doc(vec![art(1, "内容なし。")]);
        let g = build_graph(&vocab(), &[("9".to_string(), &d)]);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.nodes[0].code, "1");
        assert_eq!(g.nodes[0].name, "高知大学学則");
    }
}
