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

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::pipeline::budget::Stage;

use super::config::{DeepseekCredentials, Endpoint, Endpoints, Secret};
use super::error::{ErrorKind, RetryClass, StageError};
use super::traits::{
    AbstainReason, GlossaryEntry, MarkHandle, TokenUsage, Translator, TranslatorEvent,
    TranslatorStream, ZhFragment, EVENT_QUEUE_ITEMS,
};

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

/// The model to use: the `DEEPSEEK_MODEL` override when set and non-blank,
/// else [`DEFAULT_MODEL`]. Mirrors `Endpoint::from_lookup` — the lookup keeps
/// the environment out of the tests.
pub fn configured_model(lookup: impl Fn(&str) -> Option<String>) -> String {
    lookup(MODEL_VAR)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

// ---------------------------------------------------------------------------
// SSE framing (failure case 0001)
// ---------------------------------------------------------------------------

/// Buffers a `text/event-stream` body and yields one payload per `\n\n`
/// boundary.
///
/// The v1 probe parsed line by line as chunks arrived, so an event whose
/// boundary fell mid-chunk failed to parse and was dropped — failure case 0001.
/// Here the incomplete tail of a chunk stays in [`SseBuffer::residual`] and is
/// completed by the next one.
///
/// DeepSeek emits one `data:` line per event and `\n\n` separators; the CRLF
/// form of the SSE specification is not produced by this endpoint.
#[derive(Debug, Default, Clone)]
pub struct SseBuffer {
    residual: String,
}

impl SseBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `chunk`, returning every event it completed (payload without the
    /// `data:` prefix; comments and blank blocks are skipped).
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.residual.push_str(chunk);
        let mut payloads = Vec::new();
        while let Some(boundary) = self.residual.find("\n\n") {
            let block = self.residual[..boundary].to_string();
            self.residual.drain(..boundary + 2);
            if let Some(payload) = payload_of(&block) {
                payloads.push(payload);
            }
        }
        payloads
    }

    /// The bytes of an event that has not been completed yet.
    pub fn residual(&self) -> &str {
        &self.residual
    }
}

/// The payload of one complete SSE block, or `None` for keep-alive noise.
fn payload_of(block: &str) -> Option<String> {
    let lines: Vec<&str> = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// model output
// ---------------------------------------------------------------------------

/// What one request produced, once the model's JSON parses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelOutput {
    /// The translated fragment.
    Fragment { text: String, final_flag: bool },
    /// The model declined (D-03).
    Abstained { reason: AbstainReason },
}

/// Parse the model's reply.
///
/// Anything that is not one of the two shapes above is a **retryable**
/// protocol failure. There is deliberately no lenient fallback: an empty
/// translation that looked like success would reach the headphones as silence.
pub fn parse_model_output(raw: &str) -> Result<ModelOutput, StageError> {
    #[derive(Deserialize)]
    #[serde(tag = "t", rename_all = "snake_case")]
    enum Wire {
        Fragment {
            text: String,
            #[serde(default)]
            final_flag: bool,
        },
        Abstained {
            reason: String,
        },
    }

    let wire: Wire = serde_json::from_str(raw.trim()).map_err(|error| {
        StageError::protocol(PROVIDER, format!("unreadable model output: {error}"))
    })?;
    Ok(match wire {
        Wire::Fragment { text, final_flag } => ModelOutput::Fragment { text, final_flag },
        Wire::Abstained { reason } => ModelOutput::Abstained {
            // An unknown reason is still a refusal — never a silent success.
            reason: match reason.trim() {
                "silent_audio" => AbstainReason::SilentAudio,
                _ => AbstainReason::Unrecognized,
            },
        },
    })
}

// ---------------------------------------------------------------------------
// request shape
// ---------------------------------------------------------------------------

/// The system message: instructions plus the glossary section.
///
/// The section is always present, `(none supplied)` when the term base is
/// empty, so Phase 4 fills it without changing the request shape.
fn system_message(glossary: &[GlossaryEntry]) -> String {
    let mut message = String::from(SYSTEM_PROMPT);
    message.push_str("\n\nGlossary — always use these English renderings:\n");
    if glossary.is_empty() {
        message.push_str("(none supplied)");
        return message;
    }
    let entries: Vec<String> = glossary
        .iter()
        .map(|entry| format!("- {} → {}", entry.zh, entry.en))
        .collect();
    message.push_str(&entries.join("\n"));
    message
}

