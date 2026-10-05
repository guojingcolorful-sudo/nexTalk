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
    use super::*;

    /// The wire shape is the contract: camelCase, snake_case enum values, and
    /// `null` — not an omitted key — when nothing failed.
    #[test]
    fn trace_records_serialize_to_the_wire_shape() {
        let ok = TraceRecord::ok().to_json_line().expect("serializes");
        assert_eq!(ok, r#"{"status":"ok","errorCode":null}"#);

        let degraded = TraceRecord::degraded("retry_exhausted")
            .to_json_line()
            .expect("serializes");
        assert_eq!(
            degraded,
            r#"{"status":"degraded","errorCode":"retry_exhausted"}"#
        );

        let abstained = TraceRecord::abstained().to_json_line().expect("serializes");
        assert_eq!(abstained, r#"{"status":"abstained","errorCode":null}"#);
    }

    /// Forward compatibility: a line written before a field existed still
    /// reads.
    #[test]
    fn trace_records_read_partial_lines_with_defaults() {
        let record: TraceRecord = serde_json::from_str(r#"{"errorCode":"circuit_open"}"#).unwrap();
        assert_eq!(record.status, SegmentStatus::Ok);
        assert_eq!(record.error_code.as_deref(), Some("circuit_open"));

        let empty: TraceRecord = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, TraceRecord::ok());
    }
}
