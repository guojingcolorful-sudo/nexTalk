//! Fragment retry policy and the per-vendor circuit breaker (02-03 T3.4).
//!
//! Two decisions live here, both taken from the cascade's single failure exit:
//!
//! - **D-09 — retry one fragment, twice, briefly.** A retryable failure waits
//!   [`RETRY_BACKOFF_MS`] before the next call and stops as soon as the
//!   [`RETRY_BUDGET_MS`] budget would be exceeded. The scope is exactly one
//!   fragment (GOV-12): schedules never accumulate across sentences, and both
//!   values are injected (`RetryConfig::wait`) so tests assert the schedule
//!   from the ledger instead of sleeping through it.
//! - **D-10 — the vendor gets two strikes.** Two consecutive failed fragments
//!   open the provider's breaker for [`BREAKER_OPEN_MS`]; while it is open the
//!   cascade refuses the call outright (`circuit_open`, zero attempts) instead
//!   of paying the full retry cost on every sentence. When the window elapses
//!   exactly one probe is admitted ([`BreakerState::HalfOpen`]); its success
//!   closes the circuit, its failure starts a fresh window.
//!
//! **Client errors never strike.** A 401 is our credential problem, not the
//! vendor's health ([`crate::pipeline::stages::RetryClass::Client`]): it is not
//! retried and it does not move the breaker. Configuration errors are our bug
//! for the same reason (`decide` returns `Terminal`, the cascade skips the
//! strike).
//!
//! Time is a parameter everywhere (`now_ms`), never read from the wall clock:
//! the two-minute window has to be testable without waiting two minutes.

use std::collections::HashMap;

use crate::pipeline::stages::RetryClass;

/// Retries allowed after the first dispatch (D-09) — total calls ≤ 1 + this.
pub const MAX_RETRY_ATTEMPTS: u32 = 2;
/// The D-09 backoff schedule: one delay per retry, in order.
pub const RETRY_BACKOFF_MS: [u64; 2] = [100, 200];
/// The per-fragment wait budget (D-09): a wait that would cross it is refused.
pub const RETRY_BUDGET_MS: u64 = 500;
/// Consecutive failed fragments that open a provider's circuit (D-10).
pub const BREAKER_THRESHOLD: u32 = 2;
/// How long a circuit stays open before one probe is admitted (D-10).
pub const BREAKER_OPEN_MS: u64 = 120_000;
/// GOV-12: retries are serialised — one fragment in flight, ever.
pub const MAX_RETRY_FRAGMENT_CONCURRENCY: usize = 1;

/// How an exhausted fragment's failure is accounted (D-19 error codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveUpReason {
    /// [`MAX_RETRY_ATTEMPTS`] retries were spent.
    AttemptsExhausted,
    /// The next backoff would cross [`RETRY_BUDGET_MS`].
    BudgetExhausted,
    /// The classification says this request can never succeed; a wait would
    /// only delay the degraded display.
    NotRetryable(RetryClass),
}

impl GiveUpReason {
    /// The aggregatable code the trace and the UI carry (D-19) — never a
    /// vendor message.
    pub fn error_code(self) -> &'static str {
        match self {
            GiveUpReason::AttemptsExhausted => "retry_exhausted",
            GiveUpReason::BudgetExhausted => "retry_budget_exhausted",
            GiveUpReason::NotRetryable(RetryClass::Client) => "client_error",
            GiveUpReason::NotRetryable(RetryClass::Terminal) => "vendor_error",
            // `decide` never gives up on a Retryable classification; mapping it
            // to the retry code keeps the enum total without a panic path.
            GiveUpReason::NotRetryable(RetryClass::Retryable) => "retry_exhausted",
        }
    }
}

/// What to do after one failed translator call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Wait `delay_ms`, then call again.
    Retry { delay_ms: u64 },
    /// Stop; the fragment degrades.
    GiveUp(GiveUpReason),
}

/// The D-09 retry policy, one fragment's worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Retries allowed after the first dispatch (0 = never retry).
    pub max_attempts: u32,
    /// The backoff schedule, one delay per retry, in order.
    pub backoff_ms: [u64; 2],
    /// The per-fragment wait budget.
    pub budget_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_RETRY_ATTEMPTS,
            backoff_ms: RETRY_BACKOFF_MS,
            budget_ms: RETRY_BUDGET_MS,
        }
    }
}

impl RetryPolicy {
    /// The whole retry decision, pure: no clock, no state, no I/O.
    ///
    /// `retries_done` counts the retries already performed (calls - 1) and
    /// `waited_ms` the delays already spent on this fragment.
    pub fn decide(self, class: RetryClass, retries_done: u32, waited_ms: u64) -> RetryDecision {
        if class != RetryClass::Retryable {
            return RetryDecision::GiveUp(GiveUpReason::NotRetryable(class));
        }
        if retries_done >= self.max_attempts || retries_done as usize >= self.backoff_ms.len() {
            return RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted);
        }
        let delay_ms = self.backoff_ms[retries_done as usize];
        if waited_ms + delay_ms > self.budget_ms {
            return RetryDecision::GiveUp(GiveUpReason::BudgetExhausted);
        }
        RetryDecision::Retry { delay_ms }
    }
}

