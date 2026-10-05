//! D-04 deterministic output check (GOV-04, 02-03 T3.6).
//!
//! The one silent lie the cascade could still tell is a number that drifts in
//! translation — 800 毫秒 becoming 900 ms, a unit dropped, a date flipped into
//! a form that reads differently ("09/29" vs "29/09"). The model cannot be
//! trusted to preserve numeric facts, so the cascade *checks* them: every
//! candidate that reaches the single TTS exit passes through
//! [`validate_number_consistency`], and a mismatch is withheld and the
//! original is shown instead (never a "probably right" translation — T-02-12).
//!
//! The check is deliberately conservative: when in doubt it returns a
//! [`ValidationResult::Mismatch`] and the segment falls back to the original.
//! Facts are compared as a **multiset of canonical values** (thousands
//! separators removed, leading/trailing zeros normalised), units must map
//! through [`UNIT_MAPPINGS`], and dates must come back as one of the two
//! accepted forms (`MM/DD/YYYY`, `YYYY-MM-DD`) with the same day. A zh number
//! with no unit accepts any en unit (the source is silent on the unit, so the
//! translation is free to add one). Worded month names and multiplier units
//! (万元/亿) are out of scope in Phase 2 — such forms are not treated as dates
//! and fall through as plain numbers.
//!
//! No new dependencies: the scanner is hand-written over `char_indices`, which
//! keeps the crate audit surface at zero for this layer.

/// Aggregatable code the cascade attaches when a translation is rejected
/// (D-19 vocabulary): it reaches the JSONL trace and the failure-case library.
pub const NUMERIC_MISMATCH_CODE: &str = "numeric_mismatch";

/// zh unit key → accepted English spellings (D-04 mapping table).
///
/// Longer keys first (毫秒 before 秒, 美元 before 元): the zh scanner matches a
/// prefix, so order is load-bearing. Values the source may legitimately use —
/// offline comparison keeps matching deterministic and key-free.
pub const UNIT_MAPPINGS: &[(&str, &[&str])] = &[
    (
        "毫秒",
        &["ms", "msec", "msecs", "millisecond", "milliseconds"],
    ),
    ("秒", &["s", "sec", "secs", "second", "seconds"]),
    ("分钟", &["min", "mins", "minute", "minutes"]),
    ("小时", &["h", "hr", "hrs", "hour", "hours"]),
    ("天", &["d", "day", "days"]),
    ("年", &["y", "yr", "yrs", "year", "years"]),
    ("美元", &["dollar", "dollars", "usd"]),
    ("元", &["yuan", "rmb", "cny"]),
];

/// Why a translation failed the deterministic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchReason {
    /// The same number came back with a different value.
    NumberDrift,
    /// The two sides do not carry the same multiset of numbers.
    NumberCountMismatch,
    /// The source number carried a unit the translation dropped.
    MissingUnit,
    /// Both sides carry a unit, but the mapping table has no pair for them.
    UnitMismatch,
    /// A date is missing, gained, or drifted into an unaccepted form.
    DateFormatDrift,
}

impl MismatchReason {
    /// Aggregatable vocabulary for the trace and the failure-case library.
    pub fn as_str(self) -> &'static str {
        match self {
            MismatchReason::NumberDrift => "number_drift",
            MismatchReason::NumberCountMismatch => "number_count_mismatch",
            MismatchReason::MissingUnit => "missing_unit",
            MismatchReason::UnitMismatch => "unit_mismatch",
            MismatchReason::DateFormatDrift => "date_format_drift",
        }
    }
}

/// The verdict for one english candidate against its Chinese source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Every numeric fact survived.
    Match,
    /// The candidate must not be spoken; `expected`/`found` are the raw source
    /// substrings (evidence for the trace, never a vendor message).
    Mismatch {
        reason: MismatchReason,
        expected: String,
        found: String,
    },
}

/// One extracted fact.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fact {
    Number {
        /// Canonical value: separators removed, zeros normalised ("800.0" → "800").
        value: String,
        /// Canonical zh unit key; `None` when the side carries no unit.
        unit: Option<String>,
        /// The raw source substring — the evidence carried on a mismatch.
        raw: String,
    },
    Date {
        y: u32,
        m: u32,
        d: u32,
        raw: String,
    },
    /// A date-shaped token in a form the table does not accept (e.g. 29/09/2026).
    UnmappedDate {
        raw: String,
    },
}

impl Fact {
    fn raw(&self) -> &str {
        match self {
            Fact::Number { raw, .. } | Fact::Date { raw, .. } | Fact::UnmappedDate { raw } => raw,
        }
    }
}

