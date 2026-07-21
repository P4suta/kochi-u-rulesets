use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand};
use kochi_university_regulations::model::Document;
use kochi_university_regulations::{extract, graph, markdown, references, search};
use serde::Deserialize;

mod commit;
mod sync;

/// Parse 高知大学 ruleset PDFs into structured JSON + human-readable Markdown,
/// and keep the corpus in sync with the university's regulations index.
#[derive(Parser)]
#[command(name = "kochi-university-regulations", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Mirror the live 規則集 index into `sources/` and update `manifest.json`.
    Sync(sync::SyncArgs),
    /// Parse `sources/*.pdf` into the static-site data set under `<out-dir>`.
    Build(BuildArgs),
    /// Commit changed paths to the current branch (signed, via the GitHub API; CI only).
    Commit(commit::CommitArgs),
}

#[derive(clap::Args)]
struct BuildArgs {
    /// PDFs or directories to parse. Directories are expanded to their `*.pdf`.
    #[arg(default_value = "sources")]
    inputs: Vec<PathBuf>,

    /// Output directory. Writes `<out-dir>/docs/<code>.{json,md}` plus the
    /// aggregate `search.idx`, `graph.json`, and `site.json`.
    #[arg(long, default_value = "web/public")]
    out_dir: PathBuf,

    /// Manifest of {code,name,url} entries used to key documents and build the site index.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,

    /// Write each page's raw extracted text to this directory (debugging / fixture regeneration).
    #[arg(long)]
    dump_raw: Option<PathBuf>,

    /// Exit non-zero if any input fails to parse.
    #[arg(long)]
    strict: bool,
}

/// One `manifest.json` entry. `name` equals the document's parsed title.
#[derive(Deserialize, Clone)]
struct ManifestEntry {
    code: String,
    name: String,
    url: String,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Sync(args) => sync::run(args),
        Command::Build(args) => build(args),
        Command::Commit(args) => commit::run(args),
    }
}

fn build(args: BuildArgs) -> anyhow::Result<()> {
    let inputs = expand_inputs(&args.inputs)?;
    if inputs.is_empty() {
        return Err(anyhow!("no PDF inputs found"));
    }

    let manifest_bytes = std::fs::read(&args.manifest)
        .with_context(|| format!("failed to read {}", args.manifest.display()))?;
    let manifest: Vec<ManifestEntry> = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("failed to parse {}", args.manifest.display()))?;

    let docs_dir = args.out_dir.join("docs");
    std::fs::create_dir_all(&docs_dir)
        .with_context(|| format!("failed to create {}", docs_dir.display()))?;

    // Own each parsed document so we can later borrow all of them into the
    // index/graph builders (which take `&[(String, &Document)]`).
    let mut docs: Vec<(String, Document)> = Vec::new();
    let mut succeeded = 0usize;
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    // Titles/stems that parsed but matched no manifest entry — surfaced at the end.
    let mut unmapped: Vec<String> = Vec::new();

    for input in &inputs {
        // catch_unwind guards against a panic inside `pdf-extract` on a malformed
        // PDF, so one bad file never aborts the batch.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            parse_one(input, args.dump_raw.as_deref())
        }));
        let document = match result {
            Ok(Ok(document)) => document,
            Ok(Err(e)) => {
                failures.push((input.clone(), format!("{e:#}")));
                continue;
            }
            Err(_) => {
                failures.push((input.clone(), "パニック（PDF抽出中）".to_string()));
                continue;
            }
        };

        // Key the document by its manifest `code`: title match first (name == title),
        // then a fallback on the input file stem, else skip with a warning.
        let stem = input.file_stem().unwrap_or_default().to_string_lossy();
        let code = manifest
            .iter()
            .find(|e| e.name == document.title)
            .or_else(|| manifest.iter().find(|e| e.name == stem))
            .map(|e| e.code.clone());
        let Some(code) = code else {
            unmapped.push(format!("{} ({})", document.title, display_name(input)));
            failures.push((input.clone(), "manifest にコードなし".to_string()));
            continue;
        };

        succeeded += 1;
        println!("✓ {} → {code}: {}", display_name(input), stats(&document));
        docs.push((code, document));
    }

    // Cross-reference pass, corpus-wide and in dependency order: parse each document's
    // 制定根拠, invert them into the subordinate index, then annotate every document's
    // inline references (別に定める resolves against the index). Must run before the
    // JSON is written and before the graph/search aggregates borrow the documents.
    let vocab: Vec<(String, String)> = manifest
        .iter()
        .map(|e| (e.code.clone(), e.name.clone()))
        .collect();
    for (_code, document) in docs.iter_mut() {
        document.authorities = references::parse_authorities(document, &vocab);
    }
    let sub_index = references::build_subordinate_index(&docs);
    for (code, document) in docs.iter_mut() {
        references::annotate(code, document, &vocab, &sub_index);
    }

    // Now write each document's JSON + Markdown (annotated).
    for (code, document) in &docs {
        let json = serde_json::to_string_pretty(document)?;
        let json_out = docs_dir.join(code).with_extension("json");
        let md_out = docs_dir.join(code).with_extension("md");
        std::fs::write(&json_out, json)
            .with_context(|| format!("failed to write {}", json_out.display()))?;
        std::fs::write(&md_out, markdown::render(document))
            .with_context(|| format!("failed to write {}", md_out.display()))?;
    }

    // Aggregate outputs. `pairs` borrows every owned document.
    let pairs: Vec<(String, &Document)> = docs.iter().map(|(c, d)| (c.clone(), d)).collect();

    let idx = search::build_index(&pairs);
    let idx_out = args.out_dir.join("search.idx");
    std::fs::write(&idx_out, &idx)
        .with_context(|| format!("failed to write {}", idx_out.display()))?;

    let graph_json = serde_json::to_string_pretty(&graph::build_graph(&vocab, &pairs))?;
    let graph_out = args.out_dir.join("graph.json");
    std::fs::write(&graph_out, graph_json)
        .with_context(|| format!("failed to write {}", graph_out.display()))?;

    let by_code: std::collections::HashMap<&str, &Document> =
        docs.iter().map(|(c, d)| (c.as_str(), d)).collect();
    let rulesets: Vec<serde_json::Value> = manifest
        .iter()
        .map(|e| {
            let doc = by_code.get(e.code.as_str());
            let article_count = doc.map_or(0, |d| d.all_articles().len());
            serde_json::json!({
                "code": e.code,
                "name": e.name,
                "sourceUrl": e.url,
                "hasData": article_count > 0,
                "category": classify_category(&e.name),
                "articleCount": article_count,
            })
        })
        .collect();
    let site_json = serde_json::to_string_pretty(&serde_json::json!({ "rulesets": rulesets }))?;
    let site_out = args.out_dir.join("site.json");
    std::fs::write(&site_out, site_json)
        .with_context(|| format!("failed to write {}", site_out.display()))?;

    println!("\n{succeeded}/{} 成功", inputs.len());
    for (path, reason) in &failures {
        println!("  ✗ {}: {reason}", display_name(path));
    }
    if !unmapped.is_empty() {
        println!("\nmanifest に対応しない文書:");
        for name in &unmapped {
            println!("  ? {name}");
        }
    }

    if args.strict && !failures.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

