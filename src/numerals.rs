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

/// True for hiragana, katakana, and CJK ideographs — the scripts of Japanese
/// legal prose, which never separates two such characters with a space.
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}'   // hiragana + katakana
        | '\u{3400}'..='\u{9FFF}' // CJK unified (incl. ext-A)
        | '\u{F900}'..='\u{FAFF}' // CJK compatibility ideographs
    )
}

/// Removes a lone ASCII space sitting between two CJK characters — a residual
/// per-glyph extraction artifact that survives when a line is only lightly
/// letter-spaced. Because Japanese prose never spaces two kanji/kana, this only
/// deletes contamination; a run of two or more spaces (a table/column
/// separator) and any space touching a non-CJK character (dates, ＧＰＡ, law
/// citations) are left untouched.
pub fn normalize_prose(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let drop = c == ' '
            && i > 0
            && i + 1 < chars.len()
            && chars[i - 1] != ' '
            && chars[i + 1] != ' '
            && is_cjk(chars[i - 1])
            && is_cjk(chars[i + 1]);
        if !drop {
            out.push(c);
        }
    }
    out
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

    #[test]
    fn prose_drops_lone_space_between_cjk() {
        assert_eq!(normalize_prose("退学 させられた場合"), "退学させられた場合");
    }

    #[test]
    fn prose_keeps_spaces_around_non_cjk_and_runs() {
        // A space touching a digit/latin glyph is a real separator; a 2-space
        // run is a column separator — both survive.
        assert_eq!(normalize_prose("第 178 号"), "第 178 号");
        assert_eq!(normalize_prose("医学部  660"), "医学部  660");
    }
}
