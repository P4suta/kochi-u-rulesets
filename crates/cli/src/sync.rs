//! `sync` subcommand — mirror the live 高知大学 規則集 index into `sources/`.
//!
//! Fetches the regulations index page, parses every ruleset PDF link out of the
//! DOM, downloads the PDFs into `sources/<name>.pdf`, and writes a `manifest.json`
//! ledger. The manifest keys each entry by the URL's numeric **code** (a stable
//! identifier), so a rename shows up as a name change and a move as a URL change
//! rather than as a delete-plus-add — the change signal stays clean.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

/// The regulations index that lists every ruleset PDF.
const INDEX_URL: &str = "https://www.kochi-u.ac.jp/education-support/regulations/";
const USER_AGENT: &str = "kochi-u-rulesets sync (+https://github.com/P4suta/kochi-u-rulesets)";
/// Abort if the index yields fewer entries than this — guards against an error
/// page / restructured HTML being read as "everything was removed".
const MIN_ENTRIES: usize = 20;

/// Ruleset codes whose index link is broken upstream, mapped to a working URL.
/// The 規則集 index points 高知大学学章取扱要領 (140007) at a `wp-content/uploads`
/// path that 404s; the canonical `kisoku_syuu` copy is live. Keyed by code, so it
/// applies whether or not the site ever fixes the index link.
const URL_OVERRIDES: &[(&str, &str)] = &[(
    "140007",
    "https://www.kochi-u.ac.jp/JA/kisoku_syuu/pdf/1/140007.pdf",
)];

/// The working URL for `code`: an override when the index link is known-broken,
/// otherwise the URL the index gave.
fn resolve_url(code: &str, index_url: String) -> String {
    URL_OVERRIDES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(index_url, |(_, u)| (*u).to_owned())
}

#[derive(clap::Args)]
pub struct SyncArgs {
    /// Directory to mirror the ruleset PDFs into.
    #[arg(long, default_value = "sources")]
    sources: PathBuf,

    /// Path to the identity ledger written/read across runs.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,
}

/// One ruleset as recorded in `manifest.json`. `code` is the identity key;
/// `name` and `url` are what the site currently offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub code: String,
    pub name: String,
    pub url: String,
}

pub fn run(args: SyncArgs) -> Result<()> {
    let http = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("failed to build HTTP client")?;

    let html = http
        .get(INDEX_URL)
        .send()
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("failed to fetch index {INDEX_URL}"))?
        .text()
        .context("index response was not text")?;

    let entries = parse_index(&html);
    if entries.len() < MIN_ENTRIES {
        bail!(
            "索引ページから {} 件しか抽出できませんでした（最低 {} 件を期待）。\
             ページ構造の変更かエラー応答の可能性があるため、sources/ には一切触れず中断します。",
            entries.len(),
            MIN_ENTRIES
        );
    }

    std::fs::create_dir_all(&args.sources)
        .with_context(|| format!("failed to create {}", args.sources.display()))?;
    let previous = load_manifest(&args.manifest)?;

    let summary = apply(&http, &args.sources, &previous, &entries)?;

    // Persist the ledger last, so an interrupted download leaves the previous
    // manifest intact rather than half-applied state.
    write_manifest(&args.manifest, &entries)?;

    println!(
        "同期完了: {} 追加, {} 更新, {} 改名, {} 削除, {} 取得失敗（計 {} 規則）",
        summary.added,
        summary.updated,
        summary.renamed,
        summary.removed,
        summary.failed,
        entries.len()
    );
    Ok(())
}

/// Tally of what `apply` did, for the stdout summary.
#[derive(Debug, Default)]
struct Summary {
    added: usize,
    updated: usize,
    renamed: usize,
    removed: usize,
    /// Listed on the index but the PDF could not be fetched (e.g. a dead link).
    failed: usize,
}

