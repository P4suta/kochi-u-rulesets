use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use clap::Parser;
use kochi_u_rulesets::{extract, markdown, structure};

/// Parse 高知大学 ruleset PDFs into structured JSON + human-readable Markdown.
///
/// Single input with no `--out-dir` honours `-o`/`-m` (the original 学則 workflow).
/// Passing `--out-dir` (or more than one input, or a directory) switches to batch
/// mode: each PDF becomes `<out-dir>/<basename>.json` and `<out-dir>/<basename>.md`.
#[derive(Parser)]
struct Args {
    /// PDFs or directories to parse. Directories are expanded to their `*.pdf`.
    #[arg(default_value = "sources/高知大学学則.pdf")]
    inputs: Vec<PathBuf>,

    /// Batch output directory; enables batch mode. Defaults to `out` when more
    /// than one input is given.
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// Single-input JSON output (ignored in batch mode).
    #[arg(short, long, default_value = "data.json")]
    output: PathBuf,

    /// Single-input Markdown output (ignored in batch mode).
    #[arg(short, long, default_value = "gakusoku.md")]
    markdown: PathBuf,

    /// Write each page's raw extracted text to this directory (debugging / fixture regeneration).
    #[arg(long)]
    dump_raw: Option<PathBuf>,

    /// Exit non-zero if any input fails to parse.
    #[arg(long)]
    strict: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let inputs = expand_inputs(&args.inputs)?;
    if inputs.is_empty() {
        return Err(anyhow!("no PDF inputs found"));
    }

    let batch = args.out_dir.is_some() || inputs.len() > 1;
    if !batch {
        // Backward-compatible single-file mode.
        let stats = convert(
            &inputs[0],
            &args.output,
            &args.markdown,
            args.dump_raw.as_deref(),
        )?;
        println!(
            "{stats} → {}, {}",
            args.output.display(),
            args.markdown.display()
        );
        return Ok(());
    }

    let out_dir = args.out_dir.unwrap_or_else(|| PathBuf::from("out"));
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let mut succeeded = 0usize;
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    for input in &inputs {
        let stem = input.file_stem().unwrap_or_default();
        let json_out = out_dir.join(stem).with_extension("json");
        let md_out = out_dir.join(stem).with_extension("md");

        // catch_unwind guards against a panic inside `pdf-extract` on a malformed
        // PDF, so one bad file never aborts the batch.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            convert(input, &json_out, &md_out, None)
        }));
        match result {
            Ok(Ok(stats)) => {
                succeeded += 1;
                println!("✓ {}: {stats}", display_name(input));
            }
            Ok(Err(e)) => failures.push((input.clone(), format!("{e:#}"))),
            Err(_) => failures.push((input.clone(), "パニック（PDF抽出中）".to_string())),
        }
    }

    println!("\n{succeeded}/{} 成功", inputs.len());
    for (path, reason) in &failures {
        println!("  ✗ {}: {reason}", display_name(path));
    }

    if args.strict && !failures.is_empty() {
        std::process::exit(1);
    }
    Ok(())
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

/// Extracts, parses, and writes one PDF. Returns a short stats line for stdout.
fn convert(
    input: &Path,
    json_out: &Path,
    md_out: &Path,
    dump_raw: Option<&Path>,
) -> anyhow::Result<String> {
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

    let document = structure::parse(&pages)?;
    let json = serde_json::to_string_pretty(&document)?;
    std::fs::write(json_out, json)
        .with_context(|| format!("failed to write {}", json_out.display()))?;
    std::fs::write(md_out, markdown::render(&document))
        .with_context(|| format!("failed to write {}", md_out.display()))?;

    Ok(format!(
        "{} 章, {} 条, {} 附則, {} 別表/様式",
        document.chapters().count(),
        document.all_articles().len(),
        document.supplementary_provisions.len(),
        document.appendices.len(),
    ))
}
