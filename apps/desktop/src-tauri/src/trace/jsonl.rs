//! JSONL trace records and the single writer that persists them (02-03
//! T3.5/T3.7).
//!
//! One line per SENTENCE (frames never land here — the live path is the
//! timeline, this file is the durable record), camelCase on the wire so the
//! same field names travel to the phone (`@nextalk/protocol`'s
//! `SubtitleTrace.errorCode`). A line answers four questions:
//!
//! - **what was said** — `zh`/`en` as the segment closed;
//! - **what happened** — `status` plus the aggregatable `errorCode` (D-19)
//!   or `null`. **Never** a vendor message: a trace line has to stay
//!   countable by code, and a message blob would leak vendor wording into
//!   diagnostics;
//! - **who produced it** — `provider`/`modelVersion` and the
//!   `confidence` + `confidenceSource` provenance pair (GOV-06), `unknown`
//!   and `proxy_unavailable` when the segment carried none — never a
//!   fabricated value;
//! - **what it cost** — the per-stage [`StageUsage`] meter (D-13).
//!
//! GOV-10's six aggregation classes are required fields: 用户 (`sessionId`),
//! 任务 (`segmentId`), 模型 (`provider`/`modelVersion`), 耗时
//! (`segmentStartMs`), 状态 (`status`), 错误码 (`errorCode`). [`TraceRecord`]
//! has no `Default` and no partial constructor — [`TraceRecord::from_event`]
//! is the only bridge — so a record missing a class does not compile:
//!
//! ```compile_fail
//! # use nextalk_desktop_lib::trace::jsonl::{SegmentStatus, TraceRecord};
//! // No sessionId/segmentId/provider/... — and no Default to paper over
//! // them: the aggregation classes cannot be omitted.
//! let _ = TraceRecord { ts: 0, status: SegmentStatus::Ok, ..Default::default() };
//! ```
//!
//! [`TraceWriter`] is the ONE writer per session: producers hand records to a
//! bounded queue and a single task owns the `File`. Records arrive whole or
//! not at all (concurrent producers cannot tear a line), files roll at
//! [`ROLL_THRESHOLD_BYTES`] instead of growing without bound, and a full queue
//! is counted ([`TraceWriter::dropped_records`]) rather than blocking the
//! audio path. Files are created 0600 under `traces/<YYYY-MM-DD>/`; nothing in
//! this module opens a socket (`trace::tests` scans the source, and the
//! containment test proves a hostile session id cannot escape the root).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::lan::server::{ConfidenceLevel, ServerEvent};
use crate::pipeline::confidence::ConfidenceSource;
use crate::trace::date_dir_name;

/// Roll the open file once it reaches 8 MiB: a long session stays in
/// reviewable chunks instead of one unbounded file (D-05).
pub const ROLL_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;

/// The writer queue's bound. A producer that meets a full queue has its record
/// counted as dropped instead of blocking the audio path — the same
/// anti-pattern guard the playout queue applies to rendered audio.
pub const WRITER_QUEUE_CAPACITY: usize = 1024;

/// Longest file-slug a session id can produce.
const SLUG_MAX_LEN: usize = 64;

/// The provenance marker for a segment that carried none (Phase-1 shaped
/// sources): honest, countable, and never mistaken for a vendor id.
const UNKNOWN_PROVENANCE: &str = "unknown";

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

/// Per-stage usage meters for one segment (D-13): the STT audio duration, the
/// translation token counts and the TTS character count — all on the same line
/// so a month sums without joining anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StageUsage {
    pub stt_audio_ms: u64,
    pub translate_prompt_tokens: u64,
    pub translate_completion_tokens: u64,
    pub tts_chars: u64,
}

