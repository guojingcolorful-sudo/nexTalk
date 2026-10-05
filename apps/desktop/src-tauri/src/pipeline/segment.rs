//! Sentence aggregation (02-03 T3.2).
//!
//! The segmenter sits between the two event sources that disagree about
//! boundaries:
//!
//! - the **vendor** says "committed" (讯飞 `data.status == 2`, Deepgram
//!   `speech_final` / `UtteranceEnd`), and its final always lags the microphone;
//! - the **VAD** says "the room went quiet", which arrives first.
//!
//! A segment therefore has two independent facts: a *text* fact (the vendor's
//! committed transcript) and a *boundary* fact (VAD silence / max duration /
//! the vendor's eos). [`Segmenter`] tracks both and only reports a closed
//! segment once the text is final — never at a heuristic boundary alone, which
//! is what would drop a late final on the floor.
//!
//! Only committed text is ever fed in ([`Segmenter::push_committed`]): the
//! commit gate lives in `cascade.rs` and it is the *only* caller. The
//! segmenter never sees an interim partial, so a `wpgs` preview cannot leak
//! into a segment's text.
//!
//! `speech_ms` accumulates the VAD's voiced duration across the segment; a
//! segment that never reached [`SegmentConfig::min_speech_ms`] is reported as
//! [`DiscardReason::TransientNoise`] (coughs, keyboard clatter), not as an
//! empty sentence.

use crate::pipeline::vad::{EnergyVad, VadConfig, VadEvent, SILENCE_TRIGGER_MS};

/// Voiced milliseconds below which an "utterance" is noise, not speech.
pub const MIN_SPEECH_MS: u64 = 300;
/// Hard ceiling on one segment: an unbounded accumulator breaks the 2 s budget.
pub const MAX_SEGMENT_MS: u64 = 15_000;

/// Why a segment ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The vendor's committed final (`data.status == 2`).
    Eos,
    /// The VAD reported [`SILENCE_TRIGGER_MS`] of quiet.
    Silence,
    /// [`MAX_SEGMENT_MS`] reached — force-closed.
    MaxDuration,
    /// The caller stopped the session (`stop_session`, `interrupt`).
    ManualStop,
}

/// Why a segment was thrown away instead of closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardReason {
    /// Voiced time below [`MIN_SPEECH_MS`] — a transient, not an utterance.
    TransientNoise,
}

/// One aggregated sentence.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Monotonic id — the same key as the 02-01 `segment_id`.
    pub id: u64,
    pub started_at_ms: u64,
    pub closed_at_ms: Option<u64>,
    /// The committed Chinese text, concatenated in arrival order.
    pub zh_text: String,
    pub is_closed: bool,
    pub close_reason: Option<CloseReason>,
    /// Voiced milliseconds the VAD attributed to this segment.
    pub speech_ms: u64,
}

/// What the segmenter reports upward.
#[derive(Debug, Clone, PartialEq)]
pub enum SegmentEvent {
    /// A segment id was claimed (VAD onset or vendor-driven open).
    Opened { id: u64, at_ms: u64 },
    /// The segment is final: text committed and a boundary known.
    Closed(Segment),
    /// The segment was transient noise; it never becomes a sentence.
    Discarded {
        segment: Segment,
        reason: DiscardReason,
    },
    /// Prolonged quiet with nothing in flight (liveness signal).
    SilenceElapsed { at_ms: u64, silence_ms: u64 },
}

/// Tunables — named constants, cited by the failure-case library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentConfig {
    pub silence_trigger_ms: u64,
    pub min_speech_ms: u64,
    pub max_segment_ms: u64,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            silence_trigger_ms: SILENCE_TRIGGER_MS,
            min_speech_ms: MIN_SPEECH_MS,
            max_segment_ms: MAX_SEGMENT_MS,
        }
    }
}

/// State machine: feed frames (VAD) and committed text (vendor); read events.
#[derive(Debug)]
pub struct Segmenter {
    config: SegmentConfig,
    vad: EnergyVad,
    next_id: u64,
    current: Option<Segment>,
    /// A boundary was seen but the text is still outstanding.
    pending_close: Option<CloseReason>,
}

impl Default for Segmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl Segmenter {
    pub fn new() -> Self {
        Self::with_config(SegmentConfig::default())
    }

    pub fn with_config(config: SegmentConfig) -> Self {
        let vad = EnergyVad::new(VadConfig {
            silence_trigger_ms: config.silence_trigger_ms,
            ..VadConfig::default()
        });
        Self {
            config,
            vad,
            next_id: 0,
            current: None,
            pending_close: None,
        }
    }

