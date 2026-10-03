//! 火山 Seed-ICL 2.0 streaming synthesis — English text in, the user's cloned
//! voice out (T2.5).
//!
//! Ported field-by-field from `tools/vendor-experiments/volc-tts-stream.mjs`,
//! the script that was verified against the service on 2026-09-29.
//!
//! # Wire format
//!
//! ```text
//! client → one binary frame, then read:
//!   [0x11, 0x10, 0x10, 0x00] + u32be(len) + JSON
//!   {"user":{"uid"},"req_params":{"text","speaker","audio_params":{...}}}
//!   handshake: X-Api-Key / X-Api-Resource-Id / X-Api-Request-Id
//!
//! server → [0x11, msgType<<4, 0x10, 0x00] + …
//!   msgType = (data[1] >> 4) & 0x0f   0b1111 error | 0b1011 audio | 0b1001 json
//!   error:  u32be(code) @4, u32be(size) @8, message
//!   other:  u32be(event) @4, u32be(sid_len) @8, sid, u32be(payload_len), payload
//!   event 352 = audio for this request, 152 = session finished
//! ```
//!
//! # The cross-lingual risk (research correction 5)
//!
//! `.env.example` says an ICL 2.0 clone "only supports synthesis in the same
//! language as the training audio", while the synthesis parameters document a
//! cross-lingual path. **The two keys below are that path**: with
//! `explicit_language: "en"` and `tone_fidelity: false` an English sentence
//! goes to a clone trained on Chinese. This client sends them explicitly — it
//! never relies on a server default — but it does **not** assume they work:
//! the probe in 02-04 (T4.0) is the arbiter, and if it fails, D-11's
//! single-vendor conclusion fails with it.
//!
//! `format` is `pcm`, not the experiment script's `mp3`: the cascade consumes
//! PCM directly (24 kHz mono, the rate 02-05's playout chain resamples from).

use serde_json::{json, Value};

use super::config::VolcCredentials;
use super::error::{ErrorKind, StageError};
use super::traits::{TtsUsage, VoiceRef};

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// Synthesis sample rate: 24 kHz mono PCM16LE.
pub const SAMPLE_RATE_HZ: u32 = 24_000;

pub const AUDIO_FORMAT: &str = "pcm";

/// The synthesis text's language. Cross-lingual path — see the module docs.
pub const EXPLICIT_LANGUAGE: &str = "en";

/// `tone_fidelity: false` (还原模式) is **mandatory** for cross-lingual text.
pub const TONE_FIDELITY: bool = false;

/// `X-Api-Resource-Id` for a cloned voice.
pub const RESOURCE_CLONE: &str = "seed-icl-2.0";

/// `X-Api-Resource-Id` for a preset voice.
pub const RESOURCE_PRESET: &str = "seed-tts-2.0";

/// Audio payloads arrive under this event.
pub const EVT_TTS_RESPONSE: u32 = 352;

/// The session's terminal frame.
pub const EVT_SESSION_FINISHED: u32 = 152;

/// The only `status_code` on the terminal frame that means success.
pub const STATUS_SUCCESS: u32 = 20_000_000;

pub const MSG_TYPE_AUDIO: u8 = 0b1011;
pub const MSG_TYPE_JSON: u8 = 0b1001;
pub const MSG_TYPE_ERROR: u8 = 0b1111;

/// The four bytes every frame starts with.
pub const FRAME_HEADER: [u8; 4] = [0x11, 0x10, 0x10, 0x00];

pub const HEADER_API_KEY: &str = "X-Api-Key";
pub const HEADER_RESOURCE_ID: &str = "X-Api-Resource-Id";
pub const HEADER_REQUEST_ID: &str = "X-Api-Request-Id";

const PROVIDER: &str = "volc";

#[cfg(test)]
mod tests {
    use super::super::config::{Endpoints, Secret};
    use super::super::traits::SpeakerId;
    use super::*;

    // -----------------------------------------------------------------------
    // frame parity with tools/vendor-experiments/volc-tts-stream.mjs
    // -----------------------------------------------------------------------

