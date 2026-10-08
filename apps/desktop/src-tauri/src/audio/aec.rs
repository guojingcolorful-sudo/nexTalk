//! Capture preprocessing: AEC3 echo cancellation, noise suppression, AGC and
//! the high-pass filter (02-05 T5.1).
//!
//! This is the block that makes 戴不戴耳机都能用 true: the microphone hears the
//! speakers, and without an echo canceller the user's own cloned voice becomes
//! the next thing the cascade transcribes. The far end is the **render** side
//! (what the device is actually playing — see
//! [`RenderReference`](crate::audio::RenderReference) and the playout chain in
//! [`crate::audio::playout`]), the near end is the **capture** side.
//!
//! # Frame discipline
//!
//! `webrtc-audio-processing` processes fixed **10 ms** frames — [`FRAME_SAMPLES`]
//! (480) at [`PROCESSOR_RATE_HZ`] (48 kHz) — and its internals `assert_eq!` the
//! length. An assert inside a cpal callback is a process abort, not an error
//! return, so this module validates the length *before* the crate ever sees the
//! frame and reports both numbers in a structured error (T-02-23).
//!
//! # Ordering
//!
//! AEC3 needs the far-end reference for the audio the mic is *about to* hear.
//! Feeding capture before any render means adapting against nothing, so
//! [`AudioProcessor::process_capture_frame`] refuses until at least one render
//! frame has arrived since the last capture ([`AecError::RenderOrderViolated`]).
//! In production the output callback runs continuously (silence included), so
//! the reference is always one tick ahead.
//!
//! # No VAD here
//!
//! The crate ships AEC3 / NS / AGC / high-pass and **no** voice-activity
//! detection (research correction 2). Voice activity belongs to
//! [`crate::pipeline::vad`] — 讯飞 `eos` + Deepgram `vad_events` + the local
//! energy VAD — and must not be smuggled into this file's public surface. The
//! `audio_chain::aec_api_has_no_vad_semantics` test enforces that.

// The config structs live in `webrtc-audio-processing-config`, re-exported as
// `webrtc_audio_processing::config` so this crate needs one dependency, not two.
use webrtc_audio_processing::config::{
    Config, EchoCanceller, GainController, GainController1, GainControllerMode, HighPassFilter,
    NoiseSuppression,
};
use webrtc_audio_processing::{Error as ApmError, Processor};

/// The graph's processing rate. 48 kHz is the natural rate of the local graph
/// (the VAD's frame geometry, the cpal device path, the AEC frame size).
pub const PROCESSOR_RATE_HZ: u32 = 48_000;

/// One processing frame: 10 ms at 48 kHz. Fixed by the crate, not by us —
/// `GetFrameSize(sample_rate_hz) == sample_rate_hz / 100`.
pub const FRAME_SAMPLES: usize = (PROCESSOR_RATE_HZ / 100) as usize;

/// Which submodules the processor runs. They are independent switches on
/// purpose: the echo measurement in the test suite needs AEC without NS (NS
/// would also shave the local talker), and "all off" must be a true passthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AecConfig {
    /// AEC3. The reason this module exists.
    pub echo_canceller: bool,
    /// Background noise suppression (moderate level).
    pub noise_suppression: bool,
    /// Digital AGC ([`GainControllerMode::AdaptiveDigital`] — there is no
    /// analog volume control to prescribe to on this path).
    pub gain_control: bool,
    /// High-pass filter. Strongly recommended alongside AEC; enabling NS
    /// force-enables it inside the crate regardless of this switch.
    pub high_pass: bool,
}

impl AecConfig {
    /// Every submodule off — the configuration the pass-through test pins.
    pub const fn disabled() -> Self {
        Self {
            echo_canceller: false,
            noise_suppression: false,
            gain_control: false,
            high_pass: false,
        }
    }

    /// The production default: everything on.
    pub const fn all_enabled() -> Self {
        Self {
            echo_canceller: true,
            noise_suppression: true,
            gain_control: true,
            high_pass: true,
        }
    }

