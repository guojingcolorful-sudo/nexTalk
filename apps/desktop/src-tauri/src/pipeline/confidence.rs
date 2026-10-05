//! Confidence provenance (02-03 T3.6) — data-only by decision.
//!
//! GOV-01/02 (2026-09-30 revision): the translation chain does **no**
//! confidence marking. The three-factor score (意图/证据/引用) belongs to the
//! Phase 5 strategy cards, and Phase 2's subtitles carry no confidence badge.
//! What Phase 2 keeps is *provenance*: whether a `confidence` value came from
//! a vendor or from a local proxy — plus the third state, `ProxyUnavailable`,
//! for 讯飞 whose `sc` field is reserved and always 0 (research correction 1).
//!
//! The value travels into the JSONL trace (`confidence`, `confidenceSource`,
//! GOV-06) and drives nothing downstream here: no gate, no abstain (D-03), no
//! UI. The distinction is a type, not a convention, so a proxy value can never
//! be averaged with — or misreported as — a vendor one (T-02-10). On the wire,
//! `ProxyUnavailable` is the *absence* of the field (`lan::server`), never a
//! fabricated number.

/// Where a [`SttPartial`](crate::pipeline::stages::SttPartial) confidence value
/// came from (research correction 3).
///
/// 讯飞 has no per-word score — its `sc` field is reserved and always 0 — so a
/// proxy value must be distinguishable from a real vendor score instead of
/// being averaged into one number (D-02 / GOV-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceSource {
    /// The vendor returned a score we can use.
    Vendor,
    /// A local proxy produced the value (Phase 5 fills it; Phase 2 讯飞 stays
    /// `ProxyUnavailable`).
    Proxy,
    /// No vendor score and no proxy yet — the Phase 2 讯飞 state.
    ProxyUnavailable,
}

impl ConfidenceSource {
    /// Wire vocabulary shared with `@nextalk/protocol` (T2.7).
    pub fn as_str(self) -> &'static str {
        match self {
            ConfidenceSource::Vendor => "vendor",
            ConfidenceSource::Proxy => "proxy",
            ConfidenceSource::ProxyUnavailable => "proxy_unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_sources_use_the_wire_vocabulary() {
        assert_eq!(ConfidenceSource::Vendor.as_str(), "vendor");
        assert_eq!(ConfidenceSource::Proxy.as_str(), "proxy");
        assert_eq!(
            ConfidenceSource::ProxyUnavailable.as_str(),
            "proxy_unavailable"
        );
    }

    /// T-02-10: the provenance values never collapse into each other — a proxy
    /// estimate can never masquerade as a vendor score.
    #[test]
    fn provenance_values_stay_distinguishable() {
        let all = [
            ConfidenceSource::Vendor,
            ConfidenceSource::Proxy,
            ConfidenceSource::ProxyUnavailable,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(a.as_str(), b.as_str());
                }
            }
        }
    }
}
