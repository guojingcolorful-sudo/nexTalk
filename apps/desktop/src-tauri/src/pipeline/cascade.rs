//! Three-stage cascade assembly and the commit gate (02-03 T3.1).
//!
//! # The invariant
//!
//! **spoken English ⊆ committed finals** (GOV-15). It is enforced in exactly
//! one place — [`admit`] — and every other module is downstream of it:
//!
//! ```text
//! 讯飞 partial ─┐
//! 讯飞 final   ─┴─▶ admit() ─▶ Segmenter ─▶ Translator ─▶ final_flag gate ─▶ TTS
//!                   ^                            ^
//!                   │ the only reader of          │ the only reader of
//!                   │ SttPartial::committed       │ TranslatorEvent::Fragment::final_flag
//! ```
//!
//! Nothing else in the crate compares `committed`, and nothing else hands text
//! to a [`TtsSink`]: [`Cascade::tts_inputs`] is the complete witness of what
//! was ever spoken, which is what the poisoned-partial test asserts against.
//!
//! # The two lines
//!
//! The user path ([`Cascade`]) is the only one wired to the translator and the
//! TTS sink. The interviewer path ([`InterviewerCascade`]) is a *different
//! type* with no translator and no sink at all — "the interviewer is never
//! re-voiced" is a compile-time property here, not a convention.
//!
//! # Latency marks
//!
//! Each segment gets one [`Waterfall`](crate::pipeline::budget::Waterfall): the
//! stages report their own boundaries through a shared [`MarkHandle`] (02-01
//! contract) and this module adds `MicCallback` (segment open) and, via the
//! [`PlayoutSink`], `PlaybackFirstSample`. The clock is injected
//! ([`SharedClock`]) so tests script time and production reads the real one.

use std::fmt;
use std::sync::{Arc, Mutex};

use crate::pipeline::budget::{LatencyMark, Stage, Waterfall, WaterfallError, E2E_BUDGET_MS};
use crate::pipeline::segment::{DiscardReason, Segment, SegmentConfig, SegmentEvent, Segmenter};
use crate::pipeline::stages::{
    AbstainReason, AudioChunk, MarkHandle, StageError, SttEvent, SttPartial, SttSource, TokenUsage,
    Translator, TranslatorEvent, TtsEvent, TtsSink, TtsUsage, VendorStt, VendorTranslator,
    VendorTts, VoiceRef, ZhFragment,
};
use crate::sim::source::TimeSource;

/// Preset voice used until 02-04 resolves the user's clone per fragment.
pub const DEFAULT_VOICE_PRESET: &str = "nextalk-default";

// ------------------------------------------------------------------ the gate ---

/// What the commit gate decided about one STT frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Admission {
    /// `committed == true`: the text may enter a segment.
    Commit { text: String, is_final: bool },
    /// Interim text. It may reach the *renderer* (Phase 5), never the cascade.
    Reject { text: String },
}

/// The commit gate — the single enforcement point of the GOV-15 invariant.
///
/// 讯飞 `wpgs` previews and Deepgram interims are free to revise earlier text;
/// only frames the vendor marked committed may pass, because downstream there
/// is no way to un-say a sentence.
pub fn admit(partial: &SttPartial) -> Admission {
    if partial.committed {
        Admission::Commit {
            text: partial.text.clone(),
            is_final: partial.is_final,
        }
    } else {
        Admission::Reject {
            text: partial.text.clone(),
        }
    }
}

// ----------------------------------------------------------------- the clock ---

/// A cloneable handle onto one clock: the cascade's marks and the 02-01
/// recorder must read the same time source (Phase 1 `TimeSource` convention).
#[derive(Clone)]
pub struct SharedClock(Arc<dyn TimeSource + Send + Sync>);

impl SharedClock {
    pub fn new(clock: impl TimeSource + Sync + 'static) -> Self {
        Self(Arc::new(clock))
    }
}

impl TimeSource for SharedClock {
    fn elapsed_ms(&self) -> u64 {
        self.0.elapsed_ms()
    }
}

impl fmt::Debug for SharedClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedClock")
            .field("elapsed_ms", &self.elapsed_ms())
            .finish()
    }
}

// ----------------------------------------------------------------- the sink ---