/// One segment's trace record.
///
/// Field-level `#[serde(default)]` keeps old and partial lines readable: a
/// record written before a field existed deserializes with that field
/// defaulted, so a trace file never becomes unreadable across a release. It
/// deliberately does NOT derive `Default` — construction goes through
/// [`TraceRecord::from_event`], and GOV-10's classes cannot be omitted
/// (see the module docs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRecord {
    /// Wall-clock millis at which the sentence closed.
    #[serde(default)]
    pub ts: u64,
    /// GOV-10 用户 class.
    #[serde(default)]
    pub session_id: String,
    /// GOV-10 任务 class — the subtitle `seq` the segment spoke under.
    #[serde(default)]
    pub segment_id: u64,
    /// Where the segment's audio started (GOV-10 耗时 class).
    #[serde(default)]
    pub segment_start_ms: u64,
    #[serde(default)]
    pub zh: Option<String>,
    #[serde(default)]
    pub en: Option<String>,
    /// The reading the segment carried, exactly as read — or `null`.
    #[serde(default)]
    pub confidence: Option<ConfidenceLevel>,
    /// Where `confidence` came from (GOV-06).
    #[serde(default)]
    pub confidence_source: ConfidenceSource,
    #[serde(default)]
    pub term_hits: Vec<crate::lan::server::TermHit>,
    /// GOV-10 模型 class — the vendor id (`volc`, `deepgram`, …) or
    /// [`UNKNOWN_PROVENANCE`].
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model_version: String,
    #[serde(default)]
    pub status: SegmentStatus,
    /// The aggregatable code (D-19) — `retry_exhausted`,
    /// `retry_budget_exhausted`, `client_error`, `vendor_error`,
    /// `circuit_open` — or `null` when nothing went wrong.
    #[serde(default)]
    pub error_code: Option<String>,
    /// The segment's per-stage meter; zeroes until the pipeline attaches the
    /// real numbers (nothing here fabricates a quantity).
    #[serde(default)]
    pub usage: StageUsage,
}

impl TraceRecord {
    /// Builds the durable record for one closed sentence — the bridge the
    /// session state uses in `append_event` (D-18/GOV-10): the timeline and
    /// the JSONL file are one model, so no second instrumentation path exists.
    ///
    /// `None` for anything that is not a sentence outcome: frames, statuses,
    /// language echoes, strategy cards.
    pub fn from_event(session_id: &str, ts: u64, event: &ServerEvent) -> Option<Self> {
        match event {
            ServerEvent::Subtitle {
                seq,
                zh,
                en,
                final_flag: true,
                confidence,
                trace,
                ..
            } => {
                let (
                    provider,
                    model_version,
                    segment_start_ms,
                    term_hits,
                    confidence_source,
                    error_code,
                ) = match trace {
                    Some(trace) => (
                        trace.provider.clone(),
                        trace.model_version.clone(),
                        trace.segment_start_ms,
                        trace.term_hits.clone(),
                        from_wire_source(trace.confidence_source),
                        trace.error_code.clone(),
                    ),
                    None => (
                        UNKNOWN_PROVENANCE.to_string(),
                        UNKNOWN_PROVENANCE.to_string(),
                        0,
                        Vec::new(),
                        ConfidenceSource::ProxyUnavailable,
                        None,
                    ),
                };
                Some(Self {
                    ts,
                    session_id: session_id.to_string(),
                    segment_id: *seq,
                    segment_start_ms,
                    zh: zh.clone(),
                    en: en.clone(),
                    confidence: *confidence,
                    confidence_source,
                    term_hits,
                    provider,
                    model_version,
                    status: if error_code.is_some() {
                        SegmentStatus::Degraded
                    } else {
                        SegmentStatus::Ok
                    },
                    error_code,
                    usage: StageUsage::default(),
                })
            }
            ServerEvent::Abstained {
                seq,
                segment_start_ms,
                ..
            } => Some(Self {
                ts,
                session_id: session_id.to_string(),
                segment_id: *seq,
                segment_start_ms: *segment_start_ms,
                zh: None,
                en: None,
                confidence: None,
                // The absence is the value (GOV-06/T-02-10): an abstention
                // has no reading, so none is written.
                confidence_source: ConfidenceSource::ProxyUnavailable,
                term_hits: Vec::new(),
                provider: UNKNOWN_PROVENANCE.to_string(),
                model_version: UNKNOWN_PROVENANCE.to_string(),
                status: SegmentStatus::Abstained,
                error_code: None,
                usage: StageUsage::default(),
            }),
            _ => None,
        }
    }

    /// Attaches the segment's per-stage meter (D-13). The pipeline owns the
    /// real numbers and hands them over here; a record that never receives one
    /// keeps zeroes on purpose — no fabricated quantities.
    pub fn with_usage(mut self, usage: StageUsage) -> Self {
        self.usage = usage;
        self
    }

