//! Voice enrollment (02-04).
//!
//! The user-facing path is "read a paragraph for 1–3 minutes so we can clone
//! your voice". Modules:
//!
//! - [`capture`] (T4.1) — real microphone capture, guard checks (duration,
//!   size, silence, speech), the startup trim, and the 16 kHz mono WAV export
//!   under `<app data>/enroll/`.
//! - [`register`] (T4.2) — the 火山 voice_clone training call.
//! - [`voice_store`] (T4.2/T4.3) — the profile on disk and the
//!   clone-or-preset resolution the cascade reads per segment.
//!
//! Privacy (T-02-16): the sample and the speaker id live only in the app data
//! directory, are never logged and never enter the session JSONL. Training is
//! the only outbound path and it points at exactly one host.

pub mod capture;
pub mod register;
pub mod voice_store;
