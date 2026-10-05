//! Stage contracts — the vendor-agnostic shape of the cascade (T2.1).
//!
//! The chain is three streaming stages (Chinese STT → incremental translation
//! → cloned-voice TTS) plus a second, **output-only** STT stage for the
//! interviewer's English line. Each stage sits behind one trait here, so
//! swapping a vendor is a new impl and not a new cascade. Runtime polymorphism
//! is enum dispatch ([`super::VendorStt`] & friends): async fns in traits
//! cannot be turned into `dyn` objects, and `async-trait` would box every hop
//! inside a 2 s budget.
//!
//! # Invariants this module encodes
//!
//! 1. **Only committed text may be spoken** (D-16). [`SttPartial::is_final`]
//!    says the *vendor* considered the frame final; [`SttPartial::committed`]
//!    says *we* may act on it. They differ: 讯飞 only commits on `status == 2`
//!    (its `wpgs` revisions stream past in the meantime), Deepgram commits when
//!    a `speech_final` run is flushed. The cascade drops non-committed partials
//!    instead of forwarding them.
//! 2. **The interviewer line never reaches TTS.** [`InterviewerTrack`] marks
//!    the Deepgram stage as subtitle-only (T2.3); the cascade only feeds user
//!    fragments to the TTS sink.
//! 3. **Every stage reports its own latency boundary** (02-01 rig contract):
//!    each implementation calls [`MarkHandle::mark`] **exactly once per
//!    segment**, at its own first streamed byte. The `cold` flag and the
//!    deadline math stay in `budget.rs` — a stage never guesses them.

use std::fmt;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::pipeline::budget::Stage;

use super::error::StageError;

/// Bounded audio queue: 64 chunks ≈ 2.5 s of 40 ms frames. A full queue means
/// the vendor link is slower than the microphone — backpressure, not growth.
pub const AUDIO_QUEUE_FRAMES: usize = 64;

/// Bounded event queue from a vendor driver task to the cascade.
pub const EVENT_QUEUE_ITEMS: usize = 64;

// ---------------------------------------------------------------------------
// latency marks (02-01 rig contract)
// ---------------------------------------------------------------------------

type MarkFn = Arc<dyn Fn(Stage) + Send + Sync>;

/// Cheap cloneable handle a stage fires its latency boundary through.
///
/// `budget.rs` owns the recorder — it lives per segment and carries the `cold`
/// flag — so a stage holds only this closure: the session routes a mark to the
/// *current* segment's recorder inside it, and the stage stays segment-agnostic.
#[derive(Clone, Default)]
pub struct MarkHandle(Option<MarkFn>);

impl MarkHandle {
    pub fn new(callback: impl Fn(Stage) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(callback)))
    }

    /// No-op handle: the default in unit tests and in wiring without a rig.
    pub fn disabled() -> Self {
        Self(None)
    }

    /// Fire one boundary. **Call sites must guarantee once per segment.**
    pub fn mark(&self, stage: Stage) {
        if let Some(callback) = &self.0 {
            callback(stage);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }
}

impl fmt::Debug for MarkHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MarkHandle")
            .field(&self.0.is_some())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// shared vocabulary
// ---------------------------------------------------------------------------

/// Where a [`SttPartial::confidence`] value came from (research correction 3).
///
/// 讯飞 has no per-word score — its `sc` field is reserved and always 0 — so a
/// `None`/proxy value must be distinguishable from a real vendor score instead
/// of being averaged into one number (D-02 / GOV-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceSource {
    /// The vendor returned a score we can use.
    Vendor,
    /// A local proxy produced the value (02-03 T3.6).
    Proxy,
    /// No vendor score and no proxy yet — the Phase 2 讯飞 state.
    ProxyUnavailable,
}

impl ConfidenceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ConfidenceSource::Vendor => "vendor",
            ConfidenceSource::Proxy => "proxy",
            ConfidenceSource::ProxyUnavailable => "proxy_unavailable",
        }
    }
}

/// Why the translator declined to answer (D-03: abstain only on silent audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstainReason {
    /// The audio held no speech.
    SilentAudio,
    /// Speech was present but nothing recognisable came out of it.
    Unrecognized,
}