    /// Translate the switches into the crate's `Config`. `None` is a real
    /// "disabled" in the crate — this is not a soft preference.
    fn to_vendor(self) -> Config {
        Config {
            // `EchoCanceller::default()` is `Full { stream_delay_ms: None }`:
            // AEC3 with delay **estimation** on. The device path is not fixed
            // (built-in speakers vs. headphones vs. BlackHole in Phase 3), so
            // hard-coding a delay would be wrong for at least one of them.
            echo_canceller: self.echo_canceller.then(EchoCanceller::default),
            noise_suppression: self.noise_suppression.then(NoiseSuppression::default),
            gain_controller: self.gain_control.then(|| {
                GainController::GainController1(GainController1 {
                    mode: GainControllerMode::AdaptiveDigital,
                    ..GainController1::default()
                })
            }),
            high_pass_filter: self.high_pass.then(HighPassFilter::default),
            // Pipeline::default() keeps the 48 kHz internal rate — the rate the
            // frames are at, so no internal resampling is introduced.
            ..Config::default()
        }
    }
}

impl Default for AecConfig {
    fn default() -> Self {
        Self::all_enabled()
    }
}

/// What can go wrong before a frame reaches the C++ library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AecError {
    /// The frame is not exactly [`FRAME_SAMPLES`] long. Carries both numbers:
    /// the frame-assembly code is what has to change, and it needs to know by
    /// how much.
    BadFrameLength { expected: usize, actual: usize },
    /// Capture arrived with no render frame to reference (see the module docs).
    RenderOrderViolated,
    /// The crate rejected the configuration or the call itself.
    Processor(String),
}

impl std::fmt::Display for AecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadFrameLength { expected, actual } => write!(
                formatter,
                "音频帧长度不符：期望 {expected} 个样本（10ms @48kHz），实际 {actual} 个"
            ),
            Self::RenderOrderViolated => {
                write!(
                    formatter,
                    "处理顺序错误：采集帧之前必须先喂入渲染帧（AEC 远端参考）"
                )
            }
            Self::Processor(detail) => write!(formatter, "音频处理器错误：{detail}"),
        }
    }
}

impl std::error::Error for AecError {}

impl From<ApmError> for AecError {
    fn from(error: ApmError) -> Self {
        Self::Processor(error.to_string())
    }
}

/// The capture preprocessing block. One instance per capture path; it owns the
/// crate's `Processor` (which is internally synchronized) plus the frame
/// discipline and the render/capture ordering state.
pub struct AudioProcessor {
    inner: Processor,
    config: AecConfig,
    /// Planar frame scratch: the crate wants `Iterator<Item: AsMut<[f32]>>`,
    /// i.e. channel-major. Mono — one channel of [`FRAME_SAMPLES`].
    frame: Vec<Vec<f32>>,
    /// Has a render frame arrived that no capture frame has consumed yet?
    render_pending: bool,
}

impl AudioProcessor {
    /// Build a processor at the graph rate. Fails only if the C++ layer
    /// refuses the rate (a programming error — 48 kHz is the crate's own
    /// documented rate) or the config is invalid.
    pub fn new(config: AecConfig) -> Result<Self, AecError> {
        let inner = Processor::new(PROCESSOR_RATE_HZ)?;
        inner.set_config(config.to_vendor());
        Ok(Self {
            inner,
            config,
            frame: vec![vec![0.0; FRAME_SAMPLES]],
            render_pending: false,
        })
    }

    pub fn config(&self) -> AecConfig {
        self.config
    }

    /// Samples per frame per channel — 480 at 48 kHz. Callers size their
    /// upstream frame assembly from this instead of restating the constant.
    pub fn frame_samples(&self) -> usize {
        self.inner.num_samples_per_frame()
    }

    /// Feed one 10 ms frame of **far-end** (device output) audio. The crate
    /// does not modify the render frame; we copy into the planar scratch and
    /// hand over a mutable view.
    pub fn process_render_frame(&mut self, frame: &[f32]) -> Result<(), AecError> {
        self.check_length(frame.len())?;
        self.frame[0].copy_from_slice(frame);
        self.inner.process_render_frame(self.frame.iter_mut())?;
        self.render_pending = true;
        Ok(())
    }

    /// Feed one 10 ms frame of **near-end** (microphone) audio and get the
    /// cleaned frame back: echo removed, noise suppressed, gain normalised.
    ///
    /// Refused when no render frame has been fed since the last capture — see
    /// the module docs. The returned frame is always [`FRAME_SAMPLES`] long.
    pub fn process_capture_frame(&mut self, frame: &[f32]) -> Result<Vec<f32>, AecError> {
        // Length first: a badly sized frame is a caller bug that must be named,
        // not masked behind an ordering complaint.
        self.check_length(frame.len())?;
        if !self.render_pending {
            return Err(AecError::RenderOrderViolated);
        }
        self.frame[0].copy_from_slice(frame);
        self.inner.process_capture_frame(self.frame.iter_mut())?;
        self.render_pending = false;
        Ok(self.frame[0].clone())
    }