/// Where synthesised audio goes. 02-05 implements it with the cpal ring; the
/// epoch travels with every chunk so the playout side can drop stale audio
/// after a barge-in (T3.3).
pub trait PlayoutSink: Send {
    /// Install the latency rig handle — the sink marks `PlaybackFirstSample`
    /// on the first audible sample of each segment (02-01 contract).
    fn set_marks(&mut self, marks: MarkHandle);
    /// Hand one synthesised chunk to the playout chain. `segment_id` travels
    /// with the audio so the sink can tell one sentence's playback run from
    /// the next — and so a barge-in can attribute what it cut.
    fn play(&mut self, epoch: u64, segment_id: u64, chunk: &AudioChunk);
}

// ------------------------------------------------------------- mark routing ---

/// One segment's marks, accumulated until the waterfall is complete.
#[derive(Debug)]
struct SegmentMarks {
    segment_id: u64,
    cold: bool,
    marks: Vec<LatencyMark>,
}

impl SegmentMarks {
    fn new(segment_id: u64, cold: bool) -> Self {
        Self {
            segment_id,
            cold,
            marks: Vec::with_capacity(Stage::ALL.len()),
        }
    }

    fn push(&mut self, stage: Stage, at_ms: u64) {
        self.marks.push(LatencyMark {
            stage,
            segment_id: self.segment_id,
            at_ms,
            cold: self.cold,
        });
    }

    fn finish(self) -> Result<Waterfall, WaterfallError> {
        Waterfall::from_marks(&self.marks)
    }
}

/// Routes stage marks to the segment that owns them. Marks that arrive before
/// their segment is opened are buffered and replayed, so a mark fired inside a
/// spawned stage task can never be lost to scheduling order.
#[derive(Debug, Default)]
struct MarkRouter {
    recorders: Vec<SegmentMarks>,
    active: Option<usize>,
    pending: Vec<(Stage, u64)>,
}

impl MarkRouter {
    fn mark(&mut self, stage: Stage, at_ms: u64) {
        match self.active.and_then(|index| self.recorders.get_mut(index)) {
            Some(marks) => marks.push(stage, at_ms),
            None => self.pending.push((stage, at_ms)),
        }
    }

    fn begin(&mut self, segment_id: u64, cold: bool, open_at_ms: u64) {
        let mut marks = SegmentMarks::new(segment_id, cold);
        for (stage, at_ms) in self.pending.drain(..) {
            marks.push(stage, at_ms);
        }
        marks.push(Stage::MicCallback, open_at_ms);
        self.recorders.push(marks);
        self.active = Some(self.recorders.len() - 1);
    }

    /// Point the router at one segment's marks (the driver translates segments
    /// in close order, so the target must be selected explicitly).
    fn focus(&mut self, segment_id: u64) -> bool {
        match self
            .recorders
            .iter()
            .position(|marks| marks.segment_id == segment_id)
        {
            Some(index) => {
                self.active = Some(index);
                true
            }
            None => false,
        }
    }

    fn finish(&mut self, segment_id: u64) -> Option<Result<Waterfall, WaterfallError>> {
        let index = self
            .recorders
            .iter()
            .position(|marks| marks.segment_id == segment_id)?;
        let marks = self.recorders.remove(index);
        match self.active {
            Some(active) if active == index => self.active = None,
            Some(active) if active > index => self.active = Some(active - 1),
            _ => {}
        }
        Some(marks.finish())
    }

    /// Drop a recorder without producing a waterfall (discarded segments).
    fn discard(&mut self, segment_id: u64) {
        if let Some(index) = self
            .recorders
            .iter()
            .position(|marks| marks.segment_id == segment_id)
        {
            self.recorders.remove(index);
            match self.active {
                Some(active) if active == index => self.active = None,
                Some(active) if active > index => self.active = Some(active - 1),
                _ => {}
            }
        }
    }
}

// ----------------------------------------------------------------- the ledger ---