impl AbstainReason {
    /// Wire values shared with `@nextalk/protocol` (T2.7).
    pub fn as_str(self) -> &'static str {
        match self {
            AbstainReason::SilentAudio => "silent_audio",
            AbstainReason::Unrecognized => "unrecognized",
        }
    }
}

/// Token accounting for one translation request (D-13 cost metering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// One glossary entry — **reserved for Phase 4**; Phase 2 passes an empty slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossaryEntry {
    pub zh: String,
    pub en: String,
}

/// One committed Chinese fragment entering the translator.
///
/// Only `committed` partials become fragments (D-16), so this type has no
/// "is it final" flag: by construction it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZhFragment {
    pub text: String,
    /// Monotonic fragment id — the cascade's segment key.
    pub seq: u64,
}

/// One frame of STT output.
#[derive(Debug, Clone, PartialEq)]
pub struct SttPartial {
    /// Text of this frame (interim revisions included).
    pub text: String,
    /// The vendor called this frame final.
    pub is_final: bool,
    /// We may act on it — the only flag that may reach TTS (D-16).
    pub committed: bool,
    /// A `wpgs` revision rewrote earlier text in this frame.
    pub revision_applied: bool,
    /// Vendor or proxy score; `None` when neither exists.
    pub confidence: Option<f32>,
    pub confidence_source: ConfidenceSource,
    /// Model/version that produced this frame (D-08: recorded per sentence).
    pub model_version: String,
    pub provider: String,
}

impl SttPartial {
    /// The Phase 2 default for a vendor with no usable score (讯飞).
    pub fn without_confidence(
        provider: &str,
        model_version: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            text: text.into(),
            is_final: false,
            committed: false,
            revision_applied: false,
            confidence: None,
            confidence_source: ConfidenceSource::ProxyUnavailable,
            model_version: model_version.into(),
            provider: provider.to_string(),
        }
    }
}

/// A stream item from a vendor STT session.
#[derive(Debug, Clone, PartialEq)]
pub enum SttEvent {
    Partial(SttPartial),
    /// The vendor detected speech onset (Deepgram's `SpeechStarted`). Only the
    /// interviewer line emits it, and 02-03's barge-in detection consumes it —
    /// which is why it is a first-class event and not a swallowed log line.
    SpeechStarted {
        at_ms: u64,
    },
    /// The session ended with a classified failure. The stream may still end
    /// cleanly (channel closed) after this.
    Failed(StageError),
}

/// What a caller pushes upstream into an STT session.
///
/// The fragment end is a *message*, not a dropped channel: 讯飞 needs a closing
/// frame with `data.status == 2` to emit its committed transcript, and Deepgram
/// needs a `CloseStream` for the same reason. Closing the sender would be
/// indistinguishable from "the caller gave up".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttUpstream {
    /// One PCM16 @ 16 kHz mono chunk (framing is the client's job).
    Audio(Vec<i16>),
    /// No more audio for this fragment: finalise and flush.
    End,
}

/// A live STT session: audio in, partials out.
pub struct SttStream {
    pub provider: &'static str,
    pub upstream: mpsc::Sender<SttUpstream>,
    pub events: mpsc::Receiver<SttEvent>,
}

impl SttStream {
    pub fn new(
        provider: &'static str,
        upstream: mpsc::Sender<SttUpstream>,
        events: mpsc::Receiver<SttEvent>,
    ) -> Self {
        Self {
            provider,
            upstream,
            events,
        }
    }

    /// The plan's `next_partial` shape: `None` once the session is over
    /// (cleanly or not). Side signals ([`SttEvent::SpeechStarted`]) are skipped,
    /// not treated as the end. Use [`SttStream::next_event`] when the
    /// classification matters — D-09 retries on it.
    pub async fn next_partial(&mut self) -> Option<SttPartial> {
        loop {
            match self.events.recv().await? {
                SttEvent::Partial(partial) => return Some(partial),
                SttEvent::SpeechStarted { .. } => continue,
                SttEvent::Failed(_) => return None,
            }
        }
    }

