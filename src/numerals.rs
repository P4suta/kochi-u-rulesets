/// Parses a run of full-width (０-９) and/or half-width (0-9) digits into a `u32`.
/// This document mixes both: chapter/section/branch numbers render full-width,
/// article numbers ≥10 render half-width.
pub fn parse_digits(s: &str) -> Option<u32> {
    let ascii: String = s
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            c => c,
        })
        .collect();
    ascii.parse().ok()
}

/// Strips all whitespace from a heading/title fragment. pdf-extract renders both
/// the marker-to-title separator and short (2-char) titles' letter-spacing as
/// plain ASCII spaces (e.g. "総  則" for 総則) — stripping all whitespace
/// recovers the original title in both cases.
pub fn normalize_title(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fullwidth_digits() {
        assert_eq!(parse_digits("８４"), Some(84));
    }

    #[test]
    fn parses_halfwidth_digits() {
        assert_eq!(parse_digits("10"), Some(10));
    }

    #[test]
    fn strips_letter_spacing() {
        assert_eq!(normalize_title("総  則"), "総則");
    }

    #[test]
    fn strips_marker_separator_only() {
        assert_eq!(normalize_title("収容定員等"), "収容定員等");
    }
}
