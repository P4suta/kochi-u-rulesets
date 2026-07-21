//! Coordinate-based reconstruction of 別表 grids.
//!
//! `pdf-extract` emits table text in a jumbled reading order with columns lost,
//! but the glyph x-coordinates still encode the 2D layout. Given the ruby-free
//! glyphs of one appendix region (already grouped into visual rows), this module
//! recovers a column-aligned grid:
//!
//! 1. tokenize each row at an adaptive gap valley (letter-spacing vs column gap),
//! 2. detect columns from cross-row x-coverage valleys (robust to right-aligned
//!    numbers and the odd column-spanning header),
//! 3. assign tokens to columns by x-overlap, drop all-empty columns,
//! 4. gate on confidence — anything that doesn't look like a real grid returns
//!    `None`, and the caller keeps the faithful `raw_text`.
//!
//! Vertical (multi-line) cells are left as separate visual rows: that is a
//! faithful structural view, and merging them into logical rows would invent
//! structure. The output never drops content.

use crate::model::Table;

/// One glyph with its horizontal extent, in PDF user-space units.
#[derive(Debug, Clone)]
pub struct Glyph {
    pub x0: f64,
    pub x1: f64,
    pub text: String,
}

/// A visual row: its glyphs left-to-right.
pub type Row = Vec<Glyph>;

#[derive(Debug, Clone)]
struct Token {
    x0: f64,
    x1: f64,
    text: String,
}

/// Attempts to reconstruct a column-aligned grid from an appendix region's
/// visual rows. Returns `None` when the region doesn't look like a real table.
pub fn reconstruct(rows: &[Row], body: f64) -> Option<Table> {
    if rows.len() < 3 || body <= 0.0 {
        return None;
    }

    let gap_threshold = column_gap_threshold(rows, body)?;
    let token_rows: Vec<Vec<Token>> = rows.iter().map(|r| tokenize(r, gap_threshold)).collect();

    let columns = detect_columns(&token_rows, body)?;
    if columns.len() < 2 || columns.len() > 12 {
        return None;
    }

    // Assign every token to the column it overlaps most.
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(token_rows.len());
    for tokens in &token_rows {
        let mut cells = vec![String::new(); columns.len()];
        for t in tokens {
            if let Some(ci) = best_column(t, &columns) {
                cells[ci].push_str(&t.text);
            }
        }
        grid.push(cells);
    }

    drop_empty_columns(&mut grid);
    // Drop fully-empty leading/trailing rows (blank lines around the region).
    while grid
        .first()
        .is_some_and(|r| r.iter().all(|c| c.trim().is_empty()))
    {
        grid.remove(0);
    }
    while grid
        .last()
        .is_some_and(|r| r.iter().all(|c| c.trim().is_empty()))
    {
        grid.pop();
    }
    if grid.len() < 3 || grid.first().map_or(0, |r| r.len()) < 2 {
        return None;
    }

    // Confidence: most data rows should populate at least two columns, or this
    // is prose that happened to have a stray wide gap, not a table.
    let filled_rows = grid
        .iter()
        .filter(|r| r.iter().filter(|c| !c.trim().is_empty()).count() >= 2)
        .count();
    if (filled_rows as f64) < (grid.len() as f64) * 0.5 {
        return None;
    }

    Some(Table {
        // A GFM table has exactly one header row; treat the first visual row as
        // it (a stacked multi-row header degrades gracefully to data rows).
        header_row_count: 1,
        rows: grid
            .into_iter()
            .map(|r| r.into_iter().map(|c| c.trim().to_string()).collect())
            .collect(),
    })
}

/// The gap (in user-space units) above which a within-row space is a column
/// separator rather than letter-spacing. Found as the valley between the
/// small-gap mass (adjacent glyphs / letter spacing) and the wide column gaps.
/// Returns `None` when there is no clear bimodal split (not a griddable region).
fn column_gap_threshold(rows: &[Row], body: f64) -> Option<f64> {
    // Edge-to-edge gaps between consecutive glyphs, in body units.
    let mut gaps: Vec<f64> = Vec::new();
    for r in rows {
        for w in r.windows(2) {
            let gap = (w[1].x0 - w[0].x1) / body;
            if gap > 0.05 {
                gaps.push(gap);
            }
        }
    }
    if gaps.len() < 3 {
        return None;
    }

    // Histogram in 0.25-body buckets over [0, 4).
    const BUCKET: f64 = 0.25;
    let nbuckets = (4.0 / BUCKET) as usize;
    let mut hist = vec![0usize; nbuckets];
    for &g in &gaps {
        let b = (g / BUCKET) as usize;
        if b < nbuckets {
            hist[b] += 1;
        }
    }
    let peak = *hist.iter().max().unwrap();

    // Walk outward from the intra-token peak; the first near-empty bucket in
    // [0.5, 3.0) body units is the valley separating letter-spacing from columns.
    let lo = (0.5 / BUCKET) as usize;
    let hi = (3.0 / BUCKET) as usize;
    for (b, &count) in hist.iter().enumerate().take(hi.min(nbuckets)).skip(lo) {
        if count <= peak / 20 {
            return Some((b as f64 + 0.5) * BUCKET * body);
        }
    }
    None
}

fn tokenize(row: &Row, gap_threshold: f64) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for g in row {
        match tokens.last_mut() {
            Some(t) if g.x0 - t.x1 <= gap_threshold => {
                t.x1 = g.x1;
                t.text.push_str(&g.text);
            }
            _ => tokens.push(Token {
                x0: g.x0,
                x1: g.x1,
                text: g.text.clone(),
            }),
        }
    }
    tokens
}

