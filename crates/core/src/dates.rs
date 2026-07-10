//! Parsing of Japanese era dates ("平成20年３月26日") and rule numbers
//! ("規則第74号") out of the raw preamble/附則 text into structured values.
//!
//! Every parser here is lossy-tolerant: it returns `None`/`Unspecified` rather
//! than panicking, and the callers keep the original raw string alongside the
//! structured fields so nothing is ever silently dropped.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::numerals::parse_digits;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Era {
    Meiji,
    Taisho,
    Showa,
    Heisei,
    Reiwa,
}

impl Era {
    fn from_kanji(s: &str) -> Option<Era> {
        Some(match s {
            "明治" => Era::Meiji,
            "大正" => Era::Taisho,
            "昭和" => Era::Showa,
            "平成" => Era::Heisei,
            "令和" => Era::Reiwa,
            _ => return None,
        })
    }

    /// The Gregorian year immediately before this era's year 1, so that
    /// `gregorian = base + era_year` (令和 base 2018 → 令和元年 = 2019).
    fn gregorian_base(self) -> u32 {
        match self {
            Era::Meiji => 1867,
            Era::Taisho => 1911,
            Era::Showa => 1925,
            Era::Heisei => 1988,
            Era::Reiwa => 2018,
        }
    }
}

/// A full era date with its Gregorian ISO rendering (`"2008-03-26"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EraDate {
    pub era: Era,
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub iso: String,
}

/// The "平成16年" era-year prefix that some rule numbers carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EraYear {
    pub era: Era,
    pub year: u32,
}

/// A "規則第74号" reference, optionally prefixed with an era year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleNumber {
    pub number: u32,
    pub year: Option<EraYear>,
    /// The matched text, verbatim.
    pub raw: String,
}

/// "元" means year 1; otherwise parse the (possibly full-width) digits.
fn era_year_value(s: &str) -> Option<u32> {
    if s == "元" { Some(1) } else { parse_digits(s) }
}

// All patterns use `\s*` so residual letter-spacing ("令和３年 10 月", "規  則
// 第 159 号") still parses; the structured output is space-free by construction.
static ERA_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(明治|大正|昭和|平成|令和)\s*(元|[0-9０-９]+)\s*年\s*([0-9０-９]+)\s*月\s*([0-9０-９]+)\s*日",
    )
    .unwrap()
});
static RULE_NUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:(明治|大正|昭和|平成|令和)\s*(元|[0-9０-９]+)\s*年\s*)?規\s*則\s*第\s*([0-9０-９]+)\s*号",
    )
    .unwrap()
});

/// The first era date found in `text`, if any.
pub fn parse_era_date(text: &str) -> Option<EraDate> {
    let c = ERA_DATE.captures(text)?;
    let era = Era::from_kanji(&c[1])?;
    let year = era_year_value(&c[2])?;
    let month = parse_digits(&c[3])?;
    let day = parse_digits(&c[4])?;
    let iso = format!("{:04}-{:02}-{:02}", era.gregorian_base() + year, month, day);
    Some(EraDate {
        era,
        year,
        month,
        day,
        iso,
    })
}

/// The first "規則第N号" in `text`, with an optional leading era year.
pub fn parse_rule_number(text: &str) -> Option<RuleNumber> {
    let c = RULE_NUM.captures(text)?;
    let number = parse_digits(&c[3])?;
    let year = match (c.get(1), c.get(2)) {
        (Some(era), Some(y)) => Era::from_kanji(era.as_str())
            .zip(era_year_value(y.as_str()))
            .map(|(era, year)| EraYear { era, year }),
        _ => None,
    };
    Some(RuleNumber {
        number,
        year,
        raw: c.get(0).unwrap().as_str().to_string(),
    })
}

/// The enactment / amendment metadata for a rule: a date and/or a rule number,
/// with the raw text kept as an always-present fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Enactment {
    pub date: Option<EraDate>,
    pub rule_number: Option<RuleNumber>,
    pub raw: String,
}

