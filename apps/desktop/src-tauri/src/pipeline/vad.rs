//! Local energy VAD (02-03 T3.2).
//!
//! Research correction 2: the `webrtc-audio-processing` crate ships AEC3 / NS /
//! AGC and **no** voice-activity detector, and Phase 2 does not pull a second
//! model (silero's ONNX path) in for one boolean. Segmentation therefore gets a
//! small, dependency-free detector built from 10 ms frames and an RMS threshold
//! with hysteresis:
//!
//! - **AC RMS, not RMS.** The frame mean is removed before the energy is
//!   computed, so a DC offset (a stuck mic, a bias in the driver) reads as
//!   silence instead of speech.
//! - **Hysteresis.** Entering speech needs [`SPEECH_ON_RMS`]; staying in it
//!   only needs [`SPEECH_OFF_RMS`]. A signal hovering between the two cannot
//!   flap the state machine into fragment stutter.
//! - **Onset, not confirmation.** [`VadEvent::SpeechStart`] fires at the first
//!   frame above the high threshold, so the barge-in path (T3.3) can apply its
//!   own minimum-speech gate instead of inheriting a delay from here.
//!
//! Consumers apply their own duration policy: the segmenter discards speech
//! shorter than 300 ms ([`crate::pipeline::segment::SegmentConfig`]), barge-in
//! requires 320 ms before it interrupts
//! ([`crate::audio::playout::MIN_INTERRUPT_SPEECH_MS`]). The VAD only reports
//! what the energy says.

/// Frame length in milliseconds — the granularity of the state machine.
pub const FRAME_MS: u64 = 10;
/// The audio graph's rate (02-05 keeps this fixed end to end).
pub const SAMPLE_RATE_HZ: u32 = 48_000;
/// Samples per [`FRAME_MS`] at [`SAMPLE_RATE_HZ`].
pub const FRAME_SAMPLES: usize = 480;

/// AC RMS that starts an utterance (≈ −34 dBFS).
pub const SPEECH_ON_RMS: f32 = 0.02;
/// AC RMS that keeps one alive — the lower half of the hysteresis band.
pub const SPEECH_OFF_RMS: f32 = 0.01;

/// How much continuous silence after speech ends an utterance.
pub const SILENCE_TRIGGER_MS: u64 = 700;

/// What the detector reports upward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    /// Speech onset (first frame at/above [`SPEECH_ON_RMS`]).
    SpeechStart { at_ms: u64 },
    /// Speech ended after [`SILENCE_TRIGGER_MS`] of quiet. `speech_ms` is the
    /// voiced duration of the run — the caller decides whether it was an
    /// utterance or a transient.
    SpeechEnd { at_ms: u64, speech_ms: u64 },
    /// A silence run reached the trigger with no speech in flight. Emitted
    /// once per run; re-arms after speech or after the run is broken.
    SilenceElapsed { at_ms: u64, silence_ms: u64 },
}

/// Tunables. Named, not scattered: the failure-case library cites these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadConfig {
    pub speech_on_rms: f32,
    pub speech_off_rms: f32,
    pub silence_trigger_ms: u64,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            speech_on_rms: SPEECH_ON_RMS,
            speech_off_rms: SPEECH_OFF_RMS,
            silence_trigger_ms: SILENCE_TRIGGER_MS,
        }
    }
}

/// The AC (mean-removed) RMS of one frame.
pub fn frame_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let sum_sq: f32 = samples.iter().map(|s| (s - mean) * (s - mean)).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Energy VAD: a pure state machine over timestamped frames.
///
/// `push` accepts arbitrary-length slices; they are consumed in
/// [`FRAME_SAMPLES`] windows and a partial trailing window is buffered until
/// the next call. `at_ms` is the timestamp of the *last complete frame* in the
/// chunk; earlier frames of the same chunk are back-dated by `FRAME_MS` so a
/// multi-frame chunk still lands on the frame grid.
#[derive(Debug, Clone)]
pub struct EnergyVad {
    config: VadConfig,
    speaking: bool,
    /// Voiced milliseconds accumulated in the current run.
    speech_ms: u64,
    /// Quiet milliseconds accumulated since the last voiced frame.
    silence_run_ms: u64,
    /// Whether `SilenceElapsed` already fired for the current quiet stretch.
    silence_reported: bool,
    /// Samples of an incomplete trailing frame.
    residual: Vec<f32>,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(VadConfig::default())
    }
}