/// Extract `(code, name, url)` for every ruleset PDF anchor in the index.
///
/// The index wraps each ruleset name in an `<a href="…CODE.pdf"><span
/// class="has-icon…">名前</span></a>`, but the markup is not always clean — one
/// entry ("高知大学外国人留学生規則") is split across *two* anchors to the same
/// PDF, "高" in the first and "知大学外国人留学生規則" in the second. We therefore
/// key by `code` and **concatenate** the anchor texts in document order, which
/// reconstructs the full name without hard-coding the site's CSS classes.
pub fn parse_index(html: &str) -> Vec<Entry> {
    let doc = Html::parse_document(html);
    // `unwrap`: the selector is a compile-time constant, always valid.
    let anchor = Selector::parse("a[href$='.pdf']").unwrap();

    // code → (first url seen, name pieces in document order).
    let mut by_code: BTreeMap<String, (String, String)> = BTreeMap::new();
    for a in doc.select(&anchor) {
        let Some(url) = a.value().attr("href") else {
            continue;
        };
        // Only mirror the university's own regulation PDFs.
        if !url.contains("kochi-u.ac.jp") {
            continue;
        }
        let Some(code) = code_from_url(url) else {
            continue;
        };
        let piece = a.text().collect::<String>();
        let entry = by_code
            .entry(code)
            .or_insert_with(|| (url.to_owned(), String::new()));
        entry.1.push_str(&piece);
    }

    by_code
        .into_iter()
        .filter_map(|(code, (url, name))| {
            let name = name.trim();
            (!name.is_empty()).then(|| Entry {
                url: resolve_url(&code, url),
                code,
                name: name.to_owned(),
            })
        })
        .collect()
}

/// The numeric stem of a PDF URL, e.g. `…/pdf/2/230001.pdf` → `230001` and
/// `…/uploads/2026/01/140007.pdf` → `140007`. Returns `None` for non-numeric
/// stems so unrelated PDFs are skipped rather than keyed to a wrong code.
fn code_from_url(url: &str) -> Option<String> {
    let stem = url.rsplit('/').next()?.strip_suffix(".pdf")?;
    if !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()) {
        Some(stem.to_owned())
    } else {
        None
    }
}

fn load_manifest(path: &Path) -> Result<Vec<Entry>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn write_manifest(path: &Path, entries: &[Entry]) -> Result<()> {
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(path, json + "\n").with_context(|| format!("failed to write {}", path.display()))
}

/// Bring `sources/` in line with `entries`, using `previous` (keyed by code) to
/// detect renames and removals. The index is the source of truth for *identity*;
/// a failed PDF download (a dead link) is warned about and skipped — it never
/// aborts the sync and never deletes a good local file.
fn apply(
    http: &reqwest::blocking::Client,
    sources: &Path,
    previous: &[Entry],
    entries: &[Entry],
) -> Result<Summary> {
    let prev_by_code: BTreeMap<&str, &Entry> =
        previous.iter().map(|e| (e.code.as_str(), e)).collect();
    let now_codes: std::collections::BTreeSet<&str> =
        entries.iter().map(|e| e.code.as_str()).collect();

    let mut summary = Summary::default();

    // Removals: codes present before but gone from the index now (a real removal,
    // independent of any download).
    for prev in previous {
        if !now_codes.contains(prev.code.as_str()) {
            let old = sources.join(format!("{}.pdf", prev.name));
            if old.exists() {
                std::fs::remove_file(&old)
                    .with_context(|| format!("failed to remove {}", old.display()))?;
            }
            summary.removed += 1;
        }
    }

    for entry in entries {
        let dest = sources.join(format!("{}.pdf", entry.name));
        let prev = prev_by_code.get(entry.code.as_str()).copied();

        let wrote = match download_if_changed(http, &entry.url, &dest) {
            Ok(wrote) => wrote,
            Err(e) => {
                // A dead/temporarily-unavailable link: warn, keep any existing
                // file untouched, and move on. The manifest still records the
                // entry so a later run retries.
                eprintln!(
                    "⚠ {}（{}）を取得できませんでした: {e:#}",
                    entry.name, entry.code
                );
                summary.failed += 1;
                continue;
            }
        };

        match prev {
            None => summary.added += 1,
            Some(prev) if prev.name != entry.name => {
                // Renamed upstream and the new content is in hand: drop the file
                // under the old name.
                let old = sources.join(format!("{}.pdf", prev.name));
                if old.exists() && old != dest {
                    std::fs::remove_file(&old)
                        .with_context(|| format!("failed to remove {}", old.display()))?;
                }
                summary.renamed += 1;
            }
            // Existing ruleset, same name: a byte change is an update.
            Some(_) if wrote => summary.updated += 1,
            Some(_) => {}
        }
    }

    Ok(summary)
}

