//! Trace surface (02-03 T3.4/T3.5/T3.7): the durable record of what each
//! segment did, what it cost and why.
//!
//! The live path never reads these files: the cascade emits the debuggable
//! facts (status, aggregatable error code) into [`jsonl`] records, the session
//! state persists them through the single writer ([`jsonl::TraceWriter`]), and
//! the diagnostics page reports on them — cost converted here, through the
//! named rate table, never by the frontend. Nothing here may carry a vendor
//! message blob — D-19 exists so a trace line remains aggregatable long after
//! the incident, and nothing here opens a socket (interview text stays local).

pub mod jsonl;

use serde::{Deserialize, Serialize};

pub use jsonl::UsageSummary;

/// Wall-clock millis since the epoch — the trace surface's only clock read.
/// A pre-1970 system clock is not representable; `0` keeps the record
/// countable instead of panicking.
pub fn unix_millis_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// `YYYY-MM-DD` for a Unix-millis instant — civil date arithmetic with no
/// dependency (the date directory of D-05's layout).
pub(crate) fn date_dir_name(unix_ms: u64) -> String {
    let (year, month, day) = civil_from_days(unix_ms / 86_400_000);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to (y, m, d).
/// Valid for every non-negative day count (the epoch onward).
fn civil_from_days(days: u64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097; // day of era [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe as i64 + era as i64 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Named rate table (D-13). These are the published 2026 vendor rates from the
/// stack table in CLAUDE.md; 火山复刻 ICL 2.0 publishes no per-character rate,
/// so the TTS estimate uses the Fish S2.1 Pro CJK reference ($45/MTok ≈
/// $0.045/1K chars) until a vendor figure exists. Every stage converts through
/// these constants — the cost test re-derives the math from them, so a
/// scattered literal fails there.
pub const STT_USD_PER_MINUTE: f64 = 0.0048; // Deepgram Nova-3 streaming
pub const TRANSLATE_USD_PER_MTOK_PROMPT: f64 = 0.30; // Gemini 3.5 Flash-Lite
pub const TRANSLATE_USD_PER_MTOK_COMPLETION: f64 = 2.50; // Gemini 3.5 Flash-Lite
pub const TTS_USD_PER_1K_CHARS: f64 = 0.045; // Fish S2.1 Pro CJK reference

/// The month's cost ceiling the panel flags against — a placeholder until the
/// 套餐 lands; the badge exists so an overrun is visible the moment it does.
pub const MONTHLY_COST_BUDGET_USD: f64 = 5.0;

/// The monthly voice quota the 额度面板 measures against (T3.9) — STT-audio
/// minutes for now.
pub const MONTHLY_QUOTA_MINUTES: u64 = 600;

/// The month's cost in USD, one field per pipeline stage (D-13), converted
/// eagerly from [`UsageSummary`] so the panel renders and never recomputes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CostReport {
    pub stt_usd: f64,
    pub translate_usd: f64,
    pub tts_usd: f64,
    pub total_usd: f64,
    pub budget_usd: f64,
    pub over_budget: bool,
}

impl CostReport {
    /// Converts one usage summary through the named rate table.
    pub fn from_usage(usage: &UsageSummary) -> Self {
        let stt_usd = usage.stt_audio_ms as f64 / 60_000.0 * STT_USD_PER_MINUTE;
        let translate_usd = usage.translate_prompt_tokens as f64 / 1_000_000.0
            * TRANSLATE_USD_PER_MTOK_PROMPT
            + usage.translate_completion_tokens as f64 / 1_000_000.0
                * TRANSLATE_USD_PER_MTOK_COMPLETION;
        let tts_usd = usage.tts_chars as f64 / 1_000.0 * TTS_USD_PER_1K_CHARS;
        let total_usd = stt_usd + translate_usd + tts_usd;
        Self {
            stt_usd,
            translate_usd,
            tts_usd,
            total_usd,
            budget_usd: MONTHLY_COST_BUDGET_USD,
            over_budget: total_usd > MONTHLY_COST_BUDGET_USD,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::jsonl::UsageSummary;

    /// D-06's date directory comes from the session-start clock, not from the
    /// wall clock at write time — civil dates straight off the epoch.
    #[test]
    fn date_directories_are_civil_dates() {
        assert_eq!(date_dir_name(0), "1970-01-01");
        assert_eq!(date_dir_name(1_791_158_400_000), "2026-10-05");
        assert_eq!(
            date_dir_name(1_791_158_400_000 - 4 * 86_400_000),
            "2026-10-01"
        );
    }

    /// Test 6 (D-13): the cost conversion reads a named rate table — the test
    /// re-derives every stage from the constants, so a scattered literal would
    /// fail here.
    #[test]
    fn cost_comes_from_the_named_rate_table() {
        let usage = UsageSummary {
            sessions: 1,
            segments: 12,
            skipped_lines: 0,
            stt_audio_ms: 600_000, // 10 minutes
            translate_prompt_tokens: 1_000_000,
            translate_completion_tokens: 500_000,
            tts_chars: 100_000,
        };
        let cost = CostReport::from_usage(&usage);
        assert!((cost.stt_usd - 10.0 * STT_USD_PER_MINUTE).abs() < 1e-9);
        assert!(
            (cost.translate_usd
                - (TRANSLATE_USD_PER_MTOK_PROMPT + 0.5 * TRANSLATE_USD_PER_MTOK_COMPLETION))
                .abs()
                < 1e-9
        );
        assert!((cost.tts_usd - 100.0 * TTS_USD_PER_1K_CHARS).abs() < 1e-9);
        assert!((cost.total_usd - (cost.stt_usd + cost.translate_usd + cost.tts_usd)).abs() < 1e-9);
        assert!((cost.budget_usd - MONTHLY_COST_BUDGET_USD).abs() < 1e-9);
        assert!(
            cost.over_budget,
            "10 min + 1.5 MTok + 100k chars exceeds the placeholder budget"
        );

        // An empty month reports zero, under budget — not NaN, not an error.
        let empty = CostReport::from_usage(&UsageSummary::default());
        assert_eq!(empty.total_usd, 0.0);
        assert!(!empty.over_budget);
    }

    /// Test 5 (privacy, static half): the trace module opens no socket — no
    /// network client is reachable from the code that persists interview text.
    #[test]
    fn the_trace_module_opens_no_sockets() {
        let source = concat!(include_str!("mod.rs"), include_str!("jsonl.rs"));
        for needle in [
            concat!("req", "west"),
            concat!("std::", "net"),
            concat!("tokio::", "net"),
        ] {
            assert!(
                !source.contains(needle),
                "the trace module must not touch the network: {needle}"
            );
        }
    }
}