/// Parses an enactment block; the raw text is preserved verbatim.
pub fn parse_enactment(raw: &str) -> Enactment {
    let raw = raw.trim().to_string();
    Enactment {
        date: parse_era_date(&raw),
        rule_number: parse_rule_number(&raw),
        raw,
    }
}

/// Like [`parse_enactment`] but `None` for an empty/absent block (e.g. a rule
/// with no 最終改正 line).
pub fn parse_amendment(raw: &str) -> Option<Enactment> {
    let raw = raw.trim();
    if raw.is_empty() {
        None
    } else {
        Some(parse_enactment(raw))
    }
}

/// When a 附則 takes effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effective {
    /// "…令和７年４月１日から施行する。"
    Date(EraDate),
    /// "…公布の日から施行する。"
    OnPromulgation,
    /// No recognizable effective clause.
    Unspecified,
}

static ON_PROMULGATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"公布の日\s*から\s*施行").unwrap());

/// Extracts the primary effective clause from a 附則 body. Only the first
/// "…から施行する" date is taken; secondary ただし-clause dates and
/// なお従前の例による text remain in the block's `text`.
pub fn parse_effective(text: &str) -> Effective {
    if ON_PROMULGATION.is_match(text) {
        return Effective::OnPromulgation;
    }
    // The施行 date is the era date appearing before the first "から施行".
    let head = match text.find("から施行") {
        Some(i) => &text[..i],
        None => text,
    };
    match parse_era_date(head) {
        Some(d) => Effective::Date(d),
        None => Effective::Unspecified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn era_date_with_fullwidth_and_gengo() {
        let d = parse_era_date("平成20年３月26日 規則第74号").unwrap();
        assert_eq!(d.era, Era::Heisei);
        assert_eq!((d.year, d.month, d.day), (20, 3, 26));
        assert_eq!(d.iso, "2008-03-26");
    }

    #[test]
    fn reiwa_gannen_is_year_one() {
        let d = parse_era_date("令和元年５月１日から施行する。").unwrap();
        assert_eq!((d.era, d.year), (Era::Reiwa, 1));
        assert_eq!(d.iso, "2019-05-01");
    }

    #[test]
    fn era_date_tolerates_interior_spaces() {
        let d = parse_era_date("令和３年 10 月１日").unwrap();
        assert_eq!((d.year, d.month, d.day), (3, 10, 1));
        assert_eq!(d.iso, "2021-10-01");
    }

    #[test]
    fn rule_number_plain() {
        let r = parse_rule_number("規  則  第  74  号").unwrap();
        assert_eq!(r.number, 74);
        assert_eq!(r.year, None);
    }

    #[test]
    fn rule_number_with_era_prefix() {
        // The citation inside a 全部改正 note: "平成16年規則第144号".
        let r = parse_rule_number("高知大学学生懲戒規則（平成16年規則第144号）").unwrap();
        assert_eq!(r.number, 144);
        assert_eq!(
            r.year,
            Some(EraYear {
                era: Era::Heisei,
                year: 16
            })
        );
    }

    #[test]
    fn amendment_absent_is_none() {
        assert_eq!(parse_amendment("   "), None);
        assert!(parse_amendment("最終改正 令和７年４月21日規則第10号").is_some());
    }

    #[test]
    fn effective_date_and_on_promulgation() {
        assert_eq!(
            parse_effective("この規則は、令和７年４月１日から施行する。"),
            Effective::Date(parse_era_date("令和７年４月１日").unwrap())
        );
        assert_eq!(
            parse_effective("この規則は、公布の日から施行する。"),
            Effective::OnPromulgation
        );
        assert_eq!(
            parse_effective("附則本文に日付なし。"),
            Effective::Unspecified
        );
    }

    #[test]
    fn effective_takes_primary_date_only() {
        // A ただし clause with a second date must not override the primary one.
        let e = parse_effective(
            "この規則は、令和４年４月１日から施行する。ただし、第２条は令和５年４月１日から施行する。",
        );
        assert_eq!(
            e,
            Effective::Date(parse_era_date("令和４年４月１日").unwrap())
        );
    }
}