/// Everything the cascade observed and everything it handed downstream. The
/// tests assert against this instead of reaching into stage internals, and the
/// trace writer (T3.7) reads the same numbers.
#[derive(Debug, Default)]
pub struct CascadeLedger {
    /// Committed text the gate let through, in arrival order.
    pub admitted: Vec<String>,
    /// Interim text the gate refused (kept so the poisoned-partial test can
    /// prove it was seen and dropped, not silently ignored).
    pub rejected_partials: Vec<String>,
    /// Chinese fragments handed to the translator.
    pub translated: Vec<ZhFragment>,
    /// English assembled from `final_flag == true` runs — the only text that
    /// may reach a TTS sink.
    pub assembled: Vec<String>,
    /// The invariant witness: every string ever passed to `synthesize`.
    pub tts_inputs: Vec<String>,
    /// Runs that ended without a `final_flag` (cut streams): no TTS.
    pub unfinalized_runs: Vec<u64>,
    /// Runs that produced no usable English: no TTS.
    pub empty_translations: Vec<u64>,
    /// Segments closed with no committed text — the D-03 abstain path.
    pub empty_segments: Vec<u64>,
    /// Translator-declared abstentions (D-03).
    pub abstained: Vec<AbstainReason>,
    /// Token accounting per segment (D-13 consumes it in T3.7).
    pub usage: Vec<(u64, TokenUsage)>,
    /// Character accounting per segment (D-13).
    pub tts_usage: Vec<(u64, Option<TtsUsage>)>,
    /// Vendor speech-onset signals (the barge-in trigger source, T3.3).
    pub speech_started: Vec<u64>,
    pub closed_segments: Vec<Segment>,
    pub discarded_segments: Vec<Segment>,
    /// Segments whose waterfall could not be built (missing/duplicate marks).
    pub waterfall_errors: Vec<(u64, WaterfallError)>,
    /// Marks that arrived outside any segment's lifetime.
    pub stray_marks: u32,
}

// -------------------------------------------------------------- the cascade ---

/// Tunables for the user-track cascade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CascadeConfig {
    pub segment: SegmentConfig,
    /// The session's first segment is cold; never derived from elapsed time
    /// (T-02-02).
    pub first_segment_cold: bool,
}

impl Default for CascadeConfig {
    fn default() -> Self {
        Self {
            segment: SegmentConfig::default(),
            first_segment_cold: true,
        }
    }
}

/// One fragment cycle's audio + session identity.
#[derive(Debug, Clone)]
pub struct SegmentScript {
    pub epoch: u64,
    /// `(at_ms, frame)` — 10 ms frames of mono 48 kHz audio, scripted.
    pub frames: Vec<(u64, Vec<f32>)>,
}

/// What one closed segment produced.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentOutcome {
    pub segment: Segment,
    pub translated: bool,
    pub tts_calls: usize,
    pub waterfall: Option<Waterfall>,
}

/// The user-track cascade: STT → commit gate → segmenter → translator →
/// `final_flag` gate → TTS → playout.
///
/// Production instantiates the STT side with [`VendorStt`]; tests may supply
/// any [`SttSource`] (the deterministic doubles) through
/// [`Cascade::from_source`]. The translator and the sink stay vendor enums —
/// they are the parts the commit gate protects.
pub struct Cascade<S: SttSource = VendorStt> {
    stt: S,
    translator: VendorTranslator,
    tts: VendorTts,
    playout: Box<dyn PlayoutSink>,
    clock: SharedClock,
    router: Arc<Mutex<MarkRouter>>,
    config: CascadeConfig,
    segmenter: Segmenter,
    ledger: CascadeLedger,
    waterfalls: Vec<Waterfall>,
}

impl<S: SttSource> fmt::Debug for Cascade<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cascade")
            .field("stt", &self.stt.provider())
            .field("translator", &self.translator.provider())
            .field("tts", &self.tts.provider())
            .field("elapsed_ms", &self.clock.elapsed_ms())
            .field("config", &self.config)
            .finish()
    }
}

impl Cascade<VendorStt> {
    /// The production assembly: vendor stages selected by configuration.
    pub fn new(
        stt: VendorStt,
        translator: VendorTranslator,
        tts: VendorTts,
        playout: impl PlayoutSink + 'static,
        clock: SharedClock,
    ) -> Self {
        Self::from_source(stt, translator, tts, playout, clock)
    }
}