/// How a decided wait is honoured. Production keeps [`WaitMode::Real`]; every
/// test runs [`WaitMode::Immediate`] and asserts the schedule from the ledger,
/// so no test ever sleeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitMode {
    Real,
    Immediate,
}

/// The retry knobs carried by [`CascadeConfig`](crate::pipeline::cascade::CascadeConfig).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    pub policy: RetryPolicy,
    pub wait: WaitMode,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            policy: RetryPolicy::default(),
            wait: WaitMode::Real,
        }
    }
}

/// One recorded wait — the witness of D-09's schedule, stamped by the
/// cascade's injected clock (never wall time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryWait {
    pub segment_id: u64,
    /// 1-based retry index within the fragment.
    pub attempt: u32,
    pub delay_ms: u64,
    /// The injected clock when the wait was decided.
    pub at_ms: u64,
}

/// The observable condition of one vendor's circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal operation.
    Closed,
    /// Refusing requests until `until_ms`.
    Open { until_ms: u64 },
    /// The open window elapsed; exactly one probe may be in flight.
    HalfOpen,
}

/// One provider's circuit breaker (D-10).
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    threshold: u32,
    open_ms: u64,
    state: BreakerState,
    failures: u32,
    probe_in_flight: bool,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(BREAKER_THRESHOLD, BREAKER_OPEN_MS)
    }
}

impl CircuitBreaker {
    pub fn new(threshold: u32, open_ms: u64) -> Self {
        Self {
            threshold,
            open_ms,
            state: BreakerState::Closed,
            failures: 0,
            probe_in_flight: false,
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state
    }

    /// May a request go out right now?
    ///
    /// The [`BreakerState::Open`] → [`BreakerState::HalfOpen`] transition
    /// admits the single probe — a second concurrent request is refused, so a
    /// recovered vendor is re-trusted exactly once.
    pub fn allow_request(&mut self, now_ms: u64) -> bool {
        match self.state {
            BreakerState::Closed => true,
            BreakerState::Open { until_ms } => {
                if now_ms >= until_ms {
                    self.state = BreakerState::HalfOpen;
                    self.probe_in_flight = true;
                    true
                } else {
                    false
                }
            }
            BreakerState::HalfOpen => {
                if self.probe_in_flight {
                    false
                } else {
                    self.probe_in_flight = true;
                    true
                }
            }
        }
    }

    /// A call came back: close the circuit and forget the failures (this is
    /// also the half-open probe's success path).
    pub fn record_success(&mut self) {
        self.state = BreakerState::Closed;
        self.failures = 0;
        self.probe_in_flight = false;
    }

    /// A fragment was given up on (never called for client/config errors).
    pub fn record_failure(&mut self, now_ms: u64) {
        match self.state {
            BreakerState::HalfOpen => {
                // The probe failed: a fresh window, not an immediate re-probe.
                self.state = BreakerState::Open {
                    until_ms: now_ms + self.open_ms,
                };
                self.probe_in_flight = false;
            }
            _ => {
                self.failures += 1;
                if self.failures >= self.threshold {
                    self.state = BreakerState::Open {
                        until_ms: now_ms + self.open_ms,
                    };
                }
            }
        }
    }
}

/// The cascade's breaker set: one circuit per provider (`"deepseek"`,
/// `"volc"`, …), so one sick vendor never silences another stage.
#[derive(Debug, Clone, Default)]
pub struct BreakerSet {
    breakers: HashMap<&'static str, CircuitBreaker>,
}

impl BreakerSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_request(&mut self, provider: &'static str, now_ms: u64) -> bool {
        self.breakers
            .entry(provider)
            .or_default()
            .allow_request(now_ms)
    }

    pub fn record_success(&mut self, provider: &'static str) {
        self.breakers.entry(provider).or_default().record_success();
    }

    pub fn record_failure(&mut self, provider: &'static str, now_ms: u64) {
        self.breakers
            .entry(provider)
            .or_default()
            .record_failure(now_ms);
    }