    /// Error-aware variant of [`SttStream::next_partial`].
    pub async fn next_event(&mut self) -> Option<SttEvent> {
        self.events.recv().await
    }

    /// Push one PCM16 @ 16 kHz mono chunk upstream (framing is the client's job).
    pub async fn send_audio(&self, pcm16: Vec<i16>) -> Result<(), StageError> {
        self.upstream
            .send(SttUpstream::Audio(pcm16))
            .await
            .map_err(|_| StageError::transport(self.provider, "audio queue closed"))
    }

    /// End the fragment: the vendor flushes and returns its committed text.
    pub async fn end_fragment(&self) -> Result<(), StageError> {
        self.upstream
            .send(SttUpstream::End)
            .await
            .map_err(|_| StageError::transport(self.provider, "audio queue closed"))
    }
}

/// A Chinese STT stage (the user path) or an English one (the interviewer path).
pub trait SttSource: Send {
    /// Vendor id used in traces and errors: `"xfyun"`, `"deepgram"`.
    fn provider(&self) -> &'static str;
    /// Model/version this session runs (D-08).
    fn model_version(&self) -> String;
    /// Install the latency rig handle (02-01 contract).
    fn set_marks(&mut self, marks: MarkHandle);
    /// Open a vendor session and spawn its driver task.
    fn start(&mut self, epoch: u64) -> Result<SttStream, StageError>;
}

/// Marks an STT stage as the interviewer's **subtitle-only** line: it must
/// never be wired into the TTS sink (the cascade enforces this in 02-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterviewerTrack;

// ---------------------------------------------------------------------------
// translation
// ---------------------------------------------------------------------------

/// What the translator reports downstream.
#[derive(Debug, Clone, PartialEq)]
pub enum TranslatorEvent {
    /// A streamed English fragment. `final_flag` marks the end of the run for
    /// this fragment (TTS may start on it).
    Fragment {
        text: String,
        final_flag: bool,
        /// D-08: recorded with the sentence, not with the session.
        provider: String,
        model_version: String,
    },
    /// The model declined to translate (D-03).
    Abstained { reason: AbstainReason },
    /// Token accounting for this request (D-13 consumes it in 02-03).
    Usage(TokenUsage),
    /// The request failed; the classification decides retry vs give up.
    Failed(StageError),
}

/// A live translation request.
pub struct TranslatorStream {
    pub events: mpsc::Receiver<TranslatorEvent>,
}

impl TranslatorStream {
    pub fn new(events: mpsc::Receiver<TranslatorEvent>) -> Self {
        Self { events }
    }

    pub async fn next(&mut self) -> Option<TranslatorEvent> {
        self.events.recv().await
    }
}

/// Chinese fragment in, English fragments out.
pub trait Translator: Send {
    fn provider(&self) -> &'static str;
    fn model_version(&self) -> String;
    fn set_marks(&mut self, marks: MarkHandle);
    /// Translate one committed fragment.
    ///
    /// `glossary` is the Phase 4 term base — Phase 2 always passes an empty
    /// slice, and the parameter exists now so the request shape does not change
    /// when the term base lands (the plan forbids dropping it).
    fn translate(
        &mut self,
        fragment: &ZhFragment,
        glossary: &[GlossaryEntry],
        epoch: u64,
    ) -> Result<TranslatorStream, StageError>;
}

// ---------------------------------------------------------------------------
// voice + speech output
// ---------------------------------------------------------------------------

/// A cloned speaker id (`S_xxx` / `ICL_xxx` at 火山).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerId(String);

impl SpeakerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which voice the TTS stage should use (02-04 resolves this per fragment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceRef {
    /// The user's cloned voice.
    Clone(SpeakerId),
    /// A vendor preset voice — the fallback while no clone profile exists.
    Preset(String),
}

/// One chunk of synthesised audio, ready for the playout chain.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    /// Mono samples in [-1, 1).
    pub pcm: Vec<f32>,
    pub sample_rate_hz: u32,
}

impl AudioChunk {
    pub fn new(pcm: Vec<f32>, sample_rate_hz: u32) -> Self {
        Self {
            pcm,
            sample_rate_hz,
        }
    }