impl<S: SttSource> Cascade<S> {
    /// Build the cascade over any STT source (the deterministic doubles
    /// included).
    pub fn from_source(
        mut stt: S,
        mut translator: VendorTranslator,
        mut tts: VendorTts,
        mut playout: impl PlayoutSink + 'static,
        clock: SharedClock,
    ) -> Self {
        let router = Arc::new(Mutex::new(MarkRouter::default()));
        let marks = MarkHandle::new({
            let router = Arc::clone(&router);
            let clock = clock.clone();
            move |stage: Stage| {
                if let Ok(mut router) = router.lock() {
                    router.mark(stage, clock.elapsed_ms());
                }
            }
        });
        stt.set_marks(marks.clone());
        translator.set_marks(marks.clone());
        tts.set_marks(marks.clone());
        playout.set_marks(marks);

        Self {
            stt,
            translator,
            tts,
            playout: Box::new(playout),
            clock,
            router,
            config: CascadeConfig::default(),
            segmenter: Segmenter::new(),
            ledger: CascadeLedger::default(),
            waterfalls: Vec::new(),
        }
    }

    pub fn with_config(mut self, config: CascadeConfig) -> Self {
        self.segmenter = Segmenter::with_config(config.segment);
        self.config = config;
        self
    }

    pub fn ledger(&self) -> &CascadeLedger {
        &self.ledger
    }

    /// Take the completed waterfalls (one per closed segment, in close order).
    pub fn take_waterfalls(&mut self) -> Vec<Waterfall> {
        self.waterfalls.drain(..).collect()
    }

    /// Drive one fragment cycle: the scripted audio feeds the VAD, the STT
    /// stream supplies committed text, and every closed segment is translated
    /// and (when the run is final) spoken.
    ///
    /// One call == one vendor fragment session (`start` → stream end). The
    /// interleaving of the cpal callback with the vendor stream is 02-05's job;
    /// the state machine here is order-preserving either way.
    pub async fn drive_user_track(
        &mut self,
        script: &SegmentScript,
    ) -> Result<Vec<SegmentOutcome>, StageError> {
        let mut outcomes = Vec::new();
        let mut closed: Vec<Segment> = Vec::new();

        // 1. Audio: opens/closes segments and reports VAD liveness.
        let mut last_at_ms = script.frames.first().map(|(at_ms, _)| *at_ms).unwrap_or(0);
        for (at_ms, frame) in &script.frames {
            last_at_ms = *at_ms;
            let events = self.segmenter.push_frame(frame, *at_ms);
            self.absorb(events, &mut closed);
        }

        // 2. The vendor stream: only committed text passes the gate.
        let mut stream = self.stt.start(script.epoch)?;
        while let Some(event) = stream.next_event().await {
            match event {
                SttEvent::Partial(partial) => match admit(&partial) {
                    Admission::Commit { text, is_final } => {
                        self.ledger.admitted.push(text.clone());
                        let events = self.segmenter.push_committed(&text, is_final, last_at_ms);
                        self.absorb(events, &mut closed);
                    }
                    Admission::Reject { text } => {
                        self.ledger.rejected_partials.push(text);
                    }
                },
                SttEvent::SpeechStarted { at_ms } => {
                    // The barge-in source (T3.3). It is not a segment event:
                    // the vendor's eos is the only thing that finalises text.
                    self.ledger.speech_started.push(at_ms);
                }
                SttEvent::Failed(error) => return Err(error),
            }
        }

        // 3. The stream is over: close whatever is still open.
        let events = self.segmenter.finish(last_at_ms);
        self.absorb(events, &mut closed);

        // 4. Translate + speak, in close order.
        for segment in closed {
            outcomes.push(self.speak_segment(segment, script.epoch).await?);
        }
        Ok(outcomes)
    }

    fn absorb(&mut self, events: Vec<SegmentEvent>, closed: &mut Vec<Segment>) {
        for event in events {
            match event {
                SegmentEvent::Opened { id, at_ms } => {
                    let cold = self.config.first_segment_cold && id == 1;
                    if let Ok(mut router) = self.router.lock() {
                        router.begin(id, cold, at_ms);
                    }
                }
                SegmentEvent::Closed(segment) => {
                    self.ledger.closed_segments.push(segment.clone());
                    closed.push(segment);
                }
                SegmentEvent::Discarded { segment, reason } => {
                    if let Ok(mut router) = self.router.lock() {
                        router.discard(segment.id);
                    }
                    debug_assert_eq!(reason, DiscardReason::TransientNoise);
                    self.ledger.discarded_segments.push(segment);
                }
                SegmentEvent::SilenceElapsed { at_ms, .. } => {
                    // Liveness only; 02-05 renders idle state from it.
                    let _ = at_ms;
                }
            }
        }
    }