    /// An unknown provider (never used, never failed) is Closed.
    pub fn state_of(&self, provider: &'static str) -> BreakerState {
        self.breakers
            .get(provider)
            .map(CircuitBreaker::state)
            .unwrap_or(BreakerState::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------- policy ---

    /// The D-09 schedule verbatim: 100, then 200, then out — inside 500 ms.
    #[test]
    fn breaker_policy_follows_the_documented_schedule() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 2);
        assert_eq!(policy.backoff_ms, [100, 200]);
        assert_eq!(policy.budget_ms, 500);

        assert_eq!(
            policy.decide(RetryClass::Retryable, 0, 0),
            RetryDecision::Retry { delay_ms: 100 }
        );
        assert_eq!(
            policy.decide(RetryClass::Retryable, 1, 100),
            RetryDecision::Retry { delay_ms: 200 }
        );
        assert_eq!(
            policy.decide(RetryClass::Retryable, 2, 300),
            RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted)
        );
    }

    #[test]
    fn breaker_policy_refuses_a_wait_that_would_cross_the_budget() {
        let policy = RetryPolicy {
            budget_ms: 250,
            ..RetryPolicy::default()
        };
        assert_eq!(
            policy.decide(RetryClass::Retryable, 1, 100),
            RetryDecision::GiveUp(GiveUpReason::BudgetExhausted)
        );
    }

    /// D-06: the classification decides before any wait is considered.
    #[test]
    fn breaker_policy_never_retries_client_or_terminal_errors() {
        let policy = RetryPolicy::default();
        assert_eq!(
            policy.decide(RetryClass::Client, 0, 0),
            RetryDecision::GiveUp(GiveUpReason::NotRetryable(RetryClass::Client))
        );
        assert_eq!(
            GiveUpReason::NotRetryable(RetryClass::Client).error_code(),
            "client_error"
        );
        assert_eq!(
            policy.decide(RetryClass::Terminal, 0, 0),
            RetryDecision::GiveUp(GiveUpReason::NotRetryable(RetryClass::Terminal))
        );
        assert_eq!(
            GiveUpReason::NotRetryable(RetryClass::Terminal).error_code(),
            "vendor_error"
        );
    }

    #[test]
    fn breaker_error_codes_are_the_aggregatable_set() {
        assert_eq!(
            GiveUpReason::AttemptsExhausted.error_code(),
            "retry_exhausted"
        );
        assert_eq!(
            GiveUpReason::BudgetExhausted.error_code(),
            "retry_budget_exhausted"
        );
    }

    // ------------------------------------------------------------ breaker ---

    #[test]
    fn breaker_opens_at_the_threshold_and_is_time_bounded() {
        let mut breaker = CircuitBreaker::default();
        assert_eq!(breaker.state(), BreakerState::Closed);
        assert!(breaker.allow_request(0));

        breaker.record_failure(0);
        assert_eq!(
            breaker.state(),
            BreakerState::Closed,
            "one strike is not two"
        );
        assert!(breaker.allow_request(10));

        breaker.record_failure(20);
        assert_eq!(
            breaker.state(),
            BreakerState::Open {
                until_ms: 20 + 120_000
            },
            "the window starts at the strike"
        );
        assert!(
            !breaker.allow_request(20 + 119_999),
            "still open a millisecond before the window ends"
        );
        assert_eq!(breaker.state(), BreakerState::Open { until_ms: 120_020 });
    }

    #[test]
    fn breaker_half_open_admits_exactly_one_probe() {
        let mut breaker = CircuitBreaker::default();
        breaker.record_failure(0);
        breaker.record_failure(0);
        assert!(!breaker.allow_request(0));

        // The window elapses: the first request becomes the probe…
        assert!(breaker.allow_request(BREAKER_OPEN_MS));
        assert_eq!(breaker.state(), BreakerState::HalfOpen);
        // …and a concurrent one is refused rather than probing in parallel.
        assert!(!breaker.allow_request(BREAKER_OPEN_MS));

        // Probe success closes the circuit and resets the strikes: the vendor
        // gets its full two strikes the next time it misbehaves.
        breaker.record_success();
        assert_eq!(breaker.state(), BreakerState::Closed);
        assert!(breaker.allow_request(BREAKER_OPEN_MS));
        breaker.record_failure(BREAKER_OPEN_MS);
        assert_eq!(breaker.state(), BreakerState::Closed);
    }

    #[test]
    fn breaker_probe_failure_starts_a_fresh_window() {
        let mut breaker = CircuitBreaker::default();
        breaker.record_failure(0);
        breaker.record_failure(0);
        assert!(breaker.allow_request(120_000));

        breaker.record_failure(120_100);
        assert_eq!(
            breaker.state(),
            BreakerState::Open {
                until_ms: 120_100 + BREAKER_OPEN_MS
            },
            "a failed probe is a fresh open window, not an instant re-probe"
        );
        assert!(!breaker.allow_request(120_200));
    }

    /// One sick vendor must not silence another: the set keys by provider.
    #[test]
    fn breaker_set_isolates_providers() {
        let mut set = BreakerSet::new();
        assert_eq!(set.state_of("deepseek"), BreakerState::Closed);
        assert_eq!(set.state_of("volc"), BreakerState::Closed);

        set.record_failure("deepseek", 0);
        set.record_failure("deepseek", 0);
        assert!(matches!(
            set.state_of("deepseek"),
            BreakerState::Open { .. }
        ));
        assert_eq!(
            set.state_of("volc"),
            BreakerState::Closed,
            "volc is healthy"
        );
        assert!(set.allow_request("volc", 0));

        set.record_success("deepseek");
        assert_eq!(set.state_of("deepseek"), BreakerState::Closed);
    }
}