    /// Vendor payloads are little-endian PCM16; the playout chain is `f32`.
    pub fn from_pcm16_le(bytes: &[u8], sample_rate_hz: u32) -> Self {
        Self {
            pcm: pcm16_le_to_f32(bytes),
            sample_rate_hz,
        }
    }

    pub fn samples(&self) -> usize {
        self.pcm.len()
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate_hz == 0 {
            return 0;
        }
        (self.pcm.len() as u64) * 1_000 / self.sample_rate_hz as u64
    }
}

/// `i16` little-endian bytes → `f32` samples in [-1, 1).
pub fn pcm16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    // `chunks_exact` rather than `as_chunks` (stable 1.88): this crate keeps
    // Tauri's 1.77 floor. A trailing odd byte is dropped, not misread.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    let pairs = bytes.chunks_exact(2);
    pairs
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32_768.0)
        .collect()
}

/// Character accounting for one synthesis request (D-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TtsUsage {
    pub characters: u32,
    pub text_words: u32,
}

/// A stream item from a synthesis request.
#[derive(Debug, Clone, PartialEq)]
pub enum TtsEvent {
    Audio(AudioChunk),
    /// The vendor finished the request; `usage` is the accounting frame (D-13).
    Finished {
        usage: Option<TtsUsage>,
    },
    Failed(StageError),
}

/// A live synthesis request.
pub struct TtsStream {
    pub events: mpsc::Receiver<TtsEvent>,
}

impl TtsStream {
    pub fn new(events: mpsc::Receiver<TtsEvent>) -> Self {
        Self { events }
    }

    pub async fn next(&mut self) -> Option<TtsEvent> {
        self.events.recv().await
    }

    /// Convenience: pull chunks and stop at `Finished`/`Failed`.
    pub async fn next_audio(&mut self) -> Option<AudioChunk> {
        match self.events.recv().await? {
            TtsEvent::Audio(chunk) => Some(chunk),
            TtsEvent::Finished { .. } | TtsEvent::Failed(_) => None,
        }
    }
}

/// Text in, cloned-voice audio out.
pub trait TtsSink: Send {
    fn provider(&self) -> &'static str;
    fn model_version(&self) -> String;
    fn set_marks(&mut self, marks: MarkHandle);
    fn synthesize(
        &mut self,
        text: &str,
        voice: &VoiceRef,
        epoch: u64,
    ) -> Result<TtsStream, StageError>;
}

// ---------------------------------------------------------------------------
// deterministic doubles (02-01 rig + 02-03 tests reuse these)
// ---------------------------------------------------------------------------

/// Deterministic STT double: replays a scripted sequence, ignores audio.
#[derive(Debug, Clone)]
pub struct ScriptedStt {
    script: Vec<SttPartial>,
    marks: MarkHandle,
    model_version: String,
}

impl ScriptedStt {
    pub fn new(script: Vec<SttPartial>) -> Self {
        Self {
            script,
            marks: MarkHandle::disabled(),
            model_version: "scripted-1".to_string(),
        }
    }

    /// The canonical two-step shape: an interim partial, then the commit.
    pub fn partial_then_final(partial: &str, final_text: &str) -> Self {
        let mut first = SttPartial::without_confidence("scripted", "scripted-1", partial);
        first.is_final = false;
        let mut last = SttPartial::without_confidence("scripted", "scripted-1", final_text);
        last.is_final = true;
        last.committed = true;
        Self::new(vec![first, last])
    }
}

