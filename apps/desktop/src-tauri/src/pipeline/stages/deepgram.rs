//! Deepgram Nova-3 — the interviewer's English line (T2.3).
//!
//! Ported from `tools/vendor-experiments/stt-ab.mjs` (the Deepgram half).
//!
//! # Why this stage is subtitle-only
//!
//! This is the [`InterviewerTrack`]: whatever the interviewer says is shown as
//! subtitles and fed to the answer-strategy agent, but it is **never** fed to
//! the voice sink — only the user's own Chinese line is re-voiced. The cascade
//! enforces that in 02-03; this module marks it in the types.
//!
//! # Deliberate omissions
//!
//! - **No `Stage::SttFirstPartial` mark.** The 02-01 rig measures the *user*
//!   path end to end; the interviewer line is a side channel, and firing its
//!   boundary into the same waterfall would corrupt the budget math. The
//!   decision is asserted by a test that watches the handle stay silent.
//! - `endpointing` and `utterance_end_ms` are configured but the *cadence* of
//!   silence detection is the vendor's; [`TranscriptBuffer`] only implements
//!   what happens when the vendor says a run ended.
//!
//! # Protocol traps this file encodes
//!
//! - `language=multi` does **not** include Chinese and silently produces
//!   garbage, so the language is pinned to `en` (research correction 2). The
//!   URL test is a guard, not a style preference.
//! - Auth is the `Token <key>` scheme — `Bearer` is an instant 401.
//! - A session that sends neither audio nor `KeepAlive` for 10 s is closed
//!   with NET-0001 (research correction 4); the client must recognise that
//!   close and treat it as a stale link, not a successful end of stream.

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// Deepgram's own recommendation: the smallest interval that reliably keeps a
/// silent session open.
pub const KEEPALIVE_INTERVAL_MS: u64 = 3_000;

/// The service closes a session after 10 s without audio **and** without a
/// `KeepAlive` (NET-0001).
pub const SILENCE_CLOSE_MS: u64 = 10_000;

/// Silence this long after speech ends the utterance (`UtteranceEnd`).
pub const DEFAULT_UTTERANCE_END_MS: u32 = 1_000;

/// Endpointing: how long a pause must last for Deepgram to close a run.
pub const ENDPOINTING_MS: u32 = 300;

/// The interviewer line is 16 kHz mono — the same format 02-05's resampler
/// produces for the user path.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// The **only** language this client may request (research correction 2).
pub const LANGUAGE: &str = "en";

/// Default model, mirroring `StageRole::InterviewerStt::model()`.
pub const DEFAULT_MODEL: &str = "nova-3";

/// The heartbeat's exact shape. Deepgram accepts only this literal.
pub const KEEPALIVE_FRAME: &str = r#"{"type":"KeepAlive"}"#;

/// Sent once the fragment ends so the vendor flushes its last run.
pub const CLOSE_STREAM_FRAME: &str = r#"{"type":"CloseStream"}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::stages::traits::{ConfidenceSource, InterviewerTrack};

    #[test]
    fn the_listen_url_pins_the_language_and_never_asks_for_multi() {
        // Test 1: `multi` silently produces garbage for Chinese and is not a
        // valid language bundle for this endpoint. This is the guard.
        let url = listen_url("wss://api.deepgram.com/v1/listen", &ListenParams::default())
            .expect("the default endpoint parses");
        assert!(url.contains("language=en"), "{url}");
        assert!(!url.contains("language=multi"), "{url}");
    }

    #[test]
    fn the_listen_url_locks_every_parameter_the_cascade_depends_on() {
        // Test 2
        let url = listen_url("wss://api.deepgram.com/v1/listen", &ListenParams::default())
            .expect("parses");
        for expected in [
            "model=nova-3",
            "encoding=linear16",
            "sample_rate=16000",
            "channels=1",
            "interim_results=true",
            "endpointing=300",
            "vad_events=true",
            "utterance_end_ms=1000",
            "punctuate=true",
        ] {
            assert!(url.contains(expected), "{expected} missing from {url}");
        }
    }

    #[test]
    fn keepalive_and_close_stream_are_the_documented_literals() {
        assert_eq!(KEEPALIVE_FRAME, r#"{"type":"KeepAlive"}"#);
        assert_eq!(CLOSE_STREAM_FRAME, r#"{"type":"CloseStream"}"#);
        assert_eq!(
            KEEPALIVE_INTERVAL_MS, 3_000,
            "the named constant is the cadence"
        );
    }

    #[test]
    fn the_transcript_buffer_commits_only_on_a_flush() {
        // Test 4: an `is_final` run is *buffered*; only `speech_final` (or an
        // `UtteranceEnd` fallback) commits the sentence.
        let mut buffer = TranscriptBuffer::new();
        buffer.push_run("Could you walk");
        assert_eq!(buffer.text(), "Could you walk");
        assert!(!buffer.is_empty());
        buffer.push_run("me through the steps");
        assert_eq!(buffer.text(), "Could you walk me through the steps");
        assert_eq!(
            buffer.flush(),
            "Could you walk me through the steps",
            "the flush returns the whole utterance"
        );
        assert!(buffer.is_empty(), "a flush resets the buffer");
    }

    #[test]
    fn an_interim_run_is_replaced_not_appended() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push_interim("Could you");
        buffer.push_interim("Could you walk");
        assert_eq!(buffer.text(), "Could you walk", "interim text is volatile");
    }

    #[test]
    fn the_net_0001_close_reason_is_recognised_as_a_silent_timeout() {
        assert!(is_silent_timeout("NET-0001: No audio received for 10s"));
        assert!(!is_silent_timeout("going away"));
        assert_eq!(SILENCE_CLOSE_MS, 10_000);
    }

    #[test]
    fn the_confidence_is_only_reported_when_the_vendor_sends_one() {
        let with_score = Alternative {
            transcript: "hello".to_string(),
            confidence: Some(0.98),
        };
        let partial = partial_from_run("hello", true, false, Some(&with_score), "nova-3");
        assert_eq!(partial.confidence, Some(0.98));
        assert_eq!(partial.confidence_source, ConfidenceSource::Vendor);

        let without = Alternative {
            transcript: "hello".to_string(),
            confidence: None,
        };
        let partial = partial_from_run("hello", true, false, Some(&without), "nova-3");
        assert_eq!(partial.confidence, None, "no score is invented");
    }

    #[test]
    fn the_model_version_comes_from_the_frame_metadata() {
        // Test 9 (D-08): the version reported in traces is the one the service
        // actually ran, not the one we asked for.
        let metadata = serde_json::json!({
            "request_id": "dg-1",
            "model_info": { "name": "nova-3", "version": "2026-01-15", "arch": "chirp-3" }
        });
        assert_eq!(
            model_version_from(&metadata).as_deref(),
            Some("nova-3 2026-01-15 chirp-3")
        );
        assert_eq!(model_version_from(&serde_json::json!({})), None);
    }

    #[test]
    fn the_interviewer_track_marker_is_the_type_not_a_comment() {
        // The cascade's TTS sink accepts user fragments only; this marker is
        // how 02-03 keeps the interviewer line out of it.
        let _marker = InterviewerTrack;
    }
}