/// Classifies a ruleset by the trailing word of its name into one of the six
/// site categories.
fn classify_category(name: &str) -> &'static str {
    for suffix in ["規則", "規程", "細則", "要領", "準則"] {
        if name.ends_with(suffix) {
            return suffix;
        }
    }
    "その他"
}

/// Expands directories to their `*.pdf` and keeps only `.pdf` files (so a stray
/// `.mhtml` index or other sibling is skipped). Results are sorted for
/// deterministic batch order.
fn expand_inputs(inputs: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for input in inputs {
        if input.is_dir() {
            for entry in std::fs::read_dir(input)
                .with_context(|| format!("failed to read directory {}", input.display()))?
            {
                let path = entry?.path();
                if is_pdf(&path) {
                    out.push(path);
                }
            }
        } else if is_pdf(input) {
            out.push(input.clone());
        }
    }
    out.sort();
    Ok(out)
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Extracts and parses one PDF into a `Document`. Optionally dumps raw page text.
fn parse_one(input: &Path, dump_raw: Option<&Path>) -> anyhow::Result<Document> {
    let pages = extract::extract_pages(input)?;

    if let Some(dir) = dump_raw {
        std::fs::create_dir_all(dir)?;
        for (i, page) in pages.iter().enumerate() {
            std::fs::write(dir.join(format!("page-{:02}.txt", i + 1)), page)?;
        }
    }

    // A scanned PDF with no text layer extracts to whitespace only; report it as
    // a failure rather than emitting an empty document.
    let has_text = pages
        .iter()
        .flat_map(|p| p.chars())
        .any(|c| !c.is_whitespace());
    if !has_text {
        return Err(anyhow!("抽出テキストなし（スキャンPDFの可能性）"));
    }

    extract::parse_pdf(input)
}

/// A short stats line for stdout.
fn stats(document: &Document) -> String {
    let celled = document
        .appendices
        .iter()
        .filter(|a| a.cells.is_some())
        .count();
    format!(
        "{} 章, {} 条, {} 附則, {} 別表/様式（{} セル化）",
        document.chapters().count(),
        document.all_articles().len(),
        document.supplementary_provisions.len(),
        document.appendices.len(),
        celled,
    )
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn cli_definition_is_valid() {
        // Catches clap wiring mistakes (duplicate args, bad defaults) at test time.
        Cli::command().debug_assert();
    }

    #[test]
    fn subcommands_parse() {
        assert!(Cli::try_parse_from(["kochi-university-regulations", "sync"]).is_ok());
        assert!(Cli::try_parse_from(["kochi-university-regulations", "build", "sources"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "kochi-university-regulations",
                "commit",
                "--path",
                "out",
                "--message",
                "m",
            ])
            .is_ok()
        );
        // `commit` requires at least one --path.
        assert!(
            Cli::try_parse_from(["kochi-university-regulations", "commit", "--message", "m"])
                .is_err()
        );
        assert!(Cli::try_parse_from(["kochi-university-regulations"]).is_err());
        assert!(Cli::try_parse_from(["kochi-university-regulations", "bogus"]).is_err());
    }
}