/// Download `url` to `dest`, skipping the write when the bytes are unchanged.
/// Returns `true` when `dest` was created or overwritten.
fn download_if_changed(http: &reqwest::blocking::Client, url: &str, dest: &Path) -> Result<bool> {
    let bytes = http
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("failed to download {url}"))?
        .bytes()
        .with_context(|| format!("failed to read body of {url}"))?;

    if let Ok(existing) = std::fs::read(dest)
        && existing == bytes.as_ref()
    {
        return Ok(false);
    }
    std::fs::write(dest, &bytes).with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/regulations_index.html");

    #[test]
    fn code_from_url_handles_both_url_shapes() {
        assert_eq!(
            code_from_url("https://www.kochi-u.ac.jp/JA/kisoku_syuu/pdf/2/230001.pdf").as_deref(),
            Some("230001")
        );
        assert_eq!(
            code_from_url("https://www.kochi-u.ac.jp/wp-content/uploads/2026/01/140007.pdf")
                .as_deref(),
            Some("140007")
        );
        // Non-numeric stems are not regulation codes.
        assert_eq!(code_from_url("https://example.com/foo/bar.pdf"), None);
    }

    #[test]
    fn parse_index_extracts_all_regulations() {
        let entries = parse_index(FIXTURE);
        // The live index lists 25 distinct rulesets.
        assert_eq!(entries.len(), 25, "expected 25 distinct ruleset codes");
        // Safety floor would pass.
        assert!(entries.len() >= MIN_ENTRIES);

        // Every entry has a numeric code, a non-empty name, and a kochi-u URL.
        for e in &entries {
            assert!(e.code.bytes().all(|b| b.is_ascii_digit()), "code: {e:?}");
            assert!(!e.name.trim().is_empty(), "name: {e:?}");
            assert!(e.url.contains("kochi-u.ac.jp"), "url: {e:?}");
        }
    }

    #[test]
    fn parse_index_reconstructs_name_split_across_anchors() {
        // 外国人留学生規則 (220008) is split across two anchors to the same PDF:
        // "高" then "知大学外国人留学生規則". Concatenation must rebuild the full name.
        let entries = parse_index(FIXTURE);
        let e = entries
            .iter()
            .find(|e| e.code == "220008")
            .expect("220008 present");
        assert_eq!(e.name, "高知大学外国人留学生規則");
    }

    #[test]
    fn parse_index_overrides_broken_140007_link() {
        // The index links 140007 at a `wp-content/uploads` path that 404s; the
        // override must swap in the live `kisoku_syuu` URL.
        let entries = parse_index(FIXTURE);
        let e = entries
            .iter()
            .find(|e| e.code == "140007")
            .expect("140007 present");
        assert_eq!(e.name, "高知大学学章取扱要領");
        assert_eq!(
            e.url,
            "https://www.kochi-u.ac.jp/JA/kisoku_syuu/pdf/1/140007.pdf"
        );
        assert!(!e.url.contains("wp-content"));
    }

    #[test]
    fn parse_index_on_empty_html_is_below_floor() {
        assert!(parse_index("<html><body>no links</body></html>").len() < MIN_ENTRIES);
    }
}