    /// One JSONL line (no trailing newline; the writer adds it).
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Maps the wire vocabulary (`lan::server`) onto the internal source type.
fn from_wire_source(source: crate::lan::server::ConfidenceSource) -> ConfidenceSource {
    match source {
        crate::lan::server::ConfidenceSource::Vendor => ConfidenceSource::Vendor,
        crate::lan::server::ConfidenceSource::Proxy => ConfidenceSource::Proxy,
    }
}

/// File-slug safety: keep `[A-Za-z0-9_-]`, map everything else to `-`, collapse
/// runs and cap the length. A hostile session id can neither climb out of the
/// root nor pick a name the filesystem treats specially.
fn sanitize_slug(session_id: &str) -> String {
    let mapped: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(SLUG_MAX_LEN)
        .collect();
    let mut slug = mapped;
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "session".to_string()
    } else {
        slug
    }
}

/// Trace persistence failures. Never fatal to a session: the caller keeps
/// running, and the writer's counters stay the visible signal.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("trace io: {0}")]
    Io(#[from] std::io::Error),
    #[error("trace writer task is gone")]
    Closed,
}

/// The session file naming and roll policy (T3.7).
#[derive(Debug, Clone)]
pub struct TraceWriterConfig {
    /// The `traces` root (the app data dir in production).
    pub root_dir: PathBuf,
    /// Session identity — sanitized into the file slug; the RECORD's
    /// `sessionId` travels untrimmed (it is data, not a path).
    pub session_id: String,
    /// Session-start wall clock; picks `traces/<YYYY-MM-DD>/`.
    pub start_time_ms: u64,
    /// Roll when the open file would exceed this size.
    pub roll_threshold_bytes: u64,
    /// Bounded queue capacity.
    pub queue_capacity: usize,
}

impl TraceWriterConfig {
    /// Production defaults for the two test overrides.
    pub fn new(
        root_dir: impl AsRef<Path>,
        session_id: impl Into<String>,
        start_time_ms: u64,
    ) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            session_id: session_id.into(),
            start_time_ms,
            roll_threshold_bytes: ROLL_THRESHOLD_BYTES,
            queue_capacity: WRITER_QUEUE_CAPACITY,
        }
    }
}

/// One message to the writer task.
enum WriterMessage {
    Record(String),
    Flush(oneshot::Sender<()>),
}

/// The single writer for one session: [`Self::append`] is a non-blocking
/// hand-off to the one task that owns the file. Cheap to clone; every clone
/// feeds the same queue and the same drop counter.
#[derive(Clone)]
pub struct TraceWriter {
    session_id: Arc<String>,
    tx: mpsc::Sender<WriterMessage>,
    dropped: Arc<AtomicU64>,
    write_failures: Arc<AtomicU64>,
}

impl TraceWriter {
    /// Opens the session's trace file and starts its writer task. Creates
    /// `root/<date>/` as needed; the file is created 0600 and stays inside the
    /// root (the slug cannot escape it). An existing file from a crash is
    /// appended to, never truncated.
    pub fn open(config: TraceWriterConfig) -> Result<Self, TraceError> {
        let dir = config.root_dir.join(date_dir_name(config.start_time_ms));
        std::fs::create_dir_all(&dir)?;
        let slug = sanitize_slug(&config.session_id);
        let task = WriterTask::open(
            dir,
            slug,
            config.roll_threshold_bytes,
            Arc::new(AtomicU64::new(0)),
        )?;
        let (tx, rx) = mpsc::channel(config.queue_capacity.max(1));
        let dropped = task.dropped.clone();
        let write_failures = task.write_failures.clone();
        spawn_writer(rx, task);
        Ok(Self {
            session_id: Arc::new(config.session_id),
            tx,
            dropped,
            write_failures,
        })
    }