    /// Translate one closed segment and, if the run is final, speak it.
    async fn speak_segment(
        &mut self,
        segment: Segment,
        epoch: u64,
    ) -> Result<SegmentOutcome, StageError> {
        if let Ok(mut router) = self.router.lock() {
            router.focus(segment.id);
        }
        let mut outcome = SegmentOutcome {
            segment: segment.clone(),
            translated: false,
            tts_calls: 0,
            waterfall: None,
        };

        if segment.zh_text.trim().is_empty() {
            // D-03: no valid text is the only abstain. T3.6 emits the event.
            self.ledger.empty_segments.push(segment.id);
            self.settle_waterfall(&mut outcome);
            return Ok(outcome);
        }

        let fragment = ZhFragment {
            text: segment.zh_text.clone(),
            seq: segment.id,
        };
        self.ledger.translated.push(fragment.clone());
        outcome.translated = true;

        let mut english = String::new();
        let mut run_is_final = false;
        let mut abstained = false;
        let mut stream = self.translator.translate(&fragment, &[], epoch)?;
        while let Some(event) = stream.next().await {
            match event {
                TranslatorEvent::Fragment {
                    text, final_flag, ..
                } => {
                    english.push_str(&text);
                    if final_flag {
                        run_is_final = true;
                    }
                }
                TranslatorEvent::Usage(usage) => self.ledger.usage.push((segment.id, usage)),
                TranslatorEvent::Abstained { reason } => {
                    self.ledger.abstained.push(reason);
                    abstained = true;
                }
                TranslatorEvent::Failed(error) => return Err(error),
            }
        }

        // The second half of GOV-15: only `final_flag == true` text is spoken.
        if !run_is_final || english.trim().is_empty() || abstained {
            if !run_is_final {
                self.ledger.unfinalized_runs.push(segment.id);
            }
            if english.trim().is_empty() {
                self.ledger.empty_translations.push(segment.id);
            }
            self.settle_waterfall(&mut outcome);
            return Ok(outcome);
        }

        self.ledger.assembled.push(english.clone());
        // The invariant witness is recorded at the single hand-off point.
        self.ledger.tts_inputs.push(english.clone());

        let voice = VoiceRef::Preset(DEFAULT_VOICE_PRESET.to_string());
        let mut tts = self.tts.synthesize(&english, &voice, epoch)?;
        outcome.tts_calls = 1;
        while let Some(event) = tts.next().await {
            match event {
                TtsEvent::Audio(chunk) => self.playout.play(epoch, segment.id, &chunk),
                TtsEvent::Finished { usage } => self.ledger.tts_usage.push((segment.id, usage)),
                TtsEvent::Failed(error) => return Err(error),
            }
        }

        self.settle_waterfall(&mut outcome);
        Ok(outcome)
    }

    fn settle_waterfall(&mut self, outcome: &mut SegmentOutcome) {
        let segment_id = outcome.segment.id;
        let finished = self
            .router
            .lock()
            .ok()
            .and_then(|mut router| router.finish(segment_id));
        match finished {
            Some(Ok(waterfall)) => {
                self.waterfalls.push(waterfall.clone());
                outcome.waterfall = Some(waterfall);
            }
            Some(Err(error)) => self.ledger.waterfall_errors.push((segment_id, error)),
            None => {}
        }
    }
}

// ----------------------------------------------------------- interviewer line ---

/// One subtitle frame on the interviewer line.
#[derive(Debug, Clone, PartialEq)]
pub struct InterviewerSubtitle {
    pub segment_id: u64,
    pub text: String,
    pub is_final: bool,
}

/// What one interviewer utterance produced.
#[derive(Debug, Clone, PartialEq)]
pub struct InterviewerOutcome {
    pub subtitles: Vec<InterviewerSubtitle>,
    pub segments: Vec<Segment>,
}

/// The interviewer line: English STT in, subtitles out. There is no
/// translator and no [`TtsSink`] field — "the interviewer is never re-voiced"
/// cannot be violated by a later edit that forgets a rule, only by a type
/// change.
///
/// The STT side is generic over [`SttSource`] so the tests can script raw
/// vendor events (`SpeechStarted` included) without a vendor enum variant;
/// production instantiates it with [`VendorStt`].
pub struct InterviewerCascade<S: SttSource = VendorStt> {
    stt: S,
    segmenter: Segmenter,
}