/// Compare the numeric facts of a Chinese source and its English candidate.
pub fn validate_number_consistency(zh: &str, en: &str) -> ValidationResult {
    let zh_facts = scan_zh(zh);
    let en_facts = scan_en(en);

    // 1. Dates: a date must come back as an accepted form with the same day.
    let zh_dates: Vec<&Fact> = zh_facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Date { .. }))
        .collect();
    let en_dates: Vec<&Fact> = en_facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Date { .. }))
        .collect();
    if let Some(bad) = en_facts
        .iter()
        .find(|fact| matches!(fact, Fact::UnmappedDate { .. }))
    {
        return ValidationResult::Mismatch {
            reason: MismatchReason::DateFormatDrift,
            expected: zh_dates
                .first()
                .map_or(String::new(), |d| d.raw().to_string()),
            found: bad.raw().to_string(),
        };
    }
    if zh_dates.len() != en_dates.len() {
        return ValidationResult::Mismatch {
            reason: MismatchReason::DateFormatDrift,
            expected: join_raws(&zh_dates),
            found: join_raws(&en_dates),
        };
    }
    let mut zh_days: Vec<(u32, u32, u32, &str)> = zh_dates
        .iter()
        .map(|d| match d {
            Fact::Date { y, m, d, raw } => (*y, *m, *d, raw.as_str()),
            _ => unreachable!("filtered to Date"),
        })
        .collect();
    let mut en_days: Vec<(u32, u32, u32, &str)> = en_dates
        .iter()
        .map(|d| match d {
            Fact::Date { y, m, d, raw } => (*y, *m, *d, raw.as_str()),
            _ => unreachable!("filtered to Date"),
        })
        .collect();
    zh_days.sort();
    en_days.sort();
    for (zh_day, en_day) in zh_days.iter().zip(en_days.iter()) {
        if (zh_day.0, zh_day.1, zh_day.2) != (en_day.0, en_day.1, en_day.2) {
            return ValidationResult::Mismatch {
                reason: MismatchReason::DateFormatDrift,
                expected: zh_day.3.to_string(),
                found: en_day.3.to_string(),
            };
        }
    }

    // 2. Numbers: equal multiset of canonical values, then units, pairwise.
    let zh_numbers: Vec<&Fact> = zh_facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Number { .. }))
        .collect();
    let en_numbers: Vec<&Fact> = en_facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Number { .. }))
        .collect();
    if zh_numbers.len() != en_numbers.len() {
        return ValidationResult::Mismatch {
            reason: MismatchReason::NumberCountMismatch,
            expected: join_raws(&zh_numbers),
            found: join_raws(&en_numbers),
        };
    }
    let mut sorted_zh: Vec<&Fact> = zh_numbers.clone();
    let mut sorted_en: Vec<&Fact> = en_numbers.clone();
    sorted_zh.sort_by_key(|z| number_key(z));
    sorted_en.sort_by_key(|e| number_key(e));
    for (zh_fact, en_fact) in sorted_zh.iter().zip(sorted_en.iter()) {
        let Fact::Number {
            value: zh_value,
            unit: zh_unit,
            raw: zh_raw,
        } = zh_fact
        else {
            unreachable!("filtered to Number")
        };
        let Fact::Number {
            value: en_value,
            unit: en_unit,
            raw: en_raw,
        } = en_fact
        else {
            unreachable!("filtered to Number")
        };
        if zh_value != en_value {
            return ValidationResult::Mismatch {
                reason: MismatchReason::NumberDrift,
                expected: zh_raw.clone(),
                found: en_raw.clone(),
            };
        }
        if let Some(zh_unit) = zh_unit {
            match en_unit {
                None => {
                    return ValidationResult::Mismatch {
                        reason: MismatchReason::MissingUnit,
                        expected: zh_raw.clone(),
                        found: en_raw.clone(),
                    }
                }
                Some(en_unit) if en_unit != zh_unit => {
                    return ValidationResult::Mismatch {
                        reason: MismatchReason::UnitMismatch,
                        expected: zh_raw.clone(),
                        found: en_raw.clone(),
                    }
                }
                _ => {}
            }
        }
    }

    ValidationResult::Match
}

fn join_raws(facts: &[&Fact]) -> String {
    facts
        .iter()
        .map(|fact| fact.raw())
        .collect::<Vec<_>>()
        .join(", ")
}

fn number_key(fact: &Fact) -> (String, String) {
    match fact {
        Fact::Number { value, raw, .. } => (value.clone(), raw.clone()),
        _ => (String::new(), String::new()),
    }
}

// ------------------------------------------------------------------ scanner ---

