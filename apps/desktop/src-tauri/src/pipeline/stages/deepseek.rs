//! DeepSeek streaming translation — Chinese in, English out (T2.4).
//!
//! Ported from `tools/vendor-experiments/translation-probe.mjs`.
//!
//! # The bug this file exists to not have
//!
//! Failure case 0001 (`tools/vendor-experiments/failure-cases/`): the probe
//! parsed SSE **per TCP chunk**, so an event split across two chunks failed to
//! parse and was silently dropped — the number `800` vanished from a sentence
//! about a spiky query. [`SseBuffer`] splits on the `\n\n` boundary instead and
//! keeps the incomplete tail for the next chunk. The regression test feeds the
//! exact chunk sequence from that report.
//!
//! # Contract
//!
//! - `temperature: 0` and a non-reasoning model: translation is a
//!   deterministic transform, not a creative act, and a thinking model would
//!   spend the 1.5 s strategy budget on reasoning tokens.
//! - The context window is **≤2 sentences** (AI-SPEC §4): the previous
//!   fragment's English plus the current Chinese. More context is more tokens
//!   and more drift, not more accuracy.
//! - The prompt is three sections in a fixed order (system → glossary →
//!   source) so Phase 4 can fill the glossary without touching the protocol.
//! - Model output is a JSON object (`{"t":"fragment",…}`); anything else is a
//!   **retryable failure**, never a lenient empty-string fallback — a silent
//!   empty translation would look like success all the way to the headphones.

use serde_json::{json, Value};

use super::config::{DeepseekCredentials, Endpoints};
use super::error::StageError;
use super::traits::{AbstainReason, GlossaryEntry, ZhFragment};

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// `deepseek-chat` — the non-reasoning model. The R1 family spends tokens on
/// hidden reasoning and is banned here (see [`is_reasoning_model`]).
pub const DEFAULT_MODEL: &str = "deepseek-chat";

/// Environment override for the model id.
pub const MODEL_VAR: &str = "DEEPSEEK_MODEL";

/// OpenAI-compatible chat path on the configured base URL.
pub const CHAT_PATH: &str = "/chat/completions";

/// AI-SPEC §4: previous English + current Chinese, never more.
pub const MAX_CONTEXT_MESSAGES: usize = 2;

const PROVIDER: &str = "deepseek";

/// The system instruction: translate faithfully, keep numbers/units/dates,
/// explain nothing, answer with JSON only.
pub const SYSTEM_PROMPT: &str = "You are a simultaneous interpreter for a live technical interview. \
Translate the user's Chinese into natural, spoken English. Preserve every number, unit, date and \
proper noun exactly. Do not add, explain, summarise or omit anything. Reply with a single JSON \
object and nothing else: {\"t\":\"fragment\",\"text\":\"<the English>\",\"final_flag\":true}. \
If the fragment carries no translatable content, reply {\"t\":\"abstained\",\"reason\":\"silent_audio\"}.";

