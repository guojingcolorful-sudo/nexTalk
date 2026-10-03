//! 讯飞 iat — the user's Chinese line (T2.2).
//!
//! Ported from `tools/vendor-experiments/stt-ab.mjs`, which is the behavioural
//! reference for every constant in this file.
//!
//! # What the protocol actually does
//!
//! A session is one signed WebSocket (`wss://…/v2/iat?authorization=…&date=…`),
//! opened per fragment. Every frame the client sends carries a `data.status`:
//! `0` opens, `1` continues, `2` closes. Every frame the service sends carries
//! its own `data.status`, and only `2` means *"this transcript is done"* — that
//! is the commit gate (D-16): 0 and 1 are interim, no matter how confident the
//! text looks.
//!
//! With `dwa: "wpgs"` enabled the service also *revises* what it already said:
//! `pgs: "apd"` appends, `pgs: "rpl"` replaces the frames whose `sn` lie in
//! `rg: [from, to]` (1-based, inclusive) — and later frames' `sn` shift
//! accordingly. Rebuilding the sentence by string concatenation therefore
//! produces text that never existed; [`TranscriptBuilder`] keeps an ordered
//! `Vec<(sn, text)>` instead.
//!
//! # Deliberate omissions
//!
//! - The `sc` field is **never** read as a confidence score: it is a reserved
//!   field that stays 0. `confidence` is `None` with
//!   `ConfidenceSource::ProxyUnavailable` until 02-03's local proxy lands
//!   (research correction 3).
//! - `eos` is configured but not driven locally: silence detection is 02-03's
//!   job (T3.2). This file only guarantees the *semantics* are right.
//! - Nothing here logs a full URL or a header. The signed URL embeds the API
//!   key; only `sanitize_endpoint(url)` (host + path) may reach a log or an
//!   error.

use crate::pipeline::stages::error::StageError;

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// 40 ms of 16 kHz / 16-bit mono PCM — one upstream frame.
pub const AUDIO_FRAME_BYTES: usize = 1_280;

/// The service rejects an oversize `data.audio` with code 10163; base64 grows
/// 4/3, so the *encoded* frame must stay under this many characters.
pub const MAX_BASE64_CHARS: usize = 13_000;

/// The service caps a session at 60 s and then just stops answering.
pub const DEFAULT_SESSION_CAP_MS: u64 = 60_000;

/// `eos`: how long a silence may last before the service finalises. Passed as
/// a hint; the cadence is enforced by the service.
pub const DEFAULT_EOS_MS: u32 = 2_000;