fn read_digits(chars: &[(usize, char)], start: usize) -> (String, usize) {
    let mut digits = String::new();
    let mut i = start;
    while i < chars.len() && chars[i].1.is_ascii_digit() {
        digits.push(chars[i].1);
        i += 1;
    }
    (digits, i)
}

fn char_at(chars: &[(usize, char)], i: usize) -> Option<char> {
    chars.get(i).map(|(_, c)| *c)
}

/// Byte offset of the END of char index `i` (i == len → end of text).
fn byte_end(text: &str, chars: &[(usize, char)], i: usize) -> usize {
    match chars.get(i) {
        Some((offset, _)) => *offset,
        None => text.len(),
    }
}

fn canonical_number(integer: &str, fraction: Option<&str>) -> String {
    let stripped = integer.trim_start_matches('0');
    let integer = if stripped.is_empty() { "0" } else { stripped };
    match fraction {
        None => integer.to_string(),
        Some(fraction) => {
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                integer.to_string()
            } else {
                format!("{integer}.{fraction}")
            }
        }
    }
}

fn parse_u32(digits: &str) -> Option<u32> {
    digits.parse().ok()
}

fn valid_date(y: u32, m: u32, d: u32) -> bool {
    (1..=12).contains(&m) && (1..=31).contains(&d) && y >= 1000
}

/// zh date forms: `YYYY年M月D日` and `YYYY-MM-DD`.
fn try_zh_date(text: &str, chars: &[(usize, char)], start: usize) -> Option<(Fact, usize)> {
    let (year, after_year) = read_digits(chars, start);
    if char_at(chars, after_year) == Some('年') {
        let (month, after_month) = read_digits(chars, after_year + 1);
        if char_at(chars, after_month) == Some('月') {
            let (day, after_day) = read_digits(chars, after_month + 1);
            if char_at(chars, after_day) == Some('日') {
                if let (Some(y), Some(m), Some(d)) =
                    (parse_u32(&year), parse_u32(&month), parse_u32(&day))
                {
                    if valid_date(y, m, d) {
                        let end = after_day + 1;
                        return Some((
                            Fact::Date {
                                y,
                                m,
                                d,
                                raw: text[chars[start].0..byte_end(text, chars, end)].to_string(),
                            },
                            end,
                        ));
                    }
                }
            }
        }
    }
    if year.len() == 4 && char_at(chars, after_year) == Some('-') {
        let (month, after_month) = read_digits(chars, after_year + 1);
        if char_at(chars, after_month) == Some('-') {
            let (day, after_day) = read_digits(chars, after_month + 1);
            if let (Some(y), Some(m), Some(d)) =
                (parse_u32(&year), parse_u32(&month), parse_u32(&day))
            {
                if month.len() <= 2 && day.len() <= 2 && valid_date(y, m, d) {
                    let end = after_day;
                    return Some((
                        Fact::Date {
                            y,
                            m,
                            d,
                            raw: text[chars[start].0..byte_end(text, chars, end)].to_string(),
                        },
                        end,
                    ));
                }
            }
        }
    }
    None
}

/// zh number with an optional table unit: `800毫秒`, `800 毫秒`, `0.5 秒`.
fn try_zh_number(text: &str, chars: &[(usize, char)], start: usize) -> (Fact, usize) {
    let (integer, after_integer) = read_digits(chars, start);
    let mut j = after_integer;
    let fraction = if char_at(chars, j) == Some('.')
        && char_at(chars, j + 1).is_some_and(|c| c.is_ascii_digit())
    {
        let (fraction, after_fraction) = read_digits(chars, j + 1);
        j = after_fraction;
        Some(fraction)
    } else {
        None
    };

    // Unit: optional whitespace, then a table key (longest first — table order).
    let mut k = j;
    while char_at(chars, k).is_some_and(|c| c == ' ' || c == '\u{3000}') {
        k += 1;
    }
    let mut unit = None;
    let mut end_char = j;
    if k < chars.len() {
        let rest = &text[chars[k].0..];
        for (zh_unit, _) in UNIT_MAPPINGS {
            if rest.starts_with(zh_unit) {
                unit = Some((*zh_unit).to_string());
                end_char = k + zh_unit.chars().count();
                break;
            }
        }
    }

    (
        Fact::Number {
            value: canonical_number(&integer, fraction.as_deref()),
            unit,
            raw: text[chars[start].0..byte_end(text, chars, end_char)].to_string(),
        },
        end_char,
    )
}

