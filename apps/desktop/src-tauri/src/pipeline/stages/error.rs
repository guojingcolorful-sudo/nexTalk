//! Vendor error classification — one vocabulary for four vendors (T2.1).
//!
//! D-09 (fragment-level retry) and D-10 (per-vendor circuit breaker) both ask
//! the same question of every failure: *is this worth another attempt?* The
//! answer is [`RetryClass`], and it is decided here, once, instead of being
//! re-invented at each call site:
//!
//! - [`RetryClass::Retryable`] — transient (rate limits, 5xx, sockets, idle
//!   disconnects, protocol hiccups). D-09 retries the fragment, D-10 counts it.
//! - [`RetryClass::Terminal`] — the request will never succeed as-is (session
//!   caps, refused audio, vendor-side permanent errors).
//! - [`RetryClass::Client`] — *our* fault: stale credentials, bad signature,
//!   malformed request. Retrying the same input is pointless; D-10 must **not**
//!   trip the vendor's breaker for one of these.
//!
//! # Privacy (T-02-05)
//!
//! A `StageError` never carries a query string, an `Authorization` header or a
//! key. Vendor URLs embed `authorization=` / `signature=` parameters and the
//! handshake headers carry live credentials, so anything built here goes
//! through [`StageError::with_endpoint`], which takes an already-sanitised
//! `host/path` (see [`sanitize_endpoint`]). The display test in this module is
//! the regression gate for that rule.

use std::fmt;

use thiserror::Error;

/// What the cascade should do with a failure (D-09 retry / D-10 breaker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// Transient: retrying the same fragment may succeed.
    Retryable,
    /// Permanent: this request will never succeed.
    Terminal,
    /// Our bug or a stale credential — do not retry, do not trip the breaker.
    Client,
}

impl RetryClass {
    /// Stable identifier for logs, JSONL traces and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            RetryClass::Retryable => "retryable",
            RetryClass::Terminal => "terminal",
            RetryClass::Client => "client",
        }
    }
}

impl fmt::Display for RetryClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What went wrong, at the resolution the cascade and the breaker need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Configuration is missing or blank (never carries the value).
    Config,
    /// The vendor refused the handshake (401/403, signature or code 10005).
    Auth,
    /// An HTTP status that is not an auth failure.
    Http,
    /// A vendor error frame (`code` in the JSON payload).
    Vendor,
    /// A frame or event did not parse (malformed, truncated, wrong shape).
    Protocol,
    /// The socket died under the session (abrupt close, silent timeout).
    Transport,
    /// The vendor's session cap was reached; the client must rotate.
    SessionCap,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Config => "config",
            ErrorKind::Auth => "auth",
            ErrorKind::Http => "http",
            ErrorKind::Vendor => "vendor",
            ErrorKind::Protocol => "protocol",
            ErrorKind::Transport => "transport",
            ErrorKind::SessionCap => "session_cap",
        }
    }
}

/// A classified failure from one pipeline stage.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{provider}[{retry_class}] {kind}: {message}")]
pub struct StageError {
    /// Vendor/stage that failed: `"xfyun"`, `"deepgram"`, `"deepseek"`, `"volc"`.
    pub provider: &'static str,
    pub kind: ErrorKind,
    pub retry_class: RetryClass,
    /// Human-readable detail. **Never** a URL, a header or a credential value.
    pub message: String,
    /// Sanitised `host/path` for diagnostics, when an endpoint is known.
    pub endpoint: Option<String>,
}

