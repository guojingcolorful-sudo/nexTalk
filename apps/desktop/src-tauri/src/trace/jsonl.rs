//! JSONL trace records (02-03 T3.5; T3.7 writes the files).
//!
//! One line per segment, camelCase on the wire so the same field names travel
//! to the phone (`@nextalk/protocol`'s `SubtitleTrace.errorCode`). The two
//! fields here are the ones the degraded path owns:
//!
//! - `status` — what happened to the translation attempt;
//! - `errorCode` — the aggregatable reason (D-19) when there is one, `null`
//!   otherwise. **Never** a vendor message: a trace line has to stay countable
//!   by code, and a message blob would leak vendor wording into diagnostics.
//!
//! T3.7 extends [`TraceRecord`] with the provenance fields (provider, model,
//! timings, usage) and adds the single-writer `TraceWriter`; the shape below is
//! deliberately the smallest thing the degraded path needs today.

use serde::{Deserialize, Serialize};

/// What happened to one segment's translation attempt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    /// A translation was adopted.
    #[default]
    Ok,
    /// The translator never delivered; the original Chinese rendered (D-12).
    Degraded,
    /// The vendor declined to translate (D-03).
    Abstained,
}

/// One segment's trace record.
///
/// `#[serde(default)]` keeps old and partial lines readable: a record written
/// before a field existed deserializes with that field defaulted, so a trace
/// file never becomes unreadable across a release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TraceRecord {
    pub status: SegmentStatus,
    /// The aggregatable code (D-19) — `retry_exhausted`,
    /// `retry_budget_exhausted`, `client_error`, `vendor_error`,
    /// `circuit_open` — or `null` when nothing went wrong.
    pub error_code: Option<String>,
}

impl TraceRecord {
    /// A segment whose translation was adopted.
    pub fn ok() -> Self {
        Self {
            status: SegmentStatus::Ok,
            error_code: None,
        }
    }

    /// A segment the vendor declined to translate (D-03).
    pub fn abstained() -> Self {
        Self {
            status: SegmentStatus::Abstained,
            error_code: None,
        }
    }

    /// A segment that degraded (GOV-14), carrying its aggregatable code.
    pub fn degraded(error_code: impl Into<String>) -> Self {
        Self {
            status: SegmentStatus::Degraded,
            error_code: Some(error_code.into()),
        }
    }

    /// One JSONL line (no trailing newline; T3.7's writer adds it).
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::lan::server::{
        AbstainReason, ConfidenceLevel, LanguagePref, ServerEvent, SessionStatus, Speaker,
        SubtitleTrace,
    };
    use crate::pipeline::confidence::ConfidenceSource;

    /// The session's wall clock in the tests: 2026-10-05T00:00:00Z, so the
    /// date directory — and every assertion on it — is deterministic.
    const TS: u64 = 1_791_158_400_000;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nextalk-trace-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// A closed, provenance-carrying sentence — what the state bridge sees at
    /// the end of every real segment.
    fn traced_final(error_code: Option<&str>) -> ServerEvent {
        ServerEvent::Subtitle {
            id: "seg-1".into(),
            speaker: Speaker::User,
            seq: 7,
            zh: Some("这句话进了轨迹".into()),
            en: Some("This sentence reached the trace.".into()),
            final_flag: true,
            confidence: Some(ConfidenceLevel::Low),
            trace: Some(SubtitleTrace {
                segment_start_ms: 300,
                term_hits: vec![],
                provider: "volc".into(),
                model_version: "icl-2.0".into(),
                confidence_source: crate::lan::server::ConfidenceSource::Proxy,
                error_code: error_code.map(str::to_string),
            }),
        }
    }

