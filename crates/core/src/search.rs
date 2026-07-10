//! Position-preserving folding full-text search over articles (条 granularity).
//!
//! Build-time encodes a compact index; the same `fold` runs in wasm at query
//! time, so query and haystack normalize identically (no cross-language drift)
//! and match spans map onto the original display text with no recomputation:
//! `fold` maps exactly one char to one char, so a char offset in the folded
//! text is the same char offset in the original.

use anyhow::{bail, Context};

use crate::model::{Article, Document};

/// The weight of a hit in the article's Title field (label + タイトル).
const TITLE_WEIGHT: f32 = 3.0;
/// The weight of a hit in the article's Body field (all prose text).
const BODY_WEIGHT: f32 = 1.0;

/// Target snippet width, in characters, and how many of those characters
/// precede the match. A wider trailing context reads better for legal prose.
const SNIPPET_CHARS: usize = 60;
const SNIPPET_BEFORE: usize = 18;

/// A single ranked search result. This shape is part of the fixed public
/// contract consumed by the frontend and the wasm bridge.
#[derive(Debug, serde::Serialize)]
pub struct Hit {
    /// Owning ruleset code (the stable manifest id).
    pub code: String,
    /// Article label, e.g. "第5条".
    pub article: String,
    /// Article title if any.
    pub title: Option<String>,
    /// A ~60-char window of the ORIGINAL text around the match.
    pub snippet: String,
    /// Highlight start, as a char offset into `snippet`.
    pub hl_start: u32,
    /// Highlight length, in chars.
    pub hl_len: u32,
    pub score: f32,
}

/// Maps a string to its folded form, exactly one char to one char, so char
/// offsets are preserved: U+3000 ideographic space → ASCII space; fullwidth
/// ASCII (U+FF01..=U+FF5E) → its ASCII equivalent; ASCII uppercase →
/// lowercase; everything else unchanged.
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| {
            // Width/space-fold first, then lowercase unconditionally. This order
            // matters: fullwidth 'Ａ' must first become ASCII 'A' and then 'a'.
            // `to_ascii_lowercase` is a no-op on non-ASCII, so each char still
            // maps to exactly one char.
            match c {
                '\u{3000}' => ' ',
                '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                other => other,
            }
            .to_ascii_lowercase()
        })
        .collect()
}

/// Concatenates every prose field of an article — paragraph, item, and subitem
/// text — into one Body field, newline-joined so distinct clauses stay legible
/// in a snippet. Only `.text` is taken (not item numbers or subitem labels),
/// matching the Body definition of the search contract.
fn collect_body(article: &Article) -> String {
    let mut pieces: Vec<&str> = Vec::new();
    for para in &article.paragraphs {
        pieces.push(&para.text);
        for item in &para.items {
            pieces.push(&item.text);
            for sub in &item.subitems {
                pieces.push(&sub.text);
            }
        }
    }
    pieces.join("\n")
}

/// One field of one article, kept as both its folded form (for matching) and
/// its original form (for snippet extraction).
struct Field {
    folded: String,
    original: String,
    weight: f32,
}

impl Field {
    fn new(original: String, weight: f32) -> Self {
        Field {
            folded: fold(&original),
            original,
            weight,
        }
    }

    /// Non-overlapping occurrences of `needle` (already folded) in this field.
    fn count(&self, needle: &str) -> usize {
        self.folded.matches(needle).count()
    }
}

/// One searchable article: its identity plus its two weighted fields.
struct Entry {
    code: String,
    article: String,
    title: Option<String>,
    title_field: Field,
    body_field: Field,
}

/// The deserialized, query-ready index.
pub struct SearchIndex {
    entries: Vec<Entry>,
}

/// Builds a serialized index from `(code, &Document)` pairs. The granularity is
/// the article: every article of every document becomes one entry.
pub fn build_index(docs: &[(String, &Document)]) -> Vec<u8> {
    let mut records: Vec<Record> = Vec::new();
    for (code, doc) in docs {
        for article in doc.all_articles() {
            records.push(Record {
                code: code.clone(),
                article: article.number.labeled('条'),
                title: article.title.clone(),
                body: collect_body(article),
            });
        }
    }
    encode(&records)
}