    /// The experiment script's `parse()` walks exactly this layout; the bytes
    /// below are built the same way its server frames were observed.
    fn server_frame(msg_type: u8, event: u32, payload: &[u8]) -> Vec<u8> {
        let sid = b"sid-1";
        let mut frame = vec![0x11, msg_type << 4, 0x10, 0x00];
        frame.extend_from_slice(&event.to_be_bytes());
        frame.extend_from_slice(&(sid.len() as u32).to_be_bytes());
        frame.extend_from_slice(sid);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn error_frame(code: u32, message: &str) -> Vec<u8> {
        let mut frame = vec![0x11, MSG_TYPE_ERROR << 4, 0x10, 0x00];
        frame.extend_from_slice(&code.to_be_bytes());
        frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
        frame.extend_from_slice(message.as_bytes());
        frame
    }

    #[test]
    fn an_audio_frame_parses_to_its_event_and_payload() {
        // Test 1: `msgType = (data[1] >> 4) & 0x0f` picks 0b1011 out of 0xB0.
        let pcm = [0x01u8, 0x02, 0x03, 0x04];
        let frame = server_frame(MSG_TYPE_AUDIO, EVT_TTS_RESPONSE, &pcm);
        assert_eq!(frame[1] >> 4 & 0x0f, MSG_TYPE_AUDIO, "the nibble is the type");
        match parse_frame(&frame).expect("audio frame") {
            ServerFrame::Audio { event, pcm16_le } => {
                assert_eq!(event, EVT_TTS_RESPONSE);
                assert_eq!(pcm16_le, pcm);
            }
            other => panic!("expected audio, got {other:?}"),
        }
    }

    #[test]
    fn a_json_frame_parses_to_its_body() {
        let body = json!({ "status_code": STATUS_SUCCESS, "usage": { "characters": 42 } });
        let frame = server_frame(MSG_TYPE_JSON, EVT_SESSION_FINISHED, body.to_string().as_bytes());
        match parse_frame(&frame).expect("json frame") {
            ServerFrame::Json { event, body } => {
                assert_eq!(event, EVT_SESSION_FINISHED);
                let body = body.expect("a parsed body");
                assert_eq!(body["status_code"], json!(STATUS_SUCCESS));
                assert_eq!(body["usage"]["characters"], json!(42));
            }
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn a_json_frame_with_an_unparseable_body_keeps_its_event() {
        // The script's `catch { json = null }`: the event still reaches the
        // caller, the body does not.
        let frame = server_frame(MSG_TYPE_JSON, 999, b"{not json");
        match parse_frame(&frame).expect("frame") {
            ServerFrame::Json { event, body } => {
                assert_eq!(event, 999);
                assert!(body.is_none(), "the body is dropped, the event is not");
            }
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_json_body_is_not_an_error() {
        let frame = server_frame(MSG_TYPE_JSON, 450, b"");
        match parse_frame(&frame).expect("frame") {
            ServerFrame::Json { body, .. } => assert!(body.is_none()),
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn an_error_frame_carries_its_code_and_message() {
        let frame = error_frame(45_000_001, "invalid speaker");
        match parse_frame(&frame).expect("error frame") {
            ServerFrame::Error { code, message } => {
                assert_eq!(code, 45_000_001);
                assert_eq!(message, "invalid speaker");
            }
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn a_truncated_frame_is_a_retryable_protocol_failure() {
        // The script's `readUInt32BE` would throw; here it must classify.
        let frame = server_frame(MSG_TYPE_AUDIO, EVT_TTS_RESPONSE, &[0u8; 8]);
        for cut in [1usize, 3, 5, frame.len() - 1] {
            let error = parse_frame(&frame[..cut]).expect_err("truncated");
            assert_eq!(error.provider, PROVIDER);
            assert_eq!(error.retry_class, super::super::error::RetryClass::Retryable);
        }
    }

    // -----------------------------------------------------------------------
    // the request
    // -----------------------------------------------------------------------

    #[test]
    fn the_request_body_carries_the_cross_lingual_parameters() {
        // Test 3.
        let body = request_body("app-1", "The query took 800 ms.", "S_9k337yqg2");
        assert_eq!(body["user"]["uid"], "app-1");
        let params = &body["req_params"];
        assert_eq!(params["text"], "The query took 800 ms.");
        assert_eq!(params["speaker"], "S_9k337yqg2");
        assert_eq!(params["audio_params"]["format"], "pcm");
        assert_eq!(params["audio_params"]["sample_rate"], json!(24_000));
        // The two keys the cross-lingual path depends on must be explicit.
        assert_eq!(params["audio_params"]["explicit_language"], "en");
        assert_eq!(params["audio_params"]["tone_fidelity"], json!(false));
    }

    #[test]
    fn the_request_frame_is_the_preamble_a_length_and_the_json() {
        // Test 2.
        let frame = request_frame("app-1", "hello", "S_test");
        assert_eq!(&frame[..4], &FRAME_HEADER);
        let len = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        assert_eq!(len, frame.len() - 8, "the length covers the JSON only");
        let payload: Value = serde_json::from_slice(&frame[8..]).expect("the payload is JSON");
        assert_eq!(payload["req_params"]["text"], "hello");
        assert_eq!(payload["req_params"]["speaker"], "S_test");
    }

    #[test]
    fn the_resource_header_follows_the_voice_kind() {
        // Test 2: Clone → seed-icl-2.0, Preset → seed-tts-2.0.
        let clone = VoiceRef::Clone(SpeakerId::new("S_9k337yqg2"));
        let preset = VoiceRef::Preset("zh_female_vv_uranus_bigtts".to_string());
        assert_eq!(resource_id_for(&clone), RESOURCE_CLONE);
        assert_eq!(resource_id_for(&preset), RESOURCE_PRESET);
        assert_eq!(speaker_of(&clone), "S_9k337yqg2");
        assert_eq!(speaker_of(&preset), "zh_female_vv_uranus_bigtts");
    }

    #[test]
    fn every_handshake_carries_a_fresh_request_id() {
        // Test 2: one new UUID per connection.
        let credentials = VolcCredentials {
            app_id: "app-1".to_string(),
            access_token: Secret::new("test-volc-token"),
            resource_id: RESOURCE_CLONE.to_string(),
            preset_voice: None,
            clone_speaker: None,
        };
        let voice = VoiceRef::Clone(SpeakerId::new("S_test"));
        let first = handshake_headers(&credentials, &voice);
        let second = handshake_headers(&credentials, &voice);

        let header = |headers: &[(&'static str, String)], name: &str| {
            headers
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("{name} is missing"))
        };
        assert_eq!(header(&first, HEADER_API_KEY), "test-volc-token");
        assert_eq!(header(&first, HEADER_RESOURCE_ID), RESOURCE_CLONE);
        let one = header(&first, HEADER_REQUEST_ID);
        let two = header(&second, HEADER_REQUEST_ID);
        assert_ne!(one, two, "a new request id per connection");
        assert_eq!(one.len(), 36, "a UUID: {one}");
        assert_eq!(one.matches('-').count(), 4, "a UUID: {one}");
    }

    #[test]
    fn the_missing_credentials_error_names_the_variables_only() {
        // Test 8.
        let error = VolcCredentials::from_lookup(|_| None).expect_err("no credentials");
        let rendered = error.to_string();
        assert!(rendered.contains("VOLC_TTS_APP_ID"), "{rendered}");
        assert!(rendered.contains("VOLC_TTS_ACCESS_TOKEN"), "{rendered}");
    }

    #[test]
    fn the_client_never_renders_its_access_token() {
        // Test 8: the credential must not survive a Debug or an error string.
        let client = VolcTts::new(
            VolcCredentials {
                app_id: "app-1".to_string(),
                access_token: Secret::new("volc-secret-token-value"),
                resource_id: RESOURCE_CLONE.to_string(),
                preset_voice: None,
                clone_speaker: None,
            },
            Endpoints::defaults(),
        );
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("volc-secret-token-value"), "{rendered}");
        assert!(!client.model_version().contains("volc-secret-token-value"));
    }

    #[test]
    fn a_failed_session_status_is_terminal_failure() {
        // Test 5: `status_code != 20000000` ends the request in failure.
        assert!(session_status(STATUS_SUCCESS).is_ok());
        let error = session_status(45_001_109).expect_err("45_001_109 is not success");
        assert_eq!(error.provider, PROVIDER);
        assert_eq!(error.retry_class, super::super::error::RetryClass::Terminal);
        assert!(error.to_string().contains("45001109"), "{error}");
    }

    #[test]
    fn the_usage_frame_is_read_from_the_finished_body() {
        // Test 6 (D-13).
        let usage = usage_from(&json!({
            "status_code": STATUS_SUCCESS,
            "usage": { "characters": 42, "text_words": 11 }
        }))
        .expect("usage");
        assert_eq!(
            usage,
            TtsUsage {
                characters: 42,
                text_words: 11,
            }
        );
        assert!(usage_from(&json!({ "status_code": STATUS_SUCCESS })).is_none());
    }
}