impl EnergyVad {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            speaking: false,
            speech_ms: 0,
            silence_run_ms: 0,
            silence_reported: false,
            residual: Vec::new(),
        }
    }

    pub fn is_speaking(&self) -> bool {
        self.speaking
    }

    /// Voiced milliseconds in the current run (0 while idle).
    pub fn speech_ms(&self) -> u64 {
        self.speech_ms
    }

    pub fn push(&mut self, samples: &[f32], at_ms: u64) -> Vec<VadEvent> {
        let mut events = Vec::new();
        let mut pending: Vec<f32> = std::mem::take(&mut self.residual);
        pending.extend_from_slice(samples);

        let complete = pending.len() / FRAME_SAMPLES;
        for index in 0..complete {
            let start = index * FRAME_SAMPLES;
            let frame = &pending[start..start + FRAME_SAMPLES];
            // Back-date: the last complete frame is `at_ms`, the ones before it
            // sit one frame length earlier on the grid.
            let frames_after = complete - 1 - index;
            let frame_at_ms = at_ms.saturating_sub(frames_after as u64 * FRAME_MS);
            self.step(frame_rms(frame), frame_at_ms, &mut events);
        }

        let consumed = complete * FRAME_SAMPLES;
        self.residual = pending[consumed..].to_vec();
        events
    }

    /// Flush a buffered partial frame. Callers that only ever push whole
    /// frames never need this; a stream that stops mid-frame does.
    pub fn flush(&mut self, at_ms: u64) -> Vec<VadEvent> {
        if self.residual.is_empty() {
            return Vec::new();
        }
        let frame = std::mem::take(&mut self.residual);
        let mut events = Vec::new();
        self.step(frame_rms(&frame), at_ms, &mut events);
        events
    }

    fn step(&mut self, rms: f32, at_ms: u64, events: &mut Vec<VadEvent>) {
        if self.speaking {
            if rms >= self.config.speech_off_rms {
                self.speech_ms += FRAME_MS;
                self.silence_run_ms = 0;
                self.silence_reported = false;
            } else {
                self.silence_run_ms += FRAME_MS;
                if self.silence_run_ms >= self.config.silence_trigger_ms {
                    events.push(VadEvent::SpeechEnd {
                        at_ms,
                        speech_ms: self.speech_ms,
                    });
                    self.speaking = false;
                    self.speech_ms = 0;
                    self.silence_run_ms = 0;
                    self.silence_reported = false;
                }
            }
            return;
        }

        // Idle.
        if rms >= self.config.speech_on_rms {
            self.speaking = true;
            self.speech_ms = FRAME_MS;
            self.silence_run_ms = 0;
            self.silence_reported = false;
            events.push(VadEvent::SpeechStart { at_ms });
            return;
        }

        self.silence_run_ms += FRAME_MS;
        if self.silence_run_ms >= self.config.silence_trigger_ms && !self.silence_reported {
            self.silence_reported = true;
            events.push(VadEvent::SilenceElapsed {
                at_ms,
                silence_ms: self.silence_run_ms,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: f32 = 0.3;
    const NOISE: f32 = 0.05;
    /// Between `speech_off_rms` and `speech_on_rms` — the hysteresis band.
    const EDGE: f32 = 0.015;

    fn frame_sine(amplitude: f32) -> Vec<f32> {
        (0..FRAME_SAMPLES)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE_HZ as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
            })
            .collect()
    }

    fn frame_noise(amplitude: f32) -> Vec<f32> {
        (0..FRAME_SAMPLES)
            .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
            .collect()
    }

    /// Push `frames` whole frames starting at `start_ms` (the timestamp of the
    /// first frame). Continuity between calls is the caller's job — the VAD
    /// trusts the timestamps it is handed.
    fn run(
        vad: &mut EnergyVad,
        start_ms: u64,
        frames: usize,
        mut make: impl FnMut() -> Vec<f32>,
    ) -> Vec<VadEvent> {
        let mut events = Vec::new();
        for i in 0..frames {
            events.extend(vad.push(&make(), start_ms + i as u64 * FRAME_MS));
        }
        events
    }

    #[test]
    fn dc_offset_is_not_speech() {
        let mut vad = EnergyVad::default();
        let events = run(&mut vad, 0, 100, || vec![0.25; FRAME_SAMPLES]);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VadEvent::SpeechStart { .. })),
            "a DC offset is not a voice: {events:?}"
        );
        assert!(!vad.is_speaking());
        // The mean-removed energy reads as silence, so the liveness signal fires.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, VadEvent::SilenceElapsed { .. })),
            "a stuck mic reads as a quiet room: {events:?}"
        );
    }

    #[test]
    fn pure_silence_reports_a_single_silence_elapsed() {
        let mut vad = EnergyVad::default();
        let events = run(&mut vad, 0, 200, || vec![0.0; FRAME_SAMPLES]);
        let elapsed: Vec<_> = events
            .iter()
            .filter(|event| matches!(event, VadEvent::SilenceElapsed { .. }))
            .collect();
        assert_eq!(elapsed.len(), 1, "one report per quiet stretch: {events:?}");
        assert!(!vad.is_speaking());
    }

    #[test]
    fn steady_sine_is_one_utterance() {
        let mut vad = EnergyVad::default();
        let mut events = run(&mut vad, 0, 50, || frame_sine(ON)); // 500 ms
        events.extend(run(&mut vad, 500, 70, || vec![0.0; FRAME_SAMPLES])); // 700 ms

        assert_eq!(
            events.first(),
            Some(&VadEvent::SpeechStart { at_ms: 0 }),
            "{events:?}"
        );
        // The end arrives exactly when the trigger is reached (at 1200 ms).
        match events
            .iter()
            .find(|e| matches!(e, VadEvent::SpeechEnd { .. }))
        {
            Some(VadEvent::SpeechEnd { at_ms, speech_ms }) => {
                assert_eq!(*at_ms, 1_190, "trigger fires on the last quiet frame");
                assert_eq!(*speech_ms, 500, "voiced duration is 500 ms");
            }
            other => panic!("expected one SpeechEnd, got {other:?} in {events:?}"),
        }
    }

    #[test]
    fn burst_of_noise_is_a_short_run_not_a_segment() {
        let mut vad = EnergyVad::default();
        let mut events = run(&mut vad, 0, 20, || frame_noise(NOISE)); // 200 ms
        events.extend(run(&mut vad, 200, 70, || vec![0.0; FRAME_SAMPLES]));

        match events
            .iter()
            .find(|e| matches!(e, VadEvent::SpeechEnd { .. }))
        {
            Some(VadEvent::SpeechEnd { speech_ms, .. }) => {
                assert_eq!(*speech_ms, 200, "the caller decides 200 ms is a transient");
            }
            other => panic!("expected one SpeechEnd, got {other:?} in {events:?}"),
        }
    }

    #[test]
    fn flush_classifies_a_buffered_partial_frame() {
        let mut vad = EnergyVad::default();
        // Half a frame: buffered, not yet classified.
        let partial = frame_sine(ON)[..FRAME_SAMPLES / 2].to_vec();
        assert!(vad.push(&partial, 5).is_empty());
        assert!(!vad.is_speaking());

        // A stream that stops mid-frame flushes the remainder explicitly.
        assert_eq!(
            vad.flush(10),
            vec![VadEvent::SpeechStart { at_ms: 10 }],
            "the buffered half-frame carries the onset"
        );
        assert!(vad.is_speaking());
        assert!(vad.flush(20).is_empty(), "nothing left to flush");
    }

    #[test]
    fn hysteresis_prevents_flapping() {
        let mut vad = EnergyVad::default();
        // Idle: energy in the band (below the on-threshold) never starts speech,
        // even though it is above the off-threshold.
        let mut flip = false;
        let mut events = run(&mut vad, 0, 100, || {
            flip = !flip;
            if flip {
                frame_sine(EDGE)
            } else {
                vec![0.0; FRAME_SAMPLES]
            }
        });
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VadEvent::SpeechStart { .. })),
            "the band cannot start an utterance: {events:?}"
        );

        // Speaking: the same band keeps the run alive — no premature end.
        events = run(&mut vad, 0, 10, || frame_sine(ON));
        assert!(matches!(events.first(), Some(VadEvent::SpeechStart { .. })));
        let mut held = run(&mut vad, 100, 50, || frame_sine(EDGE)); // 500 ms in-band
        held.extend(run(&mut vad, 600, 10, || vec![0.0; FRAME_SAMPLES]));
        assert!(
            !held.iter().any(|e| matches!(e, VadEvent::SpeechEnd { .. })),
            "in-band energy must not end the utterance: {held:?}"
        );
        assert!(vad.is_speaking(), "still speaking after in-band frames");
    }
}