impl SearchIndex {
    /// Decodes an index produced by [`build_index`]. The folded fields are
    /// recomputed from the stored originals so the two forms can never drift.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let records = decode(bytes)?;
        let entries = records
            .into_iter()
            .map(|r| {
                // The Title field is the label followed by the title text, so a
                // query for either the article number or its name scores it.
                let title_original = match &r.title {
                    Some(t) => format!("{} {}", r.article, t),
                    None => r.article.clone(),
                };
                Entry {
                    title_field: Field::new(title_original, TITLE_WEIGHT),
                    body_field: Field::new(r.body, BODY_WEIGHT),
                    code: r.code,
                    article: r.article,
                    title: r.title,
                }
            })
            .collect();
        Ok(SearchIndex { entries })
    }

    /// The number of distinct documents represented in the index.
    pub fn doc_count(&self) -> usize {
        let mut codes: Vec<&str> = self.entries.iter().map(|e| e.code.as_str()).collect();
        codes.sort_unstable();
        codes.dedup();
        codes.len()
    }

    /// Ranked hits for `q`, best first, capped at `limit`. Score is the weighted
    /// sum of occurrence counts across the two fields; ties break by
    /// `(code, article)` for a fully deterministic order.
    pub fn query(&self, q: &str, limit: usize) -> Vec<Hit> {
        let needle = fold(q);
        if needle.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut hits: Vec<Hit> = Vec::new();
        for entry in &self.entries {
            let title_count = entry.title_field.count(&needle);
            let body_count = entry.body_field.count(&needle);
            let score =
                entry.title_field.weight * title_count as f32 + entry.body_field.weight * body_count as f32;
            if score <= 0.0 {
                continue;
            }

            // Snippet comes from the first field (Title, then Body) that matches.
            let field = if title_count > 0 {
                &entry.title_field
            } else {
                &entry.body_field
            };
            let (snippet, hl_start, hl_len) = snippet_for(field, &needle);

            hits.push(Hit {
                code: entry.code.clone(),
                article: entry.article.clone(),
                title: entry.title.clone(),
                snippet,
                hl_start,
                hl_len,
                score,
            });
        }

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.code.cmp(&b.code))
                .then_with(|| a.article.cmp(&b.article))
        });
        hits.truncate(limit);
        hits
    }
}

/// Extracts a snippet of the original text around the first occurrence of
/// `needle` in `field`, returning `(snippet, hl_start, hl_len)` where the
/// highlight offsets are char offsets into the snippet. The byte position of
/// the match in the folded text converts to a char index that is equally valid
/// in the original, because folding preserves char positions.
fn snippet_for(field: &Field, needle: &str) -> (String, u32, u32) {
    let byte_pos = field
        .folded
        .find(needle)
        .expect("caller only extracts a snippet for a field that matched");
    let match_char = field.folded[..byte_pos].chars().count();
    let match_len = needle.chars().count();

    let chars: Vec<char> = field.original.chars().collect();
    let start = match_char.saturating_sub(SNIPPET_BEFORE);
    let end = (start + SNIPPET_CHARS).min(chars.len());

    let snippet: String = chars[start..end].iter().collect();
    let hl_start = (match_char - start) as u32;
    // Clamp the highlight to the snippet's right edge for long matches.
    let hl_len = (match_len.min(end - match_char)) as u32;
    (snippet, hl_start, hl_len)
}

// --- Binary format --------------------------------------------------------
//
// A flat, self-describing container: 4-byte magic "KRX1", 1-byte version, a
// u32 record count, then each record as four length-prefixed UTF-8 chunks
// (code, article, title, body). The title chunk is length-prefixed like the
// rest, with a leading presence byte to distinguish `None` from `Some("")`.
// Only the originals are stored; folded forms are recomputed on decode.

const MAGIC: &[u8; 4] = b"KRX1";
const VERSION: u8 = 1;

/// A stored article record — the on-disk projection of one [`Entry`].
struct Record {
    code: String,
    article: String,
    title: Option<String>,
    body: String,
}

