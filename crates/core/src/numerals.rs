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

/// True for half- and full-width decimal digits.
fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '０'..='９')
}

/// Removes a lone ASCII space that is a residual per-glyph extraction artifact.
/// pdf-extract sprinkles single spaces between adjacent glyphs on lightly
/// letter-spaced lines, contaminating both prose ("退学 させ") and inline
/// citations ("第 10 条", "昭和 35 年法律第 105 号"). A space is dropped only when
/// both neighbors are CJK or a digit and at least one is CJK — so numbers stay
/// glued to their 年/月/条 counters while genuine separators survive: a run of
/// two or more spaces (a table column) and any space touching Latin/ＧＰＡ are
/// left untouched.
pub fn normalize_prose(text: &str) -> String {
    fn cjk_or_digit(c: char) -> bool {
        is_cjk(c) || is_digit(c)
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let drop = c == ' '
            && i > 0
            && i + 1 < chars.len()
            && chars[i - 1] != ' '
            && chars[i + 1] != ' '
            && cjk_or_digit(chars[i - 1])
            && cjk_or_digit(chars[i + 1])
            && (is_cjk(chars[i - 1]) || is_cjk(chars[i + 1]));
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
    fn prose_glues_digits_to_their_cjk_counters() {
        // A lone space between a digit and a CJK counter is extraction noise
        // ("第 178 号" → "第178号", "平成 18 年" → "平成18年").
        assert_eq!(normalize_prose("第 178 号"), "第178号");
        assert_eq!(
            normalize_prose("平成 18 年法律第 105 号"),
            "平成18年法律第105号"
        );
    }

    #[test]
    fn prose_keeps_real_separators() {
        // A 2-space run is a column separator; a space touching Latin/ＧＰＡ is
        // meaningful — both survive.
        assert_eq!(normalize_prose("医学部  660"), "医学部  660");
        assert_eq!(normalize_prose("ＧＰＡ 3.0"), "ＧＰＡ 3.0");
    }
}