/// One chat-completions request.
///
/// `previous_translation` is the *only* context sent: AI-SPEC §4 allows the
/// previous English sentence plus the current Chinese, and nothing older.
pub fn build_request_body(
    model: &str,
    fragment: &ZhFragment,
    glossary: &[GlossaryEntry],
    previous_translation: Option<&str>,
) -> Value {
    let mut messages = vec![json!({
        "role": "system",
        "content": system_message(glossary),
    })];
    if let Some(previous) = previous_translation.filter(|text| !text.trim().is_empty()) {
        messages.push(json!({ "role": "assistant", "content": previous }));
    }
    messages.push(json!({ "role": "user", "content": fragment.text }));

    json!({
        "model": model,
        "temperature": 0,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": messages,
    })
}

// ---------------------------------------------------------------------------
// client
// ---------------------------------------------------------------------------

/// The streaming DeepSeek translation client (T2.4).
#[derive(Debug, Clone)]
pub struct DeepseekTranslator {
    credentials: DeepseekCredentials,
    endpoints: Endpoints,
    model: String,
    marks: MarkHandle,
    /// The previous fragment's English, written by the last finished run.
    previous: Arc<Mutex<Option<String>>>,
    http: reqwest::Client,
}

impl DeepseekTranslator {
    pub fn new(credentials: DeepseekCredentials, endpoints: Endpoints) -> Self {
        Self {
            credentials,
            endpoints,
            model: DEFAULT_MODEL.to_string(),
            marks: MarkHandle::disabled(),
            previous: Arc::new(Mutex::new(None)),
            http: reqwest::Client::new(),
        }
    }

    /// Credentials, endpoints and the model override from the environment.
    pub fn from_env() -> Result<Self, StageError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// The same, with an injected lookup (tests, per-track configuration).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let credentials = DeepseekCredentials::from_lookup(&lookup)?;
        let model = configured_model(&lookup);
        let endpoints = Endpoints::from_lookup(lookup);
        Ok(Self::new(credentials, endpoints).with_model(model))
    }

    /// Pin the model (the refusal path, tests, a cheaper deployment).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoints.deepseek_http
    }
}

impl Translator for DeepseekTranslator {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn model_version(&self) -> String {
        self.model.clone()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn translate(
        &mut self,
        fragment: &ZhFragment,
        glossary: &[GlossaryEntry],
        _epoch: u64,
    ) -> Result<TranslatorStream, StageError> {
        if is_reasoning_model(&self.model) {
            return Err(StageError::new(
                PROVIDER,
                ErrorKind::Config,
                RetryClass::Terminal,
                format!(
                    "{} is a reasoning model; the translation budget needs the non-reasoning path",
                    self.model
                ),
            ));
        }

        let previous = self
            .previous
            .lock()
            .expect("the context lock is never poisoned");
        let body = build_request_body(&self.model, fragment, glossary, previous.as_deref());
        drop(previous);

        let url = self.endpoints.deepseek_http.join(CHAT_PATH);
        let (sender, events) = mpsc::channel(EVENT_QUEUE_ITEMS);
        tokio::spawn(run_request(RunContext {
            http: self.http.clone(),
            url,
            api_key: self.credentials.api_key.clone(),
            body,
            model: self.model.clone(),
            marks: self.marks.clone(),
            previous: self.previous.clone(),
            sender,
        }));
        Ok(TranslatorStream::new(events))
    }
}

/// Everything the driver task needs — moved in, so `translate` can return.
struct RunContext {
    http: reqwest::Client,
    url: String,
    api_key: Secret,
    body: Value,
    /// D-08: the model recorded with each sentence is the one that ran.
    model: String,
    marks: MarkHandle,
    previous: Arc<Mutex<Option<String>>>,
    sender: mpsc::Sender<TranslatorEvent>,
}

impl RunContext {
    fn model(&self) -> String {
        self.model.clone()
    }
}

async fn run_request(context: RunContext) {
    let outcome = stream_response(&context).await;
    if let Err(error) = outcome {
        let _ = context.sender.send(TranslatorEvent::Failed(error)).await;
    }
}

/// Read the SSE body and forward what it carries.
///
/// A failure here is always reported: the caller turns it into one
/// [`TranslatorEvent::Failed`], which is why an unparseable or truncated
/// response can never masquerade as an empty translation.
async fn stream_response(context: &RunContext) -> Result<(), StageError> {
    let response = context
        .http
        .post(&context.url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", context.api_key.expose()),
        )
        .json(&context.body)
        .send()
        .await
        .map_err(|error| {
            StageError::transport(PROVIDER, format!("the request failed: {error}"))
                .with_endpoint(&context.url)
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(StageError::http(PROVIDER, status.as_u16()).with_endpoint(&context.url));
    }

    let mut buffer = SseBuffer::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut body = response.bytes_stream();
    let mut run = RunState::default();

    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|error| {
            StageError::transport(PROVIDER, format!("the stream broke: {error}"))
                .with_endpoint(&context.url)
        })?;
        pending.extend_from_slice(&chunk);
        let text = take_valid_utf8(&mut pending)?;
        for payload in buffer.push(&text) {
            if payload == "[DONE]" {
                run.terminated = true;
                continue;
            }
            let event: Value = serde_json::from_str(&payload).map_err(|error| {
                StageError::protocol(PROVIDER, format!("unreadable SSE event: {error}"))
                    .with_endpoint(&context.url)
            })?;
            if let Some(usage) = usage_from(&event) {
                run.usage = Some(usage);
            }
            run.push_delta(content_delta(&event), context).await?;
        }
    }