fn push_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

fn encode(records: &[Record]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.push(VERSION);
    buf.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for r in records {
        push_str(&mut buf, &r.code);
        push_str(&mut buf, &r.article);
        match &r.title {
            Some(t) => {
                buf.push(1);
                push_str(&mut buf, t);
            }
            None => buf.push(0),
        }
        push_str(&mut buf, &r.body);
    }
    buf
}

/// A forward-only cursor over the index bytes, with bounds-checked reads.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> anyhow::Result<&'a [u8]> {
        let end = self.pos.checked_add(n).context("index length overflow")?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .context("index truncated")?;
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> anyhow::Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u8(&mut self) -> anyhow::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn string(&mut self) -> anyhow::Result<String> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        Ok(std::str::from_utf8(bytes)
            .context("index holds invalid UTF-8")?
            .to_owned())
    }
}

fn decode(bytes: &[u8]) -> anyhow::Result<Vec<Record>> {
    let mut r = Reader { bytes, pos: 0 };
    if r.take(4)? != MAGIC {
        bail!("not a KRX index (bad magic)");
    }
    let version = r.u8()?;
    if version != VERSION {
        bail!("unsupported KRX index version {version}");
    }
    let count = r.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let code = r.string()?;
        let article = r.string()?;
        let title = if r.u8()? == 1 {
            Some(r.string()?)
        } else {
            None
        };
        let body = r.string()?;
        records.push(Record {
            code,
            article,
            title,
            body,
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::Enactment;
    use crate::model::{BodyNode, BranchedNumber, Item, Paragraph, Subitem};

    fn enactment() -> Enactment {
        Enactment {
            date: None,
            rule_number: None,
            raw: String::new(),
        }
    }

    fn article(main: u32, title: Option<&str>, paras: Vec<Paragraph>) -> Article {
        Article {
            number: BranchedNumber { main, branch: None },
            title: title.map(str::to_owned),
            paragraphs: paras,
        }
    }

    fn para(text: &str) -> Paragraph {
        Paragraph {
            number: 1,
            text: text.to_owned(),
            items: Vec::new(),
        }
    }

    fn doc(articles: Vec<Article>) -> Document {
        Document {
            title: "テスト規則".to_owned(),
            enacted: enactment(),
            last_amended: None,
            full_amendment_note: None,
            body: articles.into_iter().map(BodyNode::Article).collect(),
            supplementary_provisions: Vec::new(),
            appendices: Vec::new(),
        }
    }

    #[test]
    fn fold_preserves_char_count() {
        let mixed = "ＡｂＣ　１２３！漢字ｶﾅ AbC xyz";
        assert_eq!(fold(&mixed).chars().count(), mixed.chars().count());
    }

    #[test]
    fn fold_maps_fullwidth_and_space_and_case() {
        assert_eq!(fold("ＡｂＣ１２３！　"), "abc123! ");
        // CJK, hiragana, katakana pass through untouched.
        assert_eq!(fold("漢字ひらカナ"), "漢字ひらカナ");
    }

    #[test]
    fn item_and_subitem_text_is_searchable() {
        let d = doc(vec![article(
            1,
            Some("目的"),
            vec![Paragraph {
                number: 1,
                text: "この規則は次のとおりとする。".to_owned(),
                items: vec![Item {
                    number: 1,
                    text: "学生の在籍に関すること".to_owned(),
                    subitems: vec![Subitem {
                        label: "イ".to_owned(),
                        text: "休学の取扱い".to_owned(),
                    }],
                }],
            }],
        )]);
        let index = SearchIndex::from_bytes(&build_index(&[("100".to_owned(), &d)])).unwrap();
        assert_eq!(index.query("休学", 10).len(), 1);
    }

    #[test]
    fn query_returns_expected_article_with_snippet_and_highlight() {
        let d = doc(vec![
            article(3, Some("入学"), vec![para("入学を志願する者は所定の手続を行う。")]),
            article(
                5,
                Some("退学"),
                vec![para("学生が退学しようとするときは、学長の許可を受けなければならない。")],
            ),
        ]);
        let index = SearchIndex::from_bytes(&build_index(&[("42".to_owned(), &d)])).unwrap();

        let hits = index.query("退学", 10);
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit.code, "42");
        assert_eq!(hit.article, "第5条");
        assert_eq!(hit.title.as_deref(), Some("退学"));
        // "退学" hits both the Title field (once) and the Body field (once).
        assert_eq!(hit.score, TITLE_WEIGHT + BODY_WEIGHT);

        // The highlight span, sliced out of the snippet, is the query itself.
        let s: String = hit
            .snippet
            .chars()
            .skip(hit.hl_start as usize)
            .take(hit.hl_len as usize)
            .collect();
        assert_eq!(s, "退学");
    }

    #[test]
    fn body_snippet_highlights_the_match() {
        let long =
            "第一段落の前置きがしばらく続いた後に重要語が現れてさらに文章が続いていく。";
        let d = doc(vec![article(1, Some("総則"), vec![para(long)])]);
        let index = SearchIndex::from_bytes(&build_index(&[("1".to_owned(), &d)])).unwrap();

        let hits = index.query("重要語", 10);
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        let highlighted: String = hit
            .snippet
            .chars()
            .skip(hit.hl_start as usize)
            .take(hit.hl_len as usize)
            .collect();
        assert_eq!(highlighted, "重要語");
        assert!(hit.snippet.chars().count() <= SNIPPET_CHARS);
    }

    #[test]
    fn ranking_is_by_score_then_deterministic() {
        // Article 2 mentions 学生 three times in its body; article 1 once.
        let a1 = article(1, None, vec![para("学生について定める。")]);
        let a2 = article(
            2,
            None,
            vec![para("学生と学生の関係、学生の権利について定める。")],
        );
        let d = doc(vec![a1, a2]);
        let index = SearchIndex::from_bytes(&build_index(&[("7".to_owned(), &d)])).unwrap();

        let hits = index.query("学生", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].article, "第2条");
        assert!(hits[0].score > hits[1].score);

        // Ordering is stable across repeated queries.
        let again = index.query("学生", 10);
        let order: Vec<&str> = hits.iter().map(|h| h.article.as_str()).collect();
        let order_again: Vec<&str> = again.iter().map(|h| h.article.as_str()).collect();
        assert_eq!(order, order_again);
    }

    #[test]
    fn ties_break_by_code_then_article() {
        // Two documents, one match each — equal score, so code decides.
        let da = doc(vec![article(9, Some("試験"), vec![para("本文。")])]);
        let db = doc(vec![article(1, Some("試験"), vec![para("本文。")])]);
        let bytes = build_index(&[("200".to_owned(), &da), ("100".to_owned(), &db)]);
        let index = SearchIndex::from_bytes(&bytes).unwrap();

        let hits = index.query("試験", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].code, "100");
        assert_eq!(hits[1].code, "200");
    }

    #[test]
    fn empty_query_returns_nothing() {
        let d = doc(vec![article(1, Some("目的"), vec![para("本文。")])]);
        let index = SearchIndex::from_bytes(&build_index(&[("1".to_owned(), &d)])).unwrap();
        assert!(index.query("", 10).is_empty());
        assert!(index.query("該当なし", 10).is_empty());
    }

    #[test]
    fn index_round_trips_and_counts_documents() {
        let d1 = doc(vec![article(1, Some("目的"), vec![para("あ。")])]);
        let d2 = doc(vec![
            article(1, None, vec![para("い。")]),
            article(2, Some("定義"), vec![para("う。")]),
        ]);
        let bytes = build_index(&[("1".to_owned(), &d1), ("2".to_owned(), &d2)]);
        let index = SearchIndex::from_bytes(&bytes).unwrap();
        assert_eq!(index.doc_count(), 2);
        assert_eq!(index.entries.len(), 3);
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        assert!(SearchIndex::from_bytes(b"XXXX\x01\x00\x00\x00\x00").is_err());
        assert!(SearchIndex::from_bytes(b"KRX").is_err());
    }
}
