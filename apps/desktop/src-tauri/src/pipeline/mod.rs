//! Real cloud pipeline (Phase 2+): the cascaded streaming translation chain
//! and the instruments that gate it.
//!
//! Wave 02-01 lands the latency measurement rig ([`budget`]) — the gate every
//! downstream wave is judged by; 02-02 adds the vendor stage clients
//! ([`stages`]); 02-03 ([`cascade`]) assembles them into the running chain,
//! with [`segment`] + [`vad`] deciding where a sentence ends and [`cascade`]
//! enforcing the commit gate (GOV-15). Later in the wave: `breaker.rs`
//! (retry/circuit breaker), `confidence.rs` + `validate.rs` (trace data and
//! the deterministic output check), `trace/` (JSONL provenance + metering).

pub mod breaker;
pub mod budget;
pub mod cascade;
pub mod confidence;
pub mod segment;
pub mod stages;
pub mod vad;
pub mod validate;

#[cfg(test)]
mod budget_test;