    fn check_length(&self, actual: usize) -> Result<(), AecError> {
        let expected = self.frame_samples();
        if actual == expected {
            Ok(())
        } else {
            Err(AecError::BadFrameLength { expected, actual })
        }
    }
}

impl std::fmt::Debug for AudioProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AudioProcessor")
            .field("config", &self.config)
            .field("frame_samples", &self.frame_samples())
            .field("render_pending", &self.render_pending)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// the shared handle (T5.2/T5.3)
// ---------------------------------------------------------------------------

/// One processor, two chains.
///
/// The render frames come from the playout chain (what the device is playing)
/// and the capture frames from the capture chain (what the mic hears). They run
/// on different threads, so both hold a clone of this handle rather than half
/// of an `AudioProcessor` each — a second processor would mean a second delay
/// estimate and an AEC that cancels nothing.
///
/// The mutex is held for one 10 ms frame's worth of C++ work; that is a real
/// cost on the capture path, and it is the price of one shared delay estimate.
/// It is *not* held in the cpal callback — the chains call it from their drain
/// workers.
#[derive(Clone)]
pub struct SharedProcessor {
    inner: std::sync::Arc<std::sync::Mutex<AudioProcessor>>,
}

impl SharedProcessor {
    pub fn new(config: AecConfig) -> Result<Self, AecError> {
        Ok(Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(AudioProcessor::new(config)?)),
        })
    }

    /// The production configuration (all submodules on).
    pub fn all_enabled() -> Result<Self, AecError> {
        Self::new(AecConfig::all_enabled())
    }

    pub fn frame_samples(&self) -> usize {
        self.lock().frame_samples()
    }

    pub fn process_render_frame(&self, frame: &[f32]) -> Result<(), AecError> {
        self.lock().process_render_frame(frame)
    }

    pub fn process_capture_frame(&self, frame: &[f32]) -> Result<Vec<f32>, AecError> {
        self.lock().process_capture_frame(frame)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AudioProcessor> {
        // Same reasoning as the playout queue: the guarded data is plain audio,
        // so recovering a poisoned lock beats silencing the user's voice.
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// The playout chain mirrors what it renders into the AEC as the far-end
/// reference. The verdict travels back to the caller — the queue owns the frame
/// discipline (it is the side that can carry a remainder across ticks) and
/// counts what the canceller refused, so there is exactly one counter and a
/// reader for it (CR-01; WR-03).
impl crate::audio::RenderReference for SharedProcessor {
    fn push_reference(&mut self, samples: &[f32], _sample_rate_hz: u32) -> bool {
        self.process_render_frame(samples).is_ok()
    }
}

impl std::fmt::Debug for SharedProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedProcessor")
            .field("frame_samples", &self.frame_samples())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_geometry_is_the_crates_contract() {
        assert_eq!(PROCESSOR_RATE_HZ, 48_000);
        assert_eq!(FRAME_SAMPLES, 480, "10 ms at 48 kHz");
    }

    #[test]
    fn disabled_config_maps_to_no_submodules() {
        let vendor = AecConfig::disabled().to_vendor();
        assert!(vendor.echo_canceller.is_none());
        assert!(vendor.noise_suppression.is_none());
        assert!(vendor.gain_controller.is_none());
        assert!(vendor.high_pass_filter.is_none());
    }

    #[test]
    fn all_enabled_maps_each_switch_to_its_submodule() {
        let vendor = AecConfig::all_enabled().to_vendor();
        assert_eq!(vendor.echo_canceller, Some(EchoCanceller::default()));
        assert!(vendor.noise_suppression.is_some());
        assert!(vendor.gain_controller.is_some());
        assert!(vendor.high_pass_filter.is_some());
    }

    #[test]
    fn message_names_both_lengths() {
        let error = AecError::BadFrameLength {
            expected: 480,
            actual: 479,
        };
        let message = error.to_string();
        assert!(message.contains("480"), "{message}");
        assert!(message.contains("479"), "{message}");
    }
}
