//! Deterministic simulation source (01-02 walking skeleton, completed in
//! 01-05): the four-round script data, a pure evaluator plus the stateful
//! engine that plays it into SessionState.

pub mod script;
pub mod source;

#[cfg(test)]
mod source_test;