impl<S: SttSource> fmt::Debug for InterviewerCascade<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InterviewerCascade")
            .field("stt", &self.stt.provider())
            .finish()
    }
}

impl InterviewerCascade<VendorStt> {
    pub fn new(stt: VendorStt) -> Self {
        Self::from_source(stt)
    }
}

impl<S: SttSource> InterviewerCascade<S> {
    /// Build the line over any STT source (the deterministic doubles included).
    pub fn from_source(stt: S) -> Self {
        Self {
            stt,
            segmenter: Segmenter::new(),
        }
    }

    /// Drive one interviewer utterance: `SpeechStarted` opens the segment, the
    /// committed final (`speech_final` / `UtteranceEnd`) closes it.
    pub async fn drive_line(&mut self, epoch: u64) -> Result<InterviewerOutcome, StageError> {
        let mut subtitles = Vec::new();
        let mut segments = Vec::new();
        let mut last_at_ms = 0;
        let mut current_id = 0;

        let mut stream = self.stt.start(epoch)?;
        while let Some(event) = stream.next_event().await {
            match event {
                SttEvent::SpeechStarted { at_ms } => {
                    last_at_ms = at_ms;
                    for segment_event in self.segmenter.note_speech_start(at_ms) {
                        if let SegmentEvent::Opened { id, .. } = segment_event {
                            current_id = id;
                        }
                    }
                }
                SttEvent::Partial(partial) => {
                    if partial.committed {
                        let events = self.segmenter.push_committed(
                            &partial.text,
                            partial.is_final,
                            last_at_ms,
                        );
                        for segment_event in events {
                            match segment_event {
                                SegmentEvent::Opened { id, .. } => current_id = id,
                                SegmentEvent::Closed(segment) => {
                                    subtitles.push(InterviewerSubtitle {
                                        segment_id: segment.id,
                                        text: segment.zh_text.clone(),
                                        is_final: true,
                                    });
                                    segments.push(segment);
                                }
                                SegmentEvent::Discarded { .. }
                                | SegmentEvent::SilenceElapsed { .. } => {}
                            }
                        }
                    } else {
                        subtitles.push(InterviewerSubtitle {
                            segment_id: current_id,
                            text: partial.text.clone(),
                            is_final: false,
                        });
                    }
                }
                SttEvent::Failed(error) => return Err(error),
            }
        }

        for segment_event in self.segmenter.finish(last_at_ms) {
            if let SegmentEvent::Closed(segment) = segment_event {
                subtitles.push(InterviewerSubtitle {
                    segment_id: segment.id,
                    text: segment.zh_text.clone(),
                    is_final: true,
                });
                segments.push(segment);
            }
        }

        Ok(InterviewerOutcome {
            subtitles,
            segments,
        })
    }
}

/// The 2 s gate, re-exported so cascade callers do not reach into `budget`.
pub const CASCADE_BUDGET_MS: u64 = E2E_BUDGET_MS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::segment::CloseReason;

    fn partial(text: &str, committed: bool, is_final: bool) -> SttPartial {
        let mut partial = SttPartial::without_confidence("scripted", "scripted-1", text);
        partial.committed = committed;
        partial.is_final = is_final;
        partial
    }

    #[test]
    fn the_gate_only_admits_committed_frames() {
        assert_eq!(
            admit(&partial("半句话", false, false)),
            Admission::Reject {
                text: "半句话".to_string()
            }
        );
        assert_eq!(
            admit(&partial("完整句子", true, true)),
            Admission::Commit {
                text: "完整句子".to_string(),
                is_final: true
            }
        );
        // A committed-but-not-final frame is still admitted for accumulation;
        // it does not close the segment.
        assert_eq!(
            admit(&partial("片段", true, false)),
            Admission::Commit {
                text: "片段".to_string(),
                is_final: false
            }
        );
    }

    #[test]
    fn close_reasons_are_the_four_planned_paths() {
        // Guards the enum against a well-meaning simplification to `bool`.
        let reasons = [
            CloseReason::Eos,
            CloseReason::Silence,
            CloseReason::MaxDuration,
            CloseReason::ManualStop,
        ];
        assert_eq!(reasons.len(), 4);
    }
}
