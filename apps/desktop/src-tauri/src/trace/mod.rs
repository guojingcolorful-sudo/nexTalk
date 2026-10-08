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
pub const TTS_USD_PER_1K_CHARS: f64 = 0.045; // Fish S2.1 Pro CJK reference

/// One translation vendor's published rates, per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TranslatorRates {
    pub prompt_usd_per_mtok: f64,
    pub completion_usd_per_mtok: f64,
}

/// The translator the pipeline ships: one id, one row, one source of truth
/// (WR-05). The id is the model string the vendor client reports as its
/// `model_version` — the same string every trace record stores — so the panel
/// cannot price a vendor the pipeline never calls.
pub const ACTIVE_TRANSLATOR_ID: &str = crate::pipeline::stages::deepseek::DEFAULT_MODEL;

/// `deepseek-chat`, the shipped translator (T2.4). Peak rates from DeepSeek's
/// public pricing page (fetched 2026-10-08; off-peak hours are half). The
/// legacy model string is served by DeepSeek-V4.1-Flash — that is the model
/// behind the call and what the bill charges, so the panel prices with it.
pub const DEEPSEEK_CHAT_RATES: TranslatorRates = TranslatorRates {
    prompt_usd_per_mtok: 0.30,     // input, cache miss
    completion_usd_per_mtok: 1.20, // output
};

/// Gemini 3.5 Flash-Lite — the documented alternative translator
/// (CLAUDE.md 备选), kept as a rate row so switching the stage is a lookup
/// change, never a re-pricing exercise.
pub const GEMINI_FLASH_LITE_ID: &str = "gemini-3.5-flash-lite";
pub const GEMINI_FLASH_LITE_RATES: TranslatorRates = TranslatorRates {
    prompt_usd_per_mtok: 0.30,
    completion_usd_per_mtok: 2.50,
};

/// The rate row for one translator id, as the trace records it (WR-05).
///
/// Unknown ids — a `DEEPSEEK_MODEL` override, or a vendor predating this
/// table — fall back to the shipped translator's row: the monthly aggregate is
/// priced as a whole, and the shipped translator is the honest assumption for
/// it. The test below pins [`ACTIVE_TRANSLATOR_ID`] to its own row, so
/// "unknown" can never quietly mean "a vendor we do not call" while the
/// shipped one is mispriced.
pub fn translator_rates(translator_id: &str) -> TranslatorRates {
    if translator_id == GEMINI_FLASH_LITE_ID {
        GEMINI_FLASH_LITE_RATES
    } else {
        DEEPSEEK_CHAT_RATES
    }
}

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
    ///
    /// The translation leg is priced with the *shipped* translator's row
    /// (WR-05): the same id the traces record per segment (D-08), so the money
    /// on the panel and the vendor that produced the tokens cannot drift apart.
    pub fn from_usage(usage: &UsageSummary) -> Self {
        let translate_rates = translator_rates(ACTIVE_TRANSLATOR_ID);
        let stt_usd = usage.stt_audio_ms as f64 / 60_000.0 * STT_USD_PER_MINUTE;
        let translate_usd = usage.translate_prompt_tokens as f64 / 1_000_000.0
            * translate_rates.prompt_usd_per_mtok
            + usage.translate_completion_tokens as f64 / 1_000_000.0
                * translate_rates.completion_usd_per_mtok;
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
        let rates = translator_rates(ACTIVE_TRANSLATOR_ID);
        assert!((cost.stt_usd - 10.0 * STT_USD_PER_MINUTE).abs() < 1e-9);
        assert!(
            (cost.translate_usd
                - (rates.prompt_usd_per_mtok + 0.5 * rates.completion_usd_per_mtok))
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

    /// WR-05: the rate row is keyed by the translator id the trace records.
    /// The shipped client's `model_version` must land on its own row (not some
    /// other vendor's), the alternative stays available, and the two rows are
    /// genuinely different — otherwise "keyed by id" would be a comment, not a
    /// mechanism.
    #[test]
    fn translator_rates_are_keyed_by_the_shipped_translator_id() {
        use crate::pipeline::stages::config::{DeepseekCredentials, Endpoints, Secret};
        use crate::pipeline::stages::{DeepseekTranslator, Translator};

        let translator = DeepseekTranslator::new(
            DeepseekCredentials {
                api_key: Secret::new("test-key"),
            },
            Endpoints::defaults(),
        );
        let recorded_id = translator.model_version();
        assert_eq!(
            recorded_id, ACTIVE_TRANSLATOR_ID,
            "the id the trace stores and the id the rate table keys on are one string"
        );
        assert_eq!(translator_rates(&recorded_id), DEEPSEEK_CHAT_RATES);
        assert_eq!(
            translator_rates(GEMINI_FLASH_LITE_ID),
            GEMINI_FLASH_LITE_RATES
        );
        assert_ne!(DEEPSEEK_CHAT_RATES, GEMINI_FLASH_LITE_RATES);
        // An unknown id (a future vendor, a model override) still prices — with
        // the shipped translator's row, never with a vendor we do not call.
        assert_eq!(translator_rates("some-future-model"), DEEPSEEK_CHAT_RATES);
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