    /// Push one audio frame (any length; the VAD windows it).
    pub fn push_frame(&mut self, frame: &[f32], at_ms: u64) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        for vad_event in self.vad.push(frame, at_ms) {
            match vad_event {
                VadEvent::SpeechStart { at_ms } => {
                    self.open_if_idle(at_ms, &mut events);
                }
                VadEvent::SpeechEnd { speech_ms, .. } => {
                    if let Some(segment) = self.current.as_mut() {
                        segment.speech_ms += speech_ms;
                    }
                    if self.current.is_some() && self.pending_close.is_none() {
                        self.pending_close = Some(CloseReason::Silence);
                    }
                }
                VadEvent::SilenceElapsed { at_ms, silence_ms } => {
                    events.push(SegmentEvent::SilenceElapsed { at_ms, silence_ms });
                }
            }
        }
        self.enforce_max_duration(at_ms);
        events
    }

    /// A vendor-reported speech onset (Deepgram's `SpeechStarted`).
    pub fn note_speech_start(&mut self, at_ms: u64) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        self.open_if_idle(at_ms, &mut events);
        events
    }

    /// Committed vendor text. `is_final` is the vendor's eos: it closes the
    /// segment (whatever boundary the VAD saw first is reported as the reason).
    pub fn push_committed(&mut self, text: &str, is_final: bool, at_ms: u64) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        self.open_if_idle(at_ms, &mut events);
        if let Some(segment) = self.current.as_mut() {
            // Chinese text: concatenate directly, no separator. The vendor's
            // committed final is the whole sentence in Phase 2's clients, so
            // this is normally a single assignment.
            segment.zh_text.push_str(text);
        }
        if is_final {
            let reason = self.pending_close.take().unwrap_or(CloseReason::Eos);
            self.close(reason, at_ms, &mut events);
        }
        events
    }

    /// The stream ended (or the caller stopped): close whatever is open.
    /// An unclosed `pending_close` wins — it is the earlier boundary.
    pub fn finish(&mut self, at_ms: u64) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        let reason = self.pending_close.take().unwrap_or(CloseReason::ManualStop);
        self.close(reason, at_ms, &mut events);
        events
    }

    fn open_if_idle(&mut self, at_ms: u64, events: &mut Vec<SegmentEvent>) {
        if self.current.is_some() {
            return;
        }
        self.next_id += 1;
        self.current = Some(Segment {
            id: self.next_id,
            started_at_ms: at_ms,
            closed_at_ms: None,
            zh_text: String::new(),
            is_closed: false,
            close_reason: None,
            speech_ms: 0,
        });
        events.push(SegmentEvent::Opened {
            id: self.next_id,
            at_ms,
        });
    }

    fn enforce_max_duration(&mut self, at_ms: u64) {
        let Some(segment) = self.current.as_ref() else {
            return;
        };
        if at_ms.saturating_sub(segment.started_at_ms) >= self.config.max_segment_ms
            && self.pending_close.is_none()
        {
            self.pending_close = Some(CloseReason::MaxDuration);
        }
    }

    fn close(&mut self, reason: CloseReason, at_ms: u64, events: &mut Vec<SegmentEvent>) {
        let Some(mut segment) = self.current.take() else {
            return;
        };
        segment.is_closed = true;
        segment.closed_at_ms = Some(at_ms);
        segment.close_reason = Some(reason);

        // A segment driven by audio only (speech_ms > 0) that never reached the
        // minimum is a cough or a key click, not a sentence. A vendor-driven
        // segment (no audio fed: speech_ms == 0) is the vendor's call.
        let transient = segment.speech_ms > 0 && segment.speech_ms < self.config.min_speech_ms;
        if transient {
            events.push(SegmentEvent::Discarded {
                segment,
                reason: DiscardReason::TransientNoise,
            });
        } else {
            events.push(SegmentEvent::Closed(segment));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::vad::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};

    fn frame(amplitude: f32) -> Vec<f32> {
        (0..FRAME_SAMPLES)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE_HZ as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn vendor_driven_segments_do_not_need_audio() {
        let mut segmenter = Segmenter::new();
        let mut events = segmenter.push_committed("你好", true, 1_000);
        assert!(matches!(
            events.first(),
            Some(SegmentEvent::Opened { id: 1, .. })
        ));
        match events.pop() {
            Some(SegmentEvent::Closed(segment)) => {
                assert_eq!(segment.id, 1);
                assert_eq!(segment.zh_text, "你好");
                assert_eq!(segment.close_reason, Some(CloseReason::Eos));
                assert_eq!(segment.started_at_ms, 1_000);
            }
            other => panic!("expected a closed segment, got {other:?}"),
        }
    }

    #[test]
    fn late_final_lands_on_the_silence_closed_segment() {
        let mut segmenter = Segmenter::with_config(SegmentConfig {
            silence_trigger_ms: 300,
            ..SegmentConfig::default()
        });
        let mut events = Vec::new();
        for index in 0..=40 {
            events.extend(segmenter.push_frame(&frame(0.3), index * FRAME_MS));
        }
        for index in 0..30 {
            events.extend(segmenter.push_frame(&vec![0.0; 480], 410 + index * FRAME_MS));
        }
        // The vendor final arrives well after the VAD boundary.
        events.extend(segmenter.push_committed("完整句子", true, 2_000));

        let closed = events
            .iter()
            .find_map(|event| match event {
                SegmentEvent::Closed(segment) => Some(segment.clone()),
                _ => None,
            })
            .expect("a closed segment");
        assert_eq!(closed.zh_text, "完整句子");
        assert_eq!(closed.close_reason, Some(CloseReason::Silence));
    }

    #[test]
    fn transient_noise_is_discarded_not_closed() {
        let mut segmenter = Segmenter::with_config(SegmentConfig {
            silence_trigger_ms: 300,
            min_speech_ms: 300,
            ..SegmentConfig::default()
        });
        let mut events = Vec::new();
        for index in 0..15 {
            events.extend(segmenter.push_frame(&frame(0.3), index * FRAME_MS)); // 150 ms
        }
        for index in 0..30 {
            events.extend(segmenter.push_frame(&vec![0.0; 480], 150 + index * FRAME_MS));
        }
        events.extend(segmenter.finish(2_000));

        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SegmentEvent::Closed(_))),
            "a 150 ms burst is not a sentence: {events:?}"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            SegmentEvent::Discarded {
                reason: DiscardReason::TransientNoise,
                ..
            }
        )));
    }
}
