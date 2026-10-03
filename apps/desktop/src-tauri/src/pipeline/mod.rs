//! Real cloud pipeline (Phase 2+): the cascaded streaming translation chain
//! and the instruments that gate it.
//!
//! Wave 02-01 lands the latency measurement rig ([`budget`]) — the gate every
//! downstream wave is judged by. Later waves mount their modules here:
//! `stages/` (02-02 provider clients), `cascade.rs` + `breaker.rs` (02-03),
//! `validate.rs` / cost metering (02-03).

pub mod budget;

#[cfg(test)]
mod budget_test;