/// en date-shaped tokens: `MM/DD/YYYY` and `YYYY-MM-DD` are accepted; any other
/// three-run numeric pattern on the same separator is flagged as unmapped.
fn try_en_date(text: &str, chars: &[(usize, char)], start: usize) -> Option<(Fact, usize)> {
    let (first, after_first) = read_digits(chars, start);
    let sep = char_at(chars, after_first)?;
    if !matches!(sep, '/' | '.' | '-') {
        return None;
    }
    let (second, after_second) = read_digits(chars, after_first + 1);
    if second.is_empty() || char_at(chars, after_second) != Some(sep) {
        return None;
    }
    let (third, after_third) = read_digits(chars, after_second + 1);
    if third.is_empty() {
        return None;
    }
    let raw = text[chars[start].0..byte_end(text, chars, after_third)].to_string();
    let parsed = match sep {
        '/' if first.len() <= 2 && second.len() <= 2 && third.len() == 4 => {
            match (parse_u32(&first), parse_u32(&second), parse_u32(&third)) {
                (Some(month), Some(day), Some(year)) => Some((year, month, day)),
                _ => None,
            }
        }
        '-' if first.len() == 4 && second.len() <= 2 && third.len() <= 2 => {
            match (parse_u32(&first), parse_u32(&second), parse_u32(&third)) {
                (Some(year), Some(month), Some(day)) => Some((year, month, day)),
                _ => None,
            }
        }
        _ => None,
    };
    let fact = match parsed {
        Some((y, m, d)) if valid_date(y, m, d) => Fact::Date { y, m, d, raw },
        _ => Fact::UnmappedDate { raw },
    };
    Some((fact, after_third))
}

/// en number with optional thousands separators / decimal and a unit word:
/// `800 ms`, `10,000 hours`, `0.5 seconds`.
fn try_en_number(text: &str, chars: &[(usize, char)], start: usize) -> (Fact, usize) {
    let (mut digits, mut j) = read_digits(chars, start);
    loop {
        if char_at(chars, j) == Some(',')
            && (0..3)
                .all(|offset| char_at(chars, j + 1 + offset).is_some_and(|c| c.is_ascii_digit()))
            && !char_at(chars, j + 4).is_some_and(|c| c.is_ascii_digit())
        {
            let (group, after_group) = read_digits(chars, j + 1);
            digits.push_str(&group);
            j = after_group;
        } else {
            break;
        }
    }
    let mut value = digits;
    let mut end_char = j;
    let fraction = if char_at(chars, j) == Some('.')
        && char_at(chars, j + 1).is_some_and(|c| c.is_ascii_digit())
    {
        let (fraction, after_fraction) = read_digits(chars, j + 1);
        j = after_fraction;
        end_char = j;
        Some(fraction)
    } else {
        None
    };

    // Unit: whitespace, then an ascii word; mapped through the table when known.
    let mut k = j;
    while char_at(chars, k) == Some(' ') {
        k += 1;
    }
    let mut word_end = k;
    while char_at(chars, word_end).is_some_and(|c| c.is_ascii_alphabetic()) {
        word_end += 1;
    }
    let unit = if word_end > k {
        let word: String = chars[k..word_end]
            .iter()
            .map(|(_, c)| c.to_ascii_lowercase())
            .collect();
        let mapped = UNIT_MAPPINGS
            .iter()
            .find(|(_, variants)| variants.contains(&word.as_str()))
            .map(|(zh_unit, _)| (*zh_unit).to_string());
        Some(mapped.unwrap_or(word))
    } else {
        None
    };

    // The raw carries the unit spelling the source used; the canonical unit key
    // travels separately so `ms` and `milliseconds` compare equal.
    let raw_end = if unit.is_some() { word_end } else { end_char };
    (
        Fact::Number {
            value: canonical_number(&value, fraction.as_deref()),
            unit,
            raw: text[chars[start].0..byte_end(text, chars, raw_end)].to_string(),
        },
        raw_end,
    )
}

fn scan_zh(text: &str) -> Vec<Fact> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut facts = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_ascii_digit() {
            i += 1;
            continue;
        }
        if let Some((fact, next)) = try_zh_date(text, &chars, i) {
            facts.push(fact);
            i = next;
            continue;
        }
        let (fact, next) = try_zh_number(text, &chars, i);
        facts.push(fact);
        i = next.max(i + 1);
    }
    facts
}