    if !run.terminated {
        return Err(
            StageError::protocol(PROVIDER, "the stream ended before its terminator")
                .with_endpoint(&context.url),
        );
    }
    if run.emitted.is_none() {
        return Err(
            StageError::protocol(PROVIDER, "the response carried no translatable output")
                .with_endpoint(&context.url),
        );
    }
    if let Some(text) = run.english() {
        *context
            .previous
            .lock()
            .expect("the context lock is never poisoned") = Some(text);
    }
    if let Some(usage) = run.usage {
        let _ = context.sender.send(TranslatorEvent::Usage(usage)).await;
    }
    Ok(())
}

/// The accumulator behind one response.
#[derive(Debug, Default)]
struct RunState {
    /// Content deltas received so far — the model's JSON, in pieces.
    content: String,
    /// What has already been reported downstream.
    emitted: Option<ModelOutput>,
    usage: Option<TokenUsage>,
    terminated: bool,
}

impl RunState {
    /// Append one delta, reporting the output as soon as it parses.
    async fn push_delta(&mut self, delta: &str, context: &RunContext) -> Result<(), StageError> {
        if delta.is_empty() {
            return Ok(());
        }
        if self.emitted.is_none() {
            // The rig contract: one mark per segment, at this stage's first
            // byte — the first content token, before any parsing.
            context.marks.mark(Stage::TranslateFirstToken);
        }
        self.content.push_str(delta);

        let parsed = match parse_model_output(&self.content) {
            Ok(parsed) => parsed,
            // Still incomplete: the JSON arrives in deltas.
            Err(_) => return Ok(()),
        };
        if self.emitted.as_ref() == Some(&parsed) {
            return Ok(());
        }
        let event = match &parsed {
            ModelOutput::Fragment { text, final_flag } => TranslatorEvent::Fragment {
                text: text.clone(),
                final_flag: *final_flag,
                provider: PROVIDER.to_string(),
                model_version: context.model(),
            },
            ModelOutput::Abstained { reason } => TranslatorEvent::Abstained { reason: *reason },
        };
        self.emitted = Some(parsed);
        let _ = context.sender.send(event).await;
        Ok(())
    }

    /// The English this run reported, when it reported one.
    fn english(&self) -> Option<String> {
        match &self.emitted {
            Some(ModelOutput::Fragment { text, .. }) if !text.trim().is_empty() => {
                Some(text.clone())
            }
            _ => None,
        }
    }
}

