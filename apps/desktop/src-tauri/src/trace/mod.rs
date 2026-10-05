//! Trace surface (02-03 T3.4/T3.5): the durable record of what each segment
//! did and why.
//!
//! The live path never reads these files: the cascade emits the debuggable
//! facts (status, aggregatable error code) into [`jsonl`] records, the session
//! later persists them (T3.7), and the diagnostics page reports on them.
//! Nothing here may carry a vendor message blob — D-19 exists so a trace line
//! remains aggregatable long after the incident.

pub mod jsonl;
