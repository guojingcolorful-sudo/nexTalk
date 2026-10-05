//! Trace surface (02-03 T3.4/T3.5): the durable record of what each segment
//! did and why.
//!
//! The live path never reads these files: the cascade emits the debuggable
//! facts (status, aggregatable error code) into [`jsonl`] records, the session
//! later persists them (T3.7), and the diagnostics page reports on them.
//! Nothing here may carry a vendor message blob — D-19 exists so a trace line
//! remains aggregatable long after the incident.

pub mod jsonl;

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
        assert!(
            (cost.total_usd - (cost.stt_usd + cost.translate_usd + cost.tts_usd)).abs() < 1e-9
        );
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