/// R1-class models are banned: hidden reasoning tokens blow the latency budget.
pub fn is_reasoning_model(model: &str) -> bool {
    let lowered = model.to_ascii_lowercase();
    lowered.contains("reasoner") || lowered.contains("-r1") || lowered.contains("r1-")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Failure case 0001, verbatim: the report is the source of truth for the
    /// regression, not a copy pasted into the test file.
    const CASE_0001: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/vendor-experiments/failure-cases/0001-sse-split-token-loss.json"
    ));

    /// The TCP chunk sequence of the reproduction: the boundary between two
    /// events is split across chunks (this is what killed the v1 probe).
    fn case_0001_chunks() -> Vec<String> {
        vec![
            "data: {\"choices\":[{\"delta\":{\"content\":\"with a single response exceeding \"}}]}\n\n"
                .to_string(),
            // The event boundary itself straddles the chunk edge:
            "data: {\"choices\":[{\"delta\":{\"content\":\"800 \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"milliseconds.\"}}]}"
                .to_string(),
            "\n\ndata: [DONE]\n\n".to_string(),
        ]
    }

    /// Every digit run in `haystack`, in order.
    fn numbers_in(haystack: &str) -> Vec<&str> {
        haystack
            .split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect()
    }

    #[test]
    fn sse_events_are_split_on_the_boundary_not_the_tcp_chunk() {
        // Test 1 / failure case 0001.
        let case: Value = serde_json::from_str(CASE_0001).expect("the report parses");
        let source = case["input"].as_str().expect("the report states the input");
        let expected = case["expected_output"]
            .as_str()
            .expect("the report states the expected output");
        let wrong = case["wrong_output"].as_str().expect("and the wrong one");

        let mut buffer = SseBuffer::new();
        let mut payloads: Vec<String> = Vec::new();
        for chunk in case_0001_chunks() {
            payloads.extend(buffer.push(&chunk));
        }
        assert_eq!(
            payloads.len(),
            4,
            "three content events plus the sentinel: {payloads:?}"
        );
        assert_eq!(buffer.residual(), "", "nothing is left over");

        let translated: String = payloads
            .iter()
            .filter(|payload| *payload != "[DONE]")
            .filter_map(|payload| serde_json::from_str::<Value>(payload).ok())
            .filter_map(|event| {
                event["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect();

        // The report's own regression test: the translation carries every
        // number of the source. `800` is the one the v1 probe dropped.
        let numbers = numbers_in(source);
        assert!(!numbers.is_empty(), "the source has numbers");
        for number in numbers {
            assert!(
                translated.contains(number),
                "{number} was lost: {translated}"
            );
        }
        // And it matches the good output, not the broken one. The report marks
        // the truncated head of its sample with an ellipsis — strip it.
        let expected_tail = expected.trim_matches('…').trim();
        assert_eq!(translated.trim(), expected_tail, "the report's expectation");
        assert!(
            !translated.contains("  "),
            "the bug's signature double space is present: {translated:?}"
        );
        assert!(
            wrong.contains("  "),
            "the report's wrong output is the double-space variant"
        );
    }

    #[test]
    fn an_incomplete_event_waits_for_the_next_chunk() {
        let mut buffer = SseBuffer::new();
        assert!(buffer.push("data: {\"a\":1}").is_empty(), "no boundary yet");
        assert_eq!(buffer.residual(), "data: {\"a\":1}");
        let payloads = buffer.push("\n\n");
        assert_eq!(payloads, vec!["{\"a\":1}".to_string()]);
        assert_eq!(buffer.residual(), "");
    }

    #[test]
    fn the_request_locks_temperature_zero_and_the_non_reasoning_model() {
        // Test 2
        let fragment = ZhFragment {
            text: "你能详细说一下吗？".to_string(),
            seq: 1,
        };
        let body = build_request_body(DEFAULT_MODEL, &fragment, &[], None);
        assert_eq!(body["temperature"], json!(0));
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["stream_options"]["include_usage"], json!(true));
        let model = body["model"].as_str().expect("a model id");
        assert!(!is_reasoning_model(model), "{model} is a thinking model");
        assert_eq!(model, "deepseek-chat");
    }

    #[test]
    fn r1_class_models_are_refused() {
        for banned in ["deepseek-reasoner", "deepseek-r1", "r1-distill"] {
            assert!(is_reasoning_model(banned), "{banned} must be refused");
        }
        assert!(!is_reasoning_model("deepseek-chat"));
    }

    #[test]
    fn the_context_window_is_at_most_two_sentences() {
        // Test 3
        let fragment = ZhFragment {
            text: "当前句。".to_string(),
            seq: 2,
        };
        let fresh = build_request_body(DEFAULT_MODEL, &fragment, &[], None);
        let messages = fresh["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 1 + 1, "system + the current sentence");
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "当前句。");

        let with_context =
            build_request_body(DEFAULT_MODEL, &fragment, &[], Some("The previous sentence."));
        let messages = with_context["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 1 + MAX_CONTEXT_MESSAGES);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], "The previous sentence.");
        assert_eq!(messages[2]["role"], "user");

        // The window is 2 sentences: one previous translation + the current
        // source. Nothing older is ever attached.
        assert!(messages.len() - 1 <= MAX_CONTEXT_MESSAGES);
    }

    #[test]
    fn the_glossary_section_exists_even_when_empty() {
        // Test 4: the block is part of the protocol now, so Phase 4 fills it
        // without changing the request shape.
        let fragment = ZhFragment {
            text: "慢查询日志。".to_string(),
            seq: 1,
        };
        let empty = build_request_body(DEFAULT_MODEL, &fragment, &[], None);
        let system = empty["messages"][0]["content"]
            .as_str()
            .expect("a system message");
        assert!(system.contains("Glossary"), "{system}");
        assert!(system.contains("(none supplied)"), "{system}");

        let glossary = vec![GlossaryEntry {
            zh: "慢查询日志".to_string(),
            en: "slow query log".to_string(),
        }];
        let filled = build_request_body(DEFAULT_MODEL, &fragment, &glossary, None);
        let system = filled["messages"][0]["content"]
            .as_str()
            .expect("a system message");
        assert!(system.contains("慢查询日志 → slow query log"), "{system}");
    }

    #[test]
    fn a_valid_structured_output_maps_to_a_fragment() {
        // Test 5
        let parsed = parse_model_output(
            "{\"t\":\"fragment\",\"text\":\"The query took 800 ms.\",\"final_flag\":true}",
        )
        .expect("valid output");
        match parsed {
            ModelOutput::Fragment { text, final_flag } => {
                assert_eq!(text, "The query took 800 ms.");
                assert!(final_flag);
            }
            other => panic!("expected a fragment, got {other:?}"),
        }
    }

    #[test]
    fn malformed_output_is_a_retryable_error_never_an_empty_string() {
        // Test 6: a lenient fallback would look like success downstream.
        for broken in [
            "{\"t\":\"fragment\",\"text\":\"half",
            "not json at all",
            "{\"t\":\"fragment\",\"text\":42}",
            "{\"t\":\"unknown\"}",
            "",
        ] {
            let error = parse_model_output(broken).expect_err("must not parse");
            assert_eq!(
                error.retry_class,
                crate::pipeline::stages::error::RetryClass::Retryable,
                "{broken}"
            );
            assert_eq!(error.provider, "deepseek");
        }
    }

    #[test]
    fn the_abstain_channel_maps_to_the_shared_vocabulary() {
        // Test 7 (D-03)
        let parsed =
            parse_model_output("{\"t\":\"abstained\",\"reason\":\"silent_audio\"}").expect("valid");
        assert_eq!(
            parsed,
            ModelOutput::Abstained {
                reason: AbstainReason::SilentAudio
            }
        );
        let parsed = parse_model_output("{\"t\":\"abstained\",\"reason\":\"unrecognized\"}")
            .expect("valid");
        assert_eq!(
            parsed,
            ModelOutput::Abstained {
                reason: AbstainReason::Unrecognized
            }
        );
    }

    #[test]
    fn the_missing_key_error_names_the_variable_and_leaks_nothing() {
        // Test 10
        let error = DeepseekCredentials::from_lookup(|_| None).expect_err("no key configured");
        let rendered = error.to_string();
        assert!(rendered.contains("DEEPSEEK_API_KEY"), "{rendered}");
        assert!(!rendered.contains("sk-"), "{rendered}");

        let configured = DeepseekCredentials::from_lookup(|name| {
            (name == DeepseekCredentials::API_KEY_VAR).then(|| "sk-secret-value".to_string())
        })
        .expect("configured");
        assert_eq!(configured.api_key.expose(), "sk-secret-value");
        assert!(
            !format!("{configured:?}").contains("sk-secret-value"),
            "the key never renders"
        );
    }
}