    /// Hands a record to the writer. `true` when queued; `false` when the
    /// queue is full or the writer is gone — the record is counted in
    /// [`Self::dropped_records`], never blocking the caller (the audio path
    /// must not wait on disk).
    pub fn append(&self, record: TraceRecord) -> bool {
        let queued = record.to_json_line().map_err(|_| ()).and_then(|line| {
            self.tx
                .try_send(WriterMessage::Record(line))
                .map_err(|_| ())
        });
        match queued {
            Ok(()) => true,
            Err(()) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Waits until every record queued so far is on disk (tests and shutdown).
    pub async fn flush(&self) -> Result<(), TraceError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(WriterMessage::Flush(ack_tx))
            .await
            .map_err(|_| TraceError::Closed)?;
        ack_rx.await.map_err(|_| TraceError::Closed)
    }

    /// The session identity stamped into every record.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Records the queue refused (full or closed) — a visible undercount
    /// instead of a silently lost segment.
    pub fn dropped_records(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Disk writes that failed after the record was queued.
    pub fn write_failures(&self) -> u64 {
        self.write_failures.load(Ordering::Relaxed)
    }
}

/// The task side: owns the open file, its size and its roll index.
struct WriterTask {
    dir: PathBuf,
    slug: String,
    path: PathBuf,
    file: std::fs::File,
    index: u64,
    written_bytes: u64,
    roll_threshold_bytes: u64,
    dropped: Arc<AtomicU64>,
    write_failures: Arc<AtomicU64>,
}

impl WriterTask {
    fn open(
        dir: PathBuf,
        slug: String,
        roll_threshold_bytes: u64,
        dropped: Arc<AtomicU64>,
    ) -> Result<Self, TraceError> {
        let path = dir.join(format!("{slug}.jsonl"));
        let file = open_private(&path)?;
        let written_bytes = file.metadata()?.len();
        Ok(Self {
            dir,
            slug,
            path,
            file,
            index: 1,
            written_bytes,
            roll_threshold_bytes,
            dropped,
            write_failures: Arc::new(AtomicU64::new(0)),
        })
    }

    fn path_for(&self, index: u64) -> PathBuf {
        if index == 1 {
            self.dir.join(format!("{}.jsonl", self.slug))
        } else {
            self.dir.join(format!("{}-{}.jsonl", self.slug, index))
        }
    }

    /// Appends one whole line. Rolls FIRST when the open file is non-empty and
    /// would exceed the threshold, so a roll never leaves an empty file behind
    /// and never splits a line.
    fn write_line(&mut self, line: &str) -> Result<(), std::io::Error> {
        let encoded_len = line.len() as u64 + 1;
        if self.written_bytes > 0 && self.written_bytes + encoded_len > self.roll_threshold_bytes {
            self.roll()?;
        }
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.written_bytes += encoded_len;
        Ok(())
    }

    fn roll(&mut self) -> Result<(), std::io::Error> {
        self.file.flush()?;
        self.index += 1;
        self.path = self.path_for(self.index);
        self.file = open_private(&self.path)?;
        self.written_bytes = self.file.metadata()?.len();
        Ok(())
    }
}

/// Creates/opens a trace file private to the user (0600).
fn open_private(path: &Path) -> Result<std::fs::File, std::io::Error> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

/// Spawns the writer task on the ambient runtime when there is one (Tauri
/// commands run on tokio); otherwise falls back to Tauri's async runtime so a
/// session started off a plain thread still gets its writer.
fn spawn_writer(rx: mpsc::Receiver<WriterMessage>, task: WriterTask) {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(run_writer(rx, task));
        }
        Err(_) => {
            tauri::async_runtime::spawn(run_writer(rx, task));
        }
    }
}

async fn run_writer(mut rx: mpsc::Receiver<WriterMessage>, mut task: WriterTask) {
    while let Some(message) = rx.recv().await {
        match message {
            WriterMessage::Record(line) => {
                if let Err(err) = task.write_line(&line) {
                    task.write_failures.fetch_add(1, Ordering::Relaxed);
                    eprintln!("trace write failed: {err}");
                }
            }
            WriterMessage::Flush(ack) => {
                if let Err(err) = task.file.flush() {
                    task.write_failures.fetch_add(1, Ordering::Relaxed);
                    eprintln!("trace flush failed: {err}");
                }
                let _ = ack.send(());
            }
        }
    }
    // Every sender is gone (the session ended): whatever is still buffered is
    // drained before the task exits — a stop never loses a queued line.
    let _ = task.file.flush();
}

/// The current month's totals (D-13/GOV-17), summed from the JSONL traces on
/// demand — there is no in-memory counter that could drift from the files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageSummary {
    pub sessions: u64,
    pub segments: u64,
    /// Lines that did not parse (a torn write, a future format) — counted, so
    /// an undercount is visible instead of silent.
    pub skipped_lines: u64,
    pub stt_audio_ms: u64,
    pub translate_prompt_tokens: u64,
    pub translate_completion_tokens: u64,
    pub tts_chars: u64,
}