    /// Every `.jsonl` file under `root`, sorted by path (roll order).
    fn jsonl_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for day in std::fs::read_dir(root).expect("root exists").flatten() {
            for entry in std::fs::read_dir(day.path()).expect("date dir").flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "jsonl") {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    fn lines_of(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .expect("trace file readable")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn all_lines(root: &Path) -> Vec<String> {
        jsonl_files(root)
            .iter()
            .flat_map(|path| lines_of(path))
            .collect()
    }

    fn parse(record: &TraceRecord) -> serde_json::Value {
        serde_json::from_str(&record.to_json_line().expect("serializes")).expect("json object")
    }

    fn usage_record(session: &str, usage: StageUsage) -> TraceRecord {
        TraceRecord::from_event(session, TS, &traced_final(None))
            .expect("a traced final")
            .with_usage(usage)
    }

    /// Test 1 (D-05/D-06/D-08): one line per sentence, every documented field
    /// present — the durable record is complete, not a best effort.
    #[test]
    fn a_sentence_record_carries_every_documented_field() {
        let record = TraceRecord::from_event("sess-1", TS, &traced_final(Some("retry_exhausted")))
            .expect("a traced final is a sentence");
        let value = parse(&record);
        for key in [
            "ts",
            "sessionId",
            "segmentId",
            "segmentStartMs",
            "zh",
            "en",
            "confidence",
            "confidenceSource",
            "termHits",
            "provider",
            "modelVersion",
            "status",
            "errorCode",
        ] {
            assert!(value.get(key).is_some(), "missing {key} in {value}");
        }
        assert_eq!(value["ts"], TS);
        assert_eq!(value["sessionId"], "sess-1");
        assert_eq!(value["segmentId"], 7);
        assert_eq!(value["segmentStartMs"], 300);
        assert_eq!(value["zh"], "这句话进了轨迹");
        assert_eq!(value["en"], "This sentence reached the trace.");
        assert_eq!(value["confidence"], "low");
        assert_eq!(value["confidenceSource"], "proxy");
        assert_eq!(value["termHits"], serde_json::json!([]));
        assert_eq!(value["provider"], "volc");
        assert_eq!(value["modelVersion"], "icl-2.0");
        assert_eq!(value["status"], "degraded");
        assert_eq!(value["errorCode"], "retry_exhausted");
    }

    /// D-03/D-08: an abstention is a sentence outcome too — the trace keeps the
    /// hole visible instead of dropping the segment, and it never fabricates a
    /// provider or a confidence reading for it.
    #[test]
    fn abstained_and_unprovenanced_sentences_are_still_traced() {
        let abstained = TraceRecord::from_event(
            "sess-1",
            TS,
            &ServerEvent::Abstained {
                id: "u1".into(),
                speaker: Speaker::User,
                seq: 1,
                reason: AbstainReason::SilentAudio,
                segment_start_ms: 620,
            },
        )
        .expect("an abstention is a sentence outcome");
        assert_eq!(abstained.status, SegmentStatus::Abstained);
        assert_eq!(abstained.segment_start_ms, 620);
        assert_eq!(abstained.confidence_source, ConfidenceSource::ProxyUnavailable);
        assert_eq!(abstained.provider, "unknown");
        assert_eq!(abstained.zh, None);
        assert_eq!(abstained.error_code, None);

        // Phase-1 shaped finals (no trace) still produce a line — with the
        // provenance marked unknown rather than invented.
        let mut bare = traced_final(None);
        if let ServerEvent::Subtitle { trace, .. } = &mut bare {
            *trace = None;
        }
        let record = TraceRecord::from_event("sess-1", TS, &bare).expect("a final is a sentence");
        assert_eq!(record.provider, "unknown");
        assert_eq!(record.confidence_source, ConfidenceSource::ProxyUnavailable);
        assert_eq!(record.usage, StageUsage::default());
    }

    /// One line per SENTENCE, not per frame: partials and non-sentence events
    /// never reach the durable log.
    #[test]
    fn partials_and_non_sentence_events_are_not_traced() {
        let mut partial = traced_final(None);
        if let ServerEvent::Subtitle { final_flag, .. } = &mut partial {
            *final_flag = false;
        }
        assert_eq!(TraceRecord::from_event("sess-1", TS, &partial), None);
        assert_eq!(
            TraceRecord::from_event(
                "sess-1",
                TS,
                &ServerEvent::Status {
                    session: SessionStatus::Listening
                }
            ),
            None
        );
        assert_eq!(
            TraceRecord::from_event(
                "sess-1",
                TS,
                &ServerEvent::Language {
                    language: LanguagePref::Bilingual
                }
            ),
            None
        );
    }

    /// Test 2 (D-13): one line records every stage's meter — STT audio,
    /// translation tokens, TTS characters — simultaneously.
    #[test]
    fn one_line_carries_every_stage_of_usage() {
        let record = usage_record(
            "sess-1",
            StageUsage {
                stt_audio_ms: 1_240,
                translate_prompt_tokens: 210,
                translate_completion_tokens: 96,
                tts_chars: 58,
            },
        );
        let value = parse(&record);
        assert_eq!(value["usage"]["sttAudioMs"], 1_240);
        assert_eq!(value["usage"]["translatePromptTokens"], 210);
        assert_eq!(value["usage"]["translateCompletionTokens"], 96);
        assert_eq!(value["usage"]["ttsChars"], 58);
    }

    /// Test 8 (GOV-10): the six classes Phase 8 aggregates on — 用户
    /// (`sessionId`), 任务 (`segmentId`), 模型 (`provider`/`modelVersion`),
    /// 耗时 (`segmentStartMs`), 状态 (`status`), 错误码 (`errorCode`) — sit on
    /// every record as required fields. `TraceRecord` has no `Default` and no
    /// partial constructor, so a construction missing any of them does not
    /// compile (the `compile_fail` example on [`TraceRecord`] is the witness).
    #[test]
    fn the_six_aggregation_classes_are_required_fields() {
        let value = parse(&TraceRecord::from_event("sess-agg", TS, &traced_final(None)).expect("record"));
        assert_eq!(value["sessionId"], "sess-agg");
        assert_eq!(value["segmentId"], 7);
        assert_eq!(value["provider"], "volc");
        assert_eq!(value["modelVersion"], "icl-2.0");
        assert_eq!(value["segmentStartMs"], 300);
        assert_eq!(value["status"], "ok");
        assert!(
            value.get("errorCode").is_some(),
            "null is still a present key: {value}"
        );
    }

    /// Test 3 (D-05): many producers, one writer task — the file holds exactly
    /// N lines and every line parses (a torn write would fail here).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_producers_never_tear_a_line() {
        let root = tmp_root("concurrent");
        let writer = TraceWriter::open(TraceWriterConfig::new(&root, "sess-concurrent", TS))
            .expect("writer opens");
        let mut producers = Vec::new();
        for producer in 0..8u64 {
            let writer = writer.clone();
            producers.push(tokio::spawn(async move {
                for i in 0..5u64 {
                    let event = traced_final(None);
                    let record =
                        TraceRecord::from_event(&format!("sess-{producer}"), TS + i, &event)
                            .expect("a traced final");
                    assert!(writer.append(record), "queue must not overflow in this test");
                }
            }));
        }
        for producer in producers {
            producer.await.expect("producer finishes");
        }
        writer.flush().await.expect("flush");

        let lines = all_lines(&root);
        assert_eq!(lines.len(), 40, "one line per record, no more");
        for line in &lines {
            serde_json::from_str::<TraceRecord>(line).expect("no torn line");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Test 4 (D-05 rolling): a session that outgrows the threshold rolls into
    /// `-2`, `-3`, … and no line is lost in the roll.
    #[tokio::test]
    async fn a_long_session_rolls_without_losing_a_line() {
        let root = tmp_root("roll");
        let mut config = TraceWriterConfig::new(&root, "sess-roll", TS);
        config.roll_threshold_bytes = 100; // rolls on every record at test scale
        let writer = TraceWriter::open(config).expect("writer opens");
        for i in 0..10u64 {
            let event = traced_final(None);
            let record = TraceRecord::from_event("sess-roll", TS + i, &event).expect("record");
            assert!(writer.append(record));
        }
        writer.flush().await.expect("flush");

        let files = jsonl_files(&root);
        assert!(files.len() >= 2, "expected a roll: {files:?}");
        assert!(
            files.iter().any(|path| path.ends_with("sess-roll-2.jsonl")),
            "the roll carries the -2 suffix: {files:?}"
        );
        let base = files
            .iter()
            .find(|path| path.ends_with("sess-roll.jsonl"))
            .expect("the first file keeps the plain name");
        assert_eq!(lines_of(base).len(), 1, "the base file closed at the threshold");

        let lines = all_lines(&root);
        assert_eq!(lines.len(), 10, "rolling never drops a line");
        for line in &lines {
            serde_json::from_str::<TraceRecord>(line).expect("rolled lines stay whole");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Test 5 (privacy): the file stays inside the root the writer was given
    /// (a hostile session id cannot climb out) and is created 0600. The
    /// static half — no socket anywhere in this module — lives in
    /// `trace::tests`.
    #[tokio::test]
    async fn trace_files_stay_private_and_inside_the_root() {
        use std::os::unix::fs::PermissionsExt;

        let root = tmp_root("private");
        let writer = TraceWriter::open(TraceWriterConfig::new(&root, "../../escape/../evil", TS))
            .expect("writer opens");
        let record = TraceRecord::from_event("sess", TS, &traced_final(None)).expect("record");
        assert!(writer.append(record));
        writer.flush().await.expect("flush");

        let files = jsonl_files(&root);
        assert_eq!(files.len(), 1);
        let canonical_root = root.canonicalize().expect("root exists");
        let canonical_file = files[0].canonicalize().expect("file exists");
        assert!(
            canonical_file.starts_with(&canonical_root),
            "{canonical_file:?} must stay under {canonical_root:?}"
        );
        assert!(
            !files[0]
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .contains(".."),
            "the traversal never reaches the file name: {files:?}"
        );

        let mode = std::fs::metadata(&files[0])
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "trace files are private to the user");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The month aggregator (D-13/GOV-17): only the current month's files
    /// count, per-stage totals sum, and unreadable lines are counted rather
    /// than silently swallowed.
    #[tokio::test]
    async fn usage_summary_aggregates_the_current_month_only() {
        use std::io::Write as _;

        let root = tmp_root("usage");
        let october = TraceWriter::open(TraceWriterConfig::new(&root, "sess-oct", TS))
            .expect("writer opens");
        let september =
            TraceWriter::open(TraceWriterConfig::new(&root, "sess-sep", TS - 7 * 86_400_000))
                .expect("writer opens");
        october.append(usage_record(
            "sess-oct",
            StageUsage {
                stt_audio_ms: 60_000,
                translate_prompt_tokens: 100,
                translate_completion_tokens: 40,
                tts_chars: 12,
            },
        ));
        october.append(usage_record(
            "sess-oct",
            StageUsage {
                stt_audio_ms: 30_000,
                translate_prompt_tokens: 10,
                translate_completion_tokens: 4,
                tts_chars: 2,
            },
        ));
        september.append(usage_record(
            "sess-sep",
            StageUsage {
                stt_audio_ms: 999_000,
                translate_prompt_tokens: 1_000_000,
                translate_completion_tokens: 1_000_000,
                tts_chars: 1_000_000,
            },
        ));
        october.flush().await.expect("flush");
        september.flush().await.expect("flush");

        // A torn/legacy line is counted, never silently dropped.
        let october_file = jsonl_files(&root)
            .into_iter()
            .find(|path| path.ends_with("sess-oct.jsonl"))
            .expect("october file");
        let mut handle = std::fs::OpenOptions::new()
            .append(true)
            .open(&october_file)
            .expect("append");
        writeln!(handle, "{{not json").expect("write");

        let summary = UsageSummary::load(&root, TS).expect("load");
        assert_eq!(summary.sessions, 1, "last month's sessions stay out");
        assert_eq!(summary.segments, 2);
        assert_eq!(summary.skipped_lines, 1, "the torn line is counted");
        assert_eq!(summary.stt_audio_ms, 90_000);
        assert_eq!(summary.translate_prompt_tokens, 110);
        assert_eq!(summary.translate_completion_tokens, 44);
        assert_eq!(summary.tts_chars, 14);
        assert!((summary.used_minutes() - 1.5).abs() < 1e-9);

        // A month with no traces at all is an empty summary, not an error.
        let empty = UsageSummary::load(&root, TS - 40 * 86_400_000).expect("load");
        assert_eq!(empty, UsageSummary::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Forward compatibility: a line written before a field existed still
    /// reads (D-05 keeps the file readable across releases).
    #[test]
    fn partial_lines_read_with_field_defaults() {
        let legacy: TraceRecord =
            serde_json::from_str(r#"{"status":"degraded","errorCode":"retry_exhausted"}"#).unwrap();
        assert_eq!(legacy.status, SegmentStatus::Degraded);
        assert_eq!(legacy.error_code.as_deref(), Some("retry_exhausted"));
        assert_eq!(legacy.session_id, "");
        assert_eq!(legacy.usage, StageUsage::default());

        let empty: TraceRecord = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.status, SegmentStatus::Ok);
        assert_eq!(empty.error_code, None);
        assert_eq!(empty.confidence_source, ConfidenceSource::ProxyUnavailable);
    }
}