impl StageError {
    pub fn new(
        provider: &'static str,
        kind: ErrorKind,
        retry_class: RetryClass,
        message: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            kind,
            retry_class,
            message: message.into(),
            endpoint: None,
        }
    }

    /// Missing/blank configuration. Lists the variable *names* only.
    pub fn missing_config(names: &[&str]) -> Self {
        Self::new(
            "config",
            ErrorKind::Config,
            RetryClass::Terminal,
            format!("missing environment variables: {}", names.join(", ")),
        )
    }

    /// The vendor refused our credentials or signature.
    pub fn auth(provider: &'static str, message: impl Into<String>) -> Self {
        Self::new(provider, ErrorKind::Auth, RetryClass::Client, message)
    }

    /// An HTTP status from a streaming request, classified by the shared table.
    pub fn http(provider: &'static str, status: u16) -> Self {
        let kind = if matches!(status, 401 | 403) {
            ErrorKind::Auth
        } else {
            ErrorKind::Http
        };
        Self::new(
            provider,
            kind,
            classify_http_status(status),
            format!("HTTP {status}"),
        )
    }

    /// A frame or event that did not parse. Retryable: the next fragment is a
    /// fresh request, and a truncated stream must never look like success.
    pub fn protocol(provider: &'static str, message: impl Into<String>) -> Self {
        Self::new(
            provider,
            ErrorKind::Protocol,
            RetryClass::Retryable,
            message,
        )
    }

    /// The socket died under the session.
    pub fn transport(provider: &'static str, message: impl Into<String>) -> Self {
        Self::new(
            provider,
            ErrorKind::Transport,
            RetryClass::Retryable,
            message,
        )
    }

    /// A vendor error frame carrying its own code.
    pub fn vendor(provider: &'static str, kind: ErrorKind, message: impl Into<String>) -> Self {
        Self::new(provider, kind, RetryClass::Terminal, message)
    }

    /// Attach the sanitised `host/path` (never the query — see module docs).
    pub fn with_endpoint(mut self, host_and_path: &str) -> Self {
        self.endpoint = Some(sanitize_endpoint(host_and_path));
        self
    }
}

/// Strips everything that could carry a credential: the scheme, the userinfo,
/// the query string and the fragment are dropped; only `host[:port]/path`
/// survives.
pub fn sanitize_endpoint(raw: &str) -> String {
    let without_scheme = raw.split_once("://").map_or(raw, |(_, rest)| rest);
    let without_userinfo = without_scheme
        .rsplit_once('@')
        .map_or(without_scheme, |(_, host)| host);
    let (host, path) = without_userinfo
        .split_once('/')
        .map_or((without_userinfo, ""), |(host, path)| (host, path));
    let host = host.split('?').next().unwrap_or(host);
    let path = path.split(['?', '#']).next().unwrap_or(path);
    if path.is_empty() {
        host.to_string()
    } else {
        format!("{host}/{path}")
    }
}

/// HTTP status → retry class. Timeouts and rate limits are transient; a 4xx
/// that is not one of those means the request itself is wrong.
pub fn classify_http_status(status: u16) -> RetryClass {
    match status {
        408 | 425 | 429 => RetryClass::Retryable,
        400..=499 => RetryClass::Client,
        500..=599 => RetryClass::Retryable,
        _ => RetryClass::Terminal,
    }
}

