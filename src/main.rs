use std::path::PathBuf;

use clap::Parser;
use kochi_u_rulesets::{extract, structure};

/// Extract structured JSON from 高知大学学則.pdf.
#[derive(Parser)]
struct Args {
    /// PDF to parse.
    #[arg(default_value = "高知大学学則.pdf")]
    input: PathBuf,

    /// Where to write the structured JSON.
    #[arg(short, long, default_value = "data.json")]
    output: PathBuf,

    /// Write each page's raw extracted text to this directory (debugging / fixture regeneration).
    #[arg(long)]
    dump_raw: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let pages = extract::extract_pages(&args.input)?;

    if let Some(dir) = &args.dump_raw {
        std::fs::create_dir_all(dir)?;
        for (i, page) in pages.iter().enumerate() {
            std::fs::write(dir.join(format!("page-{:02}.txt", i + 1)), page)?;
        }
    }

    let document = structure::parse(&pages)?;
    let json = serde_json::to_string_pretty(&document)?;
    std::fs::write(&args.output, json)?;

    println!(
        "{} chapters, {} articles, {} 附則, {} 別表 → {}",
        document.chapters.len(),
        document.article_numbers().len(),
        document.supplementary_provisions.len(),
        document.appended_tables.len(),
        args.output.display()
    );

    Ok(())
}