fn scan_en(text: &str) -> Vec<Fact> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut facts = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_ascii_digit() {
            i += 1;
            continue;
        }
        if let Some((fact, next)) = try_en_date(text, &chars, i) {
            facts.push(fact);
            i = next;
            continue;
        }
        let (fact, next) = try_en_number(text, &chars, i);
        facts.push(fact);
        i = next.max(i + 1);
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_numbers_with_mapped_units_pass() {
        assert_eq!(
            validate_number_consistency("延迟 800 毫秒", "latency of 800 milliseconds"),
            ValidationResult::Match
        );
        assert_eq!(
            validate_number_consistency("延迟 800毫秒", "latency 800 ms"),
            ValidationResult::Match
        );
        assert_eq!(
            validate_number_consistency("等 10000 小时", "wait 10,000 hours"),
            ValidationResult::Match
        );
        assert_eq!(
            validate_number_consistency("用了 0.5 秒", "took 0.5 seconds"),
            ValidationResult::Match
        );
    }

    #[test]
    fn drifted_numbers_are_rejected_with_raw_evidence() {
        assert_eq!(
            validate_number_consistency("查询用了 800 毫秒", "The query took 900 ms."),
            ValidationResult::Mismatch {
                reason: MismatchReason::NumberDrift,
                expected: "800 毫秒".to_string(),
                found: "900 ms".to_string(),
            }
        );
    }

    #[test]
    fn a_dropped_unit_is_rejected() {
        assert_eq!(
            validate_number_consistency("查询用了 800毫秒", "The query took 800."),
            ValidationResult::Mismatch {
                reason: MismatchReason::MissingUnit,
                expected: "800毫秒".to_string(),
                found: "800".to_string(),
            }
        );
    }

    #[test]
    fn a_swapped_unit_is_rejected() {
        assert_eq!(
            validate_number_consistency("延迟 800毫秒", "latency of 800 seconds"),
            ValidationResult::Mismatch {
                reason: MismatchReason::UnitMismatch,
                expected: "800毫秒".to_string(),
                found: "800 seconds".to_string(),
            }
        );
    }

    #[test]
    fn date_forms_map_only_through_the_table() {
        // 29/09/2026 reads as a different day — never accepted.
        assert_eq!(
            validate_number_consistency(
                "面试定在 2026年9月29日",
                "The interview is on 29/09/2026."
            ),
            ValidationResult::Mismatch {
                reason: MismatchReason::DateFormatDrift,
                expected: "2026年9月29日".to_string(),
                found: "29/09/2026".to_string(),
            }
        );
        // The accepted US form comes back as the same day: a match.
        assert_eq!(
            validate_number_consistency(
                "面试定在 2026年9月29日",
                "The interview is on 09/29/2026."
            ),
            ValidationResult::Match
        );
        // Same value, drifted day: rejected.
        assert_eq!(
            validate_number_consistency(
                "面试定在 2026年9月29日",
                "The interview is on 09/30/2026."
            ),
            ValidationResult::Mismatch {
                reason: MismatchReason::DateFormatDrift,
                expected: "2026年9月29日".to_string(),
                found: "09/30/2026".to_string(),
            }
        );
        assert_eq!(
            validate_number_consistency("面试定在 2026-09-29", "The interview is on 2026-09-29."),
            ValidationResult::Match
        );
    }

    #[test]
    fn a_date_that_disappears_is_rejected() {
        assert_eq!(
            validate_number_consistency("面试定在 2026-09-29", "The interview is soon."),
            ValidationResult::Mismatch {
                reason: MismatchReason::DateFormatDrift,
                expected: "2026-09-29".to_string(),
                found: String::new(),
            }
        );
    }

    #[test]
    fn counts_must_match() {
        assert_eq!(
            validate_number_consistency("两个指标：800 毫秒和 3 小时", "The metrics are 800 ms."),
            ValidationResult::Mismatch {
                reason: MismatchReason::NumberCountMismatch,
                expected: "800 毫秒, 3 小时".to_string(),
                found: "800 ms".to_string(),
            }
        );
    }

    #[test]
    fn prose_without_numbers_is_a_match() {
        assert_eq!(
            validate_number_consistency("今天天气不错", "Nice weather today"),
            ValidationResult::Match
        );
    }

    #[test]
    fn mismatch_reasons_use_the_aggregatable_vocabulary() {
        assert_eq!(NUMERIC_MISMATCH_CODE, "numeric_mismatch");
        assert_eq!(MismatchReason::NumberDrift.as_str(), "number_drift");
        assert_eq!(
            MismatchReason::NumberCountMismatch.as_str(),
            "number_count_mismatch"
        );
        assert_eq!(MismatchReason::MissingUnit.as_str(), "missing_unit");
        assert_eq!(MismatchReason::UnitMismatch.as_str(), "unit_mismatch");
        assert_eq!(
            MismatchReason::DateFormatDrift.as_str(),
            "date_format_drift"
        );
    }
}