impl SttSource for ScriptedStt {
    fn provider(&self) -> &'static str {
        "scripted"
    }

    fn model_version(&self) -> String {
        self.model_version.clone()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn start(&mut self, _epoch: u64) -> Result<SttStream, StageError> {
        let (upstream_tx, _upstream_rx) = mpsc::channel(AUDIO_QUEUE_FRAMES);
        let (events_tx, events_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
        let script = self.script.clone();
        let marks = self.marks.clone();
        tokio::spawn(async move {
            for (index, partial) in script.into_iter().enumerate() {
                if index == 0 {
                    marks.mark(Stage::SttFirstPartial);
                }
                if events_tx.send(SttEvent::Partial(partial)).await.is_err() {
                    return;
                }
            }
            // Dropping the sender closes the stream: a clean end.
        });
        Ok(SttStream::new("scripted", upstream_tx, events_rx))
    }
}

/// Deterministic translator double: replays a scripted event list.
///
/// Two scripts coexist: `events` replays on **every** call (the always-failing
/// or always-succeeding shapes), `per_call` plays the nth list on the nth call
/// (the last list repeats) — the shape a retry/probe test needs, where the
/// first attempts fail and a later one succeeds.
#[derive(Debug, Clone, Default)]
pub struct ScriptedTranslator {
    events: Vec<TranslatorEvent>,
    per_call: Option<Vec<Vec<TranslatorEvent>>>,
    next_call: usize,
    marks: MarkHandle,
}

impl ScriptedTranslator {
    pub fn new(events: Vec<TranslatorEvent>) -> Self {
        Self {
            events,
            per_call: None,
            next_call: 0,
            marks: MarkHandle::disabled(),
        }
    }

    /// Per-call script (T3.4): the nth `translate()` plays the nth list; the
    /// last list repeats forever. Lets a test script fail→fail→succeed across
    /// the cascade's retry loop.
    pub fn per_call(calls: Vec<Vec<TranslatorEvent>>) -> Self {
        Self {
            events: Vec::new(),
            per_call: Some(calls),
            next_call: 0,
            marks: MarkHandle::disabled(),
        }
    }

    /// One fragment that is already final — what most downstream tests need.
    pub fn one_fragment(text: &str) -> Self {
        Self::new(vec![
            TranslatorEvent::Fragment {
                text: text.to_string(),
                final_flag: true,
                provider: "scripted".to_string(),
                model_version: "scripted-1".to_string(),
            },
            TranslatorEvent::Usage(TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
            }),
        ])
    }

    /// The event list the next call plays.
    fn script_for_call(&mut self) -> Vec<TranslatorEvent> {
        let Some(calls) = &self.per_call else {
            return self.events.clone();
        };
        if calls.is_empty() {
            return self.events.clone();
        }
        let index = self.next_call.min(calls.len() - 1);
        self.next_call += 1;
        calls[index].clone()
    }
}