/// Decode the complete UTF-8 prefix of `pending`, leaving a trailing partial
/// character for the next chunk.
///
/// A TCP chunk can end inside a multi-byte character; `from_utf8_lossy` would
/// replace it with U+FFFD and corrupt the translation, so the tail waits.
fn take_valid_utf8(pending: &mut Vec<u8>) -> Result<String, StageError> {
    match std::str::from_utf8(pending) {
        Ok(text) => {
            let decoded = text.to_string();
            pending.clear();
            Ok(decoded)
        }
        Err(error) if error.error_len().is_none() => {
            let split = error.valid_up_to();
            let decoded = std::str::from_utf8(&pending[..split])
                .expect("the prefix is valid by construction")
                .to_string();
            pending.drain(..split);
            Ok(decoded)
        }
        Err(_) => Err(StageError::protocol(
            PROVIDER,
            "the response body is not UTF-8",
        )),
    }
}

/// The `usage` frame's counts, when the frame carries one.
fn usage_from(event: &Value) -> Option<TokenUsage> {
    let usage = event.get("usage")?.as_object()?;
    Some(TokenUsage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage.get("completion_tokens")?.as_u64()? as u32,
    })
}

/// The content delta of a chat-completions chunk, if this event is one.
fn content_delta(event: &Value) -> &str {
    event["choices"][0]["delta"]["content"]
        .as_str()
        .unwrap_or_default()
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

        let with_context = build_request_body(
            DEFAULT_MODEL,
            &fragment,
            &[],
            Some("The previous sentence."),
        );
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
        let parsed =
            parse_model_output("{\"t\":\"abstained\",\"reason\":\"unrecognized\"}").expect("valid");
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

    #[test]
    fn the_model_override_is_read_from_the_lookup_and_never_blank() {
        assert_eq!(configured_model(|_| None), DEFAULT_MODEL);
        assert_eq!(
            configured_model(|name| (name == MODEL_VAR).then(|| "deepseek-v4".to_string())),
            "deepseek-v4"
        );
        // A blank override is not a model id.
        assert_eq!(configured_model(|_| Some("   ".to_string())), DEFAULT_MODEL);
    }

    #[test]
    fn the_lookup_constructor_requires_the_key_and_takes_the_override() {
        let error = DeepseekTranslator::from_lookup(|_| None).expect_err("no key");
        assert!(error.to_string().contains("DEEPSEEK_API_KEY"), "{error}");

        let translator = DeepseekTranslator::from_lookup(|name| match name {
            "DEEPSEEK_API_KEY" => Some("sk-secret-value".to_string()),
            "DEEPSEEK_MODEL" => Some("deepseek-chat-v2".to_string()),
            _ => None,
        })
        .expect("configured");
        assert_eq!(translator.model(), "deepseek-chat-v2");
        assert!(!format!("{translator:?}").contains("sk-secret-value"));
    }

    #[test]
    fn a_chunk_boundary_inside_a_character_never_corrupts_it() {
        // The body arrives as bytes, and a chunk can end mid-character: here
        // the last byte of `。` is left for the next chunk.
        let sentence = "这条响应超过 800 毫秒。";
        let bytes = sentence.as_bytes().to_vec();
        let (head, tail) = bytes.split_at(bytes.len() - 1);
        let mut pending = head.to_vec();

        let first = take_valid_utf8(&mut pending).expect("the prefix decodes");
        assert_eq!(first, "这条响应超过 800 毫秒", "the partial tail waits");
        assert_eq!(
            pending.len(),
            2,
            "the bytes of the split character are kept"
        );

        pending.extend_from_slice(tail);
        let second = take_valid_utf8(&mut pending).expect("the tail completes");
        assert_eq!(second, "。");
        assert!(pending.is_empty(), "nothing is left behind");
        assert_eq!(format!("{first}{second}"), sentence, "never a U+FFFD");
    }

    #[test]
    fn a_body_that_is_not_utf8_is_a_retryable_protocol_failure() {
        let mut pending = vec![0xff, 0xfe];
        let error = take_valid_utf8(&mut pending).expect_err("not text");
        assert_eq!(error.retry_class, RetryClass::Retryable);
    }
}