/// 12_000 Hz… the iat endpoint only accepts 16 kHz L16 mono.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::stages::traits::ConfidenceSource;
    use base64::Engine;

    const TEST_SECRET: &str = "test-secret-value";
    const TEST_API_KEY: &str = "test-api-key";
    const TEST_HOST: &str = "iat-api.xfyun.cn";
    const TEST_DATE: &str = "Thu, 01 Jan 2026 00:00:00 GMT";

    /// Precomputed with the reference script (`stt-ab.mjs`, node 21):
    ///
    /// ```text
    /// origin    = "host: iat-api.xfyun.cn\ndate: Thu, 01 Jan 2026 00:00:00 GMT\nGET /v2/iat HTTP/1.1"
    /// signature = base64(HMAC-SHA256("test-secret-value", origin))
    /// ```
    const EXPECTED_SIGNATURE: &str = "aI+0YqF5gES5iuTV7rujfAVT4Gg4+Dqn7fnsMJkmVpw=";
    const EXPECTED_AUTHORIZATION: &str = "YXBpX2tleT0idGVzdC1hcGkta2V5IiwgYWxnb3JpdGhtPSJobWFjLXNoYTI1NiIsIGhlYWRlcnM9Imhvc3QgZGF0ZSByZXF1ZXN0LWxpbmUiLCJzaWduYXR1cmU9ImFJKzBZcUY1Z0VTNWl1VFY3cnVqZkFWVDRHZzQrRHFuN2Zuc01Ka21WcHc9Ig==";

    #[test]
    fn signature_matches_the_precomputed_reference_bytes() {
        // Test 1: the signing string's order is fixed — this is the regression
        // gate against a silent drift in linkage testing.
        assert_eq!(
            signature_string(TEST_HOST, "/v2/iat", TEST_DATE),
            "host: iat-api.xfyun.cn\ndate: Thu, 01 Jan 2026 00:00:00 GMT\nGET /v2/iat HTTP/1.1"
        );
        assert_eq!(
            sign(TEST_SECRET, TEST_HOST, "/v2/iat", TEST_DATE),
            EXPECTED_SIGNATURE
        );
        assert_eq!(
            authorization_header(TEST_API_KEY, EXPECTED_SIGNATURE),
            EXPECTED_AUTHORIZATION
        );
    }

    #[test]
    fn the_signed_url_carries_every_parameter_the_service_checks() {
        let url = signed_url(
            "wss://iat-api.xfyun.cn/v2/iat",
            TEST_API_KEY,
            TEST_SECRET,
            "appid-test",
            TEST_DATE,
        )
        .expect("the default endpoint parses");

        assert!(url.starts_with("wss://iat-api.xfyun.cn/v2/iat?authorization="));
        assert!(url.contains(&format!("&host={TEST_HOST}")));
        assert!(url.contains("&appid=appid-test"));
        assert!(url.contains("date=Thu%2C+01+Jan+2026+00%3A00%3A00+GMT"));
    }

    #[test]
    fn the_signed_url_uses_the_authority_and_path_of_a_loopback_mock() {
        let url = signed_url(
            "ws://127.0.0.1:41234/v2/iat",
            TEST_API_KEY,
            TEST_SECRET,
            "appid-test",
            TEST_DATE,
        )
        .expect("parses");
        // The `host` parameter must match what the socket actually dials.
        assert!(url.contains("host=127.0.0.1%3A41234"), "{url}");
    }

    #[test]
    fn an_unparseable_endpoint_is_a_configuration_error() {
        let error = signed_url("nonsense", TEST_API_KEY, TEST_SECRET, "app", TEST_DATE)
            .expect_err("no scheme, no host");
        assert_eq!(
            error.retry_class,
            crate::pipeline::stages::error::RetryClass::Terminal
        );
        assert_eq!(error.kind, crate::pipeline::stages::error::ErrorKind::Config);
    }

    #[test]
    fn wpgs_append_builds_the_sentence_in_order() {
        // Test 3
        let mut builder = TranscriptBuilder::new();
        assert!(!builder.apply(WpgsFrame::append(1, "你能")));
        assert!(!builder.apply(WpgsFrame::append(2, "详细")));
        assert_eq!(builder.text(), "你能详细");
    }

    #[test]
    fn wpgs_replace_rewrites_the_covered_range() {
        // Test 4: `rg:[1,2]` is 1-based and inclusive.
        let mut builder = TranscriptBuilder::new();
        builder.apply(WpgsFrame::append(1, "你能"));
        builder.apply(WpgsFrame::append(2, "详细"));
        assert!(
            builder.apply(WpgsFrame::replace(3, 1, 2, "你能详述")),
            "a replace is reported as a revision"
        );
        assert_eq!(builder.text(), "你能详述");
    }

    #[test]
    fn a_partial_replace_keeps_the_frames_outside_the_range() {
        let mut builder = TranscriptBuilder::new();
        builder.apply(WpgsFrame::append(1, "你能"));
        builder.apply(WpgsFrame::append(2, "详细"));
        builder.apply(WpgsFrame::append(3, "说一下"));
        assert!(builder.apply(WpgsFrame::replace(4, 1, 1, "您能")));
        assert_eq!(builder.text(), "您能详细说一下");
    }

    #[test]
    fn a_replace_of_an_unknown_range_appends_instead_of_dropping_text() {
        let mut builder = TranscriptBuilder::new();
        builder.apply(WpgsFrame::append(1, "你能"));
        assert!(builder.apply(WpgsFrame::replace(2, 7, 9, "了吗")));
        assert_eq!(builder.text(), "你能了吗", "no text is ever lost");
    }

    #[test]
    fn only_status_two_commits() {
        // Test 5 — 02-03's GOV-15 depends on this contract.
        for status in [0, 1] {
            let partial = partial_from_frame(status, "你能", false, "iat");
            assert!(!partial.is_final, "status {status} is interim");
            assert!(!partial.committed, "status {status} must not be spoken");
        }
        let final_frame = partial_from_frame(2, "你能详细说说优化步骤吗？", true, "iat");
        assert!(final_frame.is_final);
        assert!(final_frame.committed);
        assert_eq!(final_frame.text, "你能详细说说优化步骤吗？");
    }

    #[test]
    fn confidence_is_never_invented_from_the_reserved_sc_field() {
        let partial = partial_from_frame(1, "你能", false, "iat");
        assert_eq!(partial.confidence, None);
        assert_eq!(
            partial.confidence_source,
            ConfidenceSource::ProxyUnavailable,
            "讯飞 has no score until 02-03's proxy"
        );
        assert_eq!(partial.provider, "xfyun");
        assert_eq!(partial.model_version, "iat");
    }

    #[test]
    fn audio_is_split_below_the_10163_threshold() {
        // Test 6: 9751 bytes would encode to 13004 chars — over the limit.
        let oversized = vec![7u8; 9_751];
        let frames = audio_frames(&oversized);
        assert_eq!(
            frames.iter().map(Vec::len).sum::<usize>(),
            9_751,
            "no audio is dropped"
        );
        for frame in &frames {
            assert!(frame.len() <= AUDIO_FRAME_BYTES, "one frame per 40 ms");
            let encoded = base64::engine::general_purpose::STANDARD.encode(frame);
            assert!(
                encoded.len() < MAX_BASE64_CHARS,
                "encoded frame is {} chars",
                encoded.len()
            );
        }
        assert_eq!(frames.iter().map(Vec::len).collect::<Vec<_>>(), [1_280, 1_280, 1_280, 1_280, 1_280, 1_280, 1_280, 791]);
    }

    #[test]
    fn an_empty_audio_push_produces_no_frame() {
        assert!(audio_frames(&[]).is_empty());
    }

    #[test]
    fn handshake_failures_never_render_a_signed_url_or_a_key() {
        // Test 9
        let error = handshake_error(
            403,
            "wss://iat-api.xfyun.cn/v2/iat?authorization=secret-abc&date=now&appid=x",
        );
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains("authorization="), "{rendered}");
            assert!(!rendered.contains("secret-abc"), "{rendered}");
            assert!(!rendered.contains("appid=x"), "{rendered}");
            assert!(
                rendered.contains("iat-api.xfyun.cn/v2/iat"),
                "the host survives for diagnostics: {rendered}"
            );
        }
        assert!(error.to_string().contains("clock"), "{}", error);
        assert_eq!(
            error.retry_class,
            crate::pipeline::stages::error::RetryClass::Client,
            "a stale clock is our problem, not a transient vendor fault"
        );
    }
}