impl Translator for ScriptedTranslator {
    fn provider(&self) -> &'static str {
        "scripted"
    }

    fn model_version(&self) -> String {
        "scripted-1".to_string()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn translate(
        &mut self,
        _fragment: &ZhFragment,
        _glossary: &[GlossaryEntry],
        _epoch: u64,
    ) -> Result<TranslatorStream, StageError> {
        let (events_tx, events_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
        let events = self.script_for_call();
        let marks = self.marks.clone();
        tokio::spawn(async move {
            let mut marked = false;
            for event in events {
                if !marked {
                    marked = true;
                    marks.mark(Stage::TranslateFirstToken);
                }
                if events_tx.send(event).await.is_err() {
                    return;
                }
            }
        });
        Ok(TranslatorStream::new(events_rx))
    }
}

/// Deterministic TTS double: emits a fixed number of silent chunks.
#[derive(Debug, Clone)]
pub struct ScriptedTts {
    chunks: Vec<AudioChunk>,
    usage: Option<TtsUsage>,
    marks: MarkHandle,
}

impl Default for ScriptedTts {
    fn default() -> Self {
        Self {
            chunks: vec![AudioChunk::new(vec![0.0; 240], 24_000)],
            usage: None,
            marks: MarkHandle::disabled(),
        }
    }
}

impl ScriptedTts {
    pub fn new(chunks: Vec<AudioChunk>) -> Self {
        Self {
            chunks,
            usage: None,
            marks: MarkHandle::disabled(),
        }
    }
}

impl TtsSink for ScriptedTts {
    fn provider(&self) -> &'static str {
        "scripted"
    }

    fn model_version(&self) -> String {
        "scripted-1".to_string()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn synthesize(
        &mut self,
        _text: &str,
        _voice: &VoiceRef,
        _epoch: u64,
    ) -> Result<TtsStream, StageError> {
        let (events_tx, events_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
        let chunks = self.chunks.clone();
        let usage = self.usage;
        let marks = self.marks.clone();
        tokio::spawn(async move {
            for (index, chunk) in chunks.into_iter().enumerate() {
                if index == 0 {
                    marks.mark(Stage::TtsFirstAudio);
                }
                if events_tx.send(TtsEvent::Audio(chunk)).await.is_err() {
                    return;
                }
            }
            let _ = events_tx.send(TtsEvent::Finished { usage }).await;
        });
        Ok(TtsStream::new(events_rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn scripted_stt_yields_partial_then_final() {
        let recorded: Arc<Mutex<Vec<Stage>>> = Arc::new(Mutex::new(Vec::new()));
        let mut source = ScriptedStt::partial_then_final("你能", "你能详细说说吗？");
        source.set_marks(MarkHandle::new({
            let recorded = recorded.clone();
            move |stage| recorded.lock().unwrap().push(stage)
        }));

        let mut stream = source.start(7).expect("scripted source starts");

        let first = stream.next_partial().await.expect("a partial");
        assert!(!first.is_final, "an interim frame is not final");
        assert!(!first.committed, "…and must not be spoken (D-16)");
        assert_eq!(first.confidence, None);
        assert_eq!(first.confidence_source, ConfidenceSource::ProxyUnavailable);
        assert_eq!(first.provider, "scripted");

        let second = stream.next_partial().await.expect("the commit");
        assert!(second.is_final && second.committed);
        assert_ne!(
            first.committed, second.committed,
            "downstream can tell them apart"
        );

        assert!(stream.next_partial().await.is_none(), "stream ends cleanly");
        assert_eq!(
            recorded.lock().unwrap().as_slice(),
            [Stage::SttFirstPartial]
        );
    }

    #[tokio::test]
    async fn scripted_translator_and_tts_follow_the_contract() {
        let mut translator = ScriptedTranslator::one_fragment("The query took 800 ms.");
        let fragment = ZhFragment {
            text: "查询用了 800 毫秒。".to_string(),
            seq: 1,
        };
        let mut stream = translator
            .translate(&fragment, &[], 1)
            .expect("scripted translate");
        let first = stream.next().await.expect("a fragment");
        match first {
            TranslatorEvent::Fragment {
                text,
                final_flag,
                provider,
                ..
            } => {
                assert_eq!(text, "The query took 800 ms.");
                assert!(final_flag);
                assert_eq!(provider, "scripted");
            }
            other => panic!("expected a fragment, got {other:?}"),
        }
        assert!(matches!(
            stream.next().await,
            Some(TranslatorEvent::Usage(_))
        ));

        let mut tts = ScriptedTts::default();
        let voice = VoiceRef::Clone(SpeakerId::new("S_test"));
        let mut audio = tts
            .synthesize("The query took 800 ms.", &voice, 1)
            .expect("scripted synthesize");
        let chunk = audio.next_audio().await.expect("audio");
        assert_eq!(chunk.sample_rate_hz, 24_000);
        assert!(matches!(
            audio.next().await,
            Some(TtsEvent::Finished { usage: None })
        ));
    }

    #[test]
    fn pcm16_le_decoding_is_normalised() {
        let bytes = [0x00, 0x00, 0x00, 0x40, 0xff, 0xff, 0x00, 0x80];
        let samples = pcm16_le_to_f32(&bytes);
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[1], 0.5);
        assert!((samples[2] + 1.0 / 32_768.0).abs() < f32::EPSILON);
        assert_eq!(samples[3], -1.0);

        let chunk = AudioChunk::from_pcm16_le(&bytes, 24_000);
        assert_eq!(chunk.samples(), 4);
        assert_eq!(chunk.duration_ms(), 0);
    }

    #[test]
    fn abstain_reasons_use_the_wire_vocabulary() {
        assert_eq!(AbstainReason::SilentAudio.as_str(), "silent_audio");
        assert_eq!(AbstainReason::Unrecognized.as_str(), "unrecognized");
        assert_eq!(ConfidenceSource::Vendor.as_str(), "vendor");
        assert_eq!(
            ConfidenceSource::ProxyUnavailable.as_str(),
            "proxy_unavailable"
        );
    }
}