impl UsageSummary {
    /// Sums every `.jsonl` under `root/<current month>/` — `now_ms` picks the
    /// month (and makes the aggregation testable). A missing directory is an
    /// empty month, not an error.
    pub fn load(root_dir: &Path, now_ms: u64) -> Result<Self, TraceError> {
        let mut summary = Self::default();
        let mut session_ids = std::collections::BTreeSet::new();
        let month_prefix = date_dir_name(now_ms);
        let month_prefix = &month_prefix[..7]; // "YYYY-MM"

        let days = match std::fs::read_dir(root_dir) {
            Ok(days) => days,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(summary),
            Err(err) => return Err(err.into()),
        };
        for day in days.flatten() {
            if !day.file_name().to_string_lossy().starts_with(month_prefix) {
                continue;
            }
            let files = match std::fs::read_dir(day.path()) {
                Ok(files) => files,
                Err(_) => continue,
            };
            for file in files.flatten() {
                let path = file.path();
                if !path.extension().is_some_and(|ext| ext == "jsonl") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    summary.skipped_lines += 1;
                    continue;
                };
                for line in content.lines() {
                    match serde_json::from_str::<TraceRecord>(line) {
                        Ok(record) => {
                            summary.segments += 1;
                            session_ids.insert(record.session_id);
                            summary.stt_audio_ms += record.usage.stt_audio_ms;
                            summary.translate_prompt_tokens += record.usage.translate_prompt_tokens;
                            summary.translate_completion_tokens +=
                                record.usage.translate_completion_tokens;
                            summary.tts_chars += record.usage.tts_chars;
                        }
                        Err(_) => summary.skipped_lines += 1,
                    }
                }
            }
        }
        summary.sessions = session_ids.len() as u64;
        Ok(summary)
    }

    /// The STT audio in minutes — the quota unit for the 套餐额度 panel (T3.9).
    pub fn used_minutes(&self) -> f64 {
        self.stt_audio_ms as f64 / 60_000.0
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
        assert_eq!(
            abstained.confidence_source,
            ConfidenceSource::ProxyUnavailable
        );
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
        let value =
            parse(&TraceRecord::from_event("sess-agg", TS, &traced_final(None)).expect("record"));
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
                    assert!(
                        writer.append(record),
                        "queue must not overflow in this test"
                    );
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

    /// The queue is BOUNDED (failure-case 0016 队列无界增长): a full queue
    /// refuses the record and counts it — the producer never blocks on disk,
    /// and the loss is visible via `dropped_records()` instead of an unbounded
    /// backlog growing in memory. On this current-thread runtime the writer
    /// task cannot drain while the test body runs, so the two-slot queue
    /// deterministically refuses the third record.
    #[tokio::test]
    async fn a_full_queue_drops_counted_records_instead_of_blocking() {
        let root = tmp_root("bounded");
        let mut config = TraceWriterConfig::new(&root, "sess-bounded", TS);
        config.queue_capacity = 2;
        let writer = TraceWriter::open(config).expect("writer opens");

        let mut accepted = Vec::new();
        for i in 0..3u64 {
            let record = TraceRecord::from_event("sess-bounded", TS + i, &traced_final(None))
                .expect("record");
            accepted.push(writer.append(record));
        }
        assert_eq!(
            accepted,
            vec![true, true, false],
            "a two-slot queue admits exactly two"
        );
        assert_eq!(
            writer.dropped_records(),
            1,
            "the refusal is counted, not silent"
        );

        writer.flush().await.expect("flush");
        assert_eq!(
            all_lines(&root).len(),
            2,
            "only the queued records landed; the drop never blocked the producer"
        );
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
        assert_eq!(
            lines_of(base).len(),
            1,
            "the base file closed at the threshold"
        );

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
        let october =
            TraceWriter::open(TraceWriterConfig::new(&root, "sess-oct", TS)).expect("writer opens");
        let september = TraceWriter::open(TraceWriterConfig::new(
            &root,
            "sess-sep",
            TS - 7 * 86_400_000,
        ))
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