/// 讯飞 `code` → retry class (02-RESEARCH 讯飞错误码表).
///
/// Retryable: the service or the link hiccupped — an idle disconnect (10200),
/// an over-long audio frame (10163), a busy/queueing backend (10303/10500) —
/// the same fragment may simply work next time.
///
/// Terminal: the session or the request is finished for good (10010 session
/// cap, 10165 audio refused, 11200 engine unavailable for the requested
/// parameter set). Client: our signature, permissions or parameters are wrong
/// (10005 wrong API key, 10313 appid mismatch, 10404 concurrency limit).
///
/// Unknown codes default to `Terminal`: an unrecognised vendor error must not
/// be silently retried forever.
pub fn classify_xfyun_code(code: i32) -> RetryClass {
    match code {
        10007 | 10014 | 10114 | 10019 | 10043 | 10044 | 10047 | 10200 | 10222 | 10303 | 10500
        | 10600 | 10700 | 10163 => RetryClass::Retryable,
        10005 | 10010 | 10110 | 10160 | 10161 | 10165 | 11200 | 11201 | 11202 | 11203 => {
            RetryClass::Terminal
        }
        10006 | 10009 | 10109 | 10101 | 10313 | 10317 | 10404 => RetryClass::Client,
        _ => RetryClass::Terminal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xfyun_retryable_codes() {
        for code in [
            10007, 10014, 10114, 10019, 10043, 10044, 10047, 10200, 10222, 10303, 10500, 10600,
            10700, 10163,
        ] {
            assert_eq!(
                classify_xfyun_code(code),
                RetryClass::Retryable,
                "code {code} is transient"
            );
        }
    }

    #[test]
    fn xfyun_terminal_codes() {
        for code in [
            10005, 10010, 10110, 10160, 10161, 10165, 11200, 11201, 11202, 11203,
        ] {
            assert_eq!(
                classify_xfyun_code(code),
                RetryClass::Terminal,
                "code {code} is permanent"
            );
        }
    }

    #[test]
    fn xfyun_client_bug_codes() {
        for code in [10006, 10009, 10109, 10101, 10313, 10317, 10404] {
            assert_eq!(
                classify_xfyun_code(code),
                RetryClass::Client,
                "code {code} is our fault"
            );
        }
        assert_eq!(
            classify_xfyun_code(999_999),
            RetryClass::Terminal,
            "unknown codes are not retried forever"
        );
    }

    #[test]
    fn http_status_classes() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert_eq!(classify_http_status(status), RetryClass::Retryable);
        }
        for status in [400, 401, 403, 404, 413, 422] {
            assert_eq!(classify_http_status(status), RetryClass::Client);
        }
    }

    #[test]
    fn protocol_failures_are_retryable() {
        assert_eq!(classify_http_status(504), RetryClass::Retryable);
        assert_eq!(
            StageError::protocol("deepseek", "truncated SSE event").retry_class,
            RetryClass::Retryable
        );
        assert_eq!(
            StageError::transport("deepgram", "silent timeout").retry_class,
            RetryClass::Retryable
        );
    }

    #[test]
    fn sanitize_endpoint_drops_credentials() {
        assert_eq!(
            sanitize_endpoint("wss://iat-api.xfyun.cn/v2/iat?authorization=abc&date=now"),
            "iat-api.xfyun.cn/v2/iat"
        );
        assert_eq!(
            sanitize_endpoint("https://api.deepseek.com"),
            "api.deepseek.com"
        );
        assert_eq!(
            sanitize_endpoint("https://user:pass@example.com/a/b?c=d#e"),
            "example.com/a/b"
        );
    }

    #[test]
    fn display_never_leaks_a_signed_url_or_a_key() {
        // A signed 讯飞 URL is the realistic leak: the handshake query carries
        // `authorization=`, `signature=` and the API key in one string.
        let error = StageError::auth(
            "xfyun",
            "handshake rejected (HTTP 403); check the local clock",
        )
        .with_endpoint("wss://iat-api.xfyun.cn/v2/iat?authorization=aGk&signature=abc123");

        let rendered = error.to_string();
        assert!(rendered.contains("xfyun"), "{rendered}");
        assert!(!rendered.contains("authorization="), "{rendered}");
        assert!(!rendered.contains("signature="), "{rendered}");
        assert!(!rendered.contains("aGk"), "{rendered}");

        let debugged = format!("{error:?}");
        assert!(
            debugged.contains("iat-api.xfyun.cn/v2/iat"),
            "the sanitised endpoint survives for diagnostics: {debugged}"
        );
        assert!(!debugged.contains("authorization="), "{debugged}");
        assert!(!debugged.contains("signature="), "{debugged}");
        assert!(!debugged.contains("aGk"), "{debugged}");
    }

    #[test]
    fn missing_config_lists_names_only() {
        let error = StageError::missing_config(&["DEEPSEEK_API_KEY", "VOLC_TTS_ACCESS_TOKEN"]);
        let rendered = error.to_string();
        assert!(rendered.contains("DEEPSEEK_API_KEY"));
        assert!(rendered.contains("VOLC_TTS_ACCESS_TOKEN"));
        assert_eq!(error.retry_class, RetryClass::Terminal);
    }
}