/// Column x-intervals, from bins covered by enough rows. A bin belongs to a
/// column when several rows place a token over it; sparse bridges (a lone
/// column-spanning header) fall into the valleys and don't merge columns.
fn detect_columns(token_rows: &[Vec<Token>], body: f64) -> Option<Vec<(f64, f64)>> {
    let (min_x, max_x) = token_rows
        .iter()
        .flatten()
        .fold((f64::MAX, f64::MIN), |(lo, hi), t| {
            (lo.min(t.x0), hi.max(t.x1))
        });
    if !min_x.is_finite() || max_x <= min_x {
        return None;
    }

    const BUCKET: f64 = 0.25;
    let step = BUCKET * body;
    let nbins = ((max_x - min_x) / step).ceil() as usize + 1;
    let mut cover = vec![0usize; nbins];
    for tokens in token_rows {
        // Count each row at most once per bin so a row's width doesn't dominate.
        let mut seen = vec![false; nbins];
        for t in tokens {
            let b0 = ((t.x0 - min_x) / step) as usize;
            let b1 = (((t.x1 - min_x) / step) as usize).min(nbins - 1);
            for (b, seen_b) in seen.iter_mut().enumerate().take(b1 + 1).skip(b0) {
                if !*seen_b {
                    *seen_b = true;
                    cover[b] += 1;
                }
            }
        }
    }

    // A column bin must be covered by at least two rows, so a lone wide token
    // (a footnote spanning the table) can't bridge two columns into one.
    let threshold = 2usize;
    let mut columns: Vec<(f64, f64)> = Vec::new();
    let mut b = 0;
    while b < nbins {
        if cover[b] >= threshold {
            let start = b;
            while b < nbins && cover[b] >= threshold {
                b += 1;
            }
            columns.push((min_x + start as f64 * step, min_x + b as f64 * step));
        } else {
            b += 1;
        }
    }
    Some(columns)
}

fn overlap(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.1.min(b.1) - a.0.max(b.0)).max(0.0)
}

fn best_column(t: &Token, columns: &[(f64, f64)]) -> Option<usize> {
    let tok = (t.x0, t.x1);
    columns
        .iter()
        .enumerate()
        .map(|(i, &c)| (i, overlap(tok, c)))
        .filter(|&(_, ov)| ov > 0.0)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(i, _)| i)
        // A token overlapping no detected column snaps to the nearest one so its
        // content is never dropped.
        .or_else(|| {
            columns
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let da = ((a.1.0 + a.1.1) / 2.0 - (t.x0 + t.x1) / 2.0).abs();
                    let db = ((b.1.0 + b.1.1) / 2.0 - (t.x0 + t.x1) / 2.0).abs();
                    da.partial_cmp(&db).unwrap()
                })
                .map(|(i, _)| i)
        })
}

fn drop_empty_columns(grid: &mut [Vec<String>]) {
    let ncol = grid.first().map_or(0, |r| r.len());
    let keep: Vec<bool> = (0..ncol)
        .map(|i| grid.iter().any(|r| !r[i].trim().is_empty()))
        .collect();
    for row in grid.iter_mut() {
        let mut i = 0;
        row.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a row of glyphs from `(text, x_start)` pairs, each glyph 1.0 wide.
    fn row(cells: &[(&str, f64)]) -> Row {
        let mut r = Row::new();
        for &(text, mut x) in cells {
            for ch in text.chars() {
                r.push(Glyph {
                    x0: x,
                    x1: x + 1.0,
                    text: ch.to_string(),
                });
                x += 1.0;
            }
        }
        r
    }

    #[test]
    fn reconstructs_a_clean_two_column_table() {
        // Two columns separated by a wide (4-unit) gap; letter spacing is tight.
        let body = 1.0;
        let rows = vec![
            row(&[("学部", 0.0), ("定員", 10.0)]),
            row(&[("医学部", 0.0), ("95", 10.0)]),
            row(&[("理学部", 0.0), ("55", 10.0)]),
            row(&[("工学部", 0.0), ("70", 10.0)]),
        ];
        let t = reconstruct(&rows, body).expect("should reconstruct");
        assert_eq!(t.rows.len(), 4);
        assert!(t.rows.iter().all(|r| r.len() == 2));
        assert_eq!(t.rows[1], vec!["医学部", "95"]);
        assert_eq!(t.rows[3], vec!["工学部", "70"]);
        assert_eq!(t.header_row_count, 1);
    }

    #[test]
    fn single_column_region_is_not_a_table() {
        let body = 1.0;
        let rows = vec![
            row(&[("これは様式の一行目", 0.0)]),
            row(&[("二行目のテキスト", 0.0)]),
            row(&[("三行目のテキスト", 0.0)]),
        ];
        assert!(reconstruct(&rows, body).is_none());
    }

    #[test]
    fn right_aligned_numbers_share_a_column() {
        let body = 1.0;
        // Numbers of different widths right-align at x≈13.
        let rows = vec![
            row(&[("科目", 0.0), ("定員", 11.0)]),
            row(&[("甲", 0.0), ("5", 13.0)]),
            row(&[("乙", 0.0), ("120", 11.0)]),
            row(&[("丙", 0.0), ("75", 12.0)]),
        ];
        let t = reconstruct(&rows, body).expect("should reconstruct");
        assert!(t.rows.iter().all(|r| r.len() == 2));
        assert_eq!(t.rows[1], vec!["甲", "5"]);
        assert_eq!(t.rows[2], vec!["乙", "120"]);
    }
}
