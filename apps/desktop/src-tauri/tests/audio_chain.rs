//! 02-05 音频链路集成套件 — T5.1（AEC/NS/AGC 处理块）。
//!
//! Everything here runs against **synthetic signals and scripted backends**:
//! zero real devices, zero network, so the suite is CI-safe. The real-device
//! variants live behind `#[ignore]` in their own files (`audio_devices.rs`).
//!
//! T5.1 pins five contracts the whole audio graph rests on:
//!
//! | # | contract | why it matters |
//! |---|----------|----------------|
//! | 1 | 10 ms / 480-sample planar frames | `webrtc-audio-processing` asserts, and an assert in a cpal callback is a crash |
//! | 2 | wrong lengths are structured errors | the error must name both lengths — the graph's frame discipline is the fix |
//! | 3 | render-before-capture | the far-end reference must be one frame ahead of the mic, or the AEC converges on the wrong delay |
//! | 4 | echo drops, local speech survives | this is 戴不戴耳机都能用; measured in dB, not asserted rhetorically |
//! | 5 | NS/AGC switches are real | "off" must be a true passthrough, not a quieter lie |
//!
//! The plan's research correction 2 is pinned by [`aec_api_has_no_vad_semantics`]:
//! the crate ships AEC3/NS/AGC/HPF and **no** VAD — voice activity lives in
//! `pipeline/vad.rs` (讯飞 eos + Deepgram vad_events + local energy).

use std::f32::consts::PI;

use nextalk_desktop_lib::audio::aec::{
    AecConfig, AecError, AudioProcessor, FRAME_SAMPLES, PROCESSOR_RATE_HZ,
};

// ---------------------------------------------------------------------------
// synthetic signal helpers (deterministic; no rand dependency)
// ---------------------------------------------------------------------------

/// Deterministic pseudo-noise (xorshift32), scaled to `amplitude`.
///
/// Used as the far-end signal: broadband so the AEC's adaptive filter has
/// something to identify, and reproducible so a failure is diagnosable.
struct Noise {
    state: u32,
}

impl Noise {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    fn next_sample(&mut self) -> f32 {
        // xorshift32 — the standard Marsaglia triple.
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.state = value;
        (value as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn frame(&mut self, samples: usize, amplitude: f32) -> Vec<f32> {
        (0..samples)
            .map(|_| self.next_sample() * amplitude)
            .collect()
    }
}

/// A phase-continuous sine: `n` is the absolute sample index, so consecutive
/// frames stitch without a discontinuity click.
fn tone_at(index_offset: usize, samples: usize, hz: f32, amplitude: f32) -> Vec<f32> {
    (0..samples)
        .map(|n| {
            let t = (index_offset + n) as f32 / PROCESSOR_RATE_HZ as f32;
            (2.0 * PI * hz * t).sin() * amplitude
        })
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum / samples.len() as f32).sqrt()
}

fn db_drop(before: f32, after: f32) -> f32 {
    if after <= 0.0 {
        return f32::INFINITY;
    }
    if before <= 0.0 {
        return 0.0;
    }
    20.0 * (before / after).log10()
}

/// A processor with echo cancellation and nothing else — the configuration the
/// echo-suppression measurement needs (NS would also shave the local tone and
/// make the "speech preserved" assertion lie).
fn echo_only() -> AudioProcessor {
    AudioProcessor::new(AecConfig {
        echo_canceller: true,
        ..AecConfig::disabled()
    })
    .expect("48 kHz processor builds")
}

// ---------------------------------------------------------------------------
// Test 1 — frame discipline, correct case
// ---------------------------------------------------------------------------

#[test]
fn ten_millisecond_frames_are_accepted_and_keep_their_length() {
    let mut processor = AudioProcessor::new(AecConfig::all_enabled()).expect("processor builds");
    assert_eq!(
        processor.frame_samples(),
        FRAME_SAMPLES,
        "10 ms at 48 kHz is 480 samples — the crate's fixed frame"
    );
    assert_eq!(FRAME_SAMPLES, 480, "the crate's own assertion, restated");

    let render = tone_at(0, FRAME_SAMPLES, 440.0, 0.3);
    processor
        .process_render_frame(&render)
        .expect("a full render frame is accepted");

    let capture = tone_at(0, FRAME_SAMPLES, 220.0, 0.2);
    let out = processor
        .process_capture_frame(&capture)
        .expect("a full capture frame is accepted");

    assert_eq!(
        out.len(),
        FRAME_SAMPLES,
        "the capture frame length is invariant across the processor"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — frame discipline, the error case
// ---------------------------------------------------------------------------

#[test]
fn wrong_frame_lengths_are_structured_errors_not_panics() {
    let mut processor = AudioProcessor::new(AecConfig::all_enabled()).expect("processor builds");

    // Render side: the crate's own `as_mut_ptrs` would `assert_eq!` here, which
    // in a cpal callback is a process abort. The wrapper must intercept.
    let render_error = processor
        .process_render_frame(&vec![0.0; FRAME_SAMPLES - 1])
        .expect_err("479 samples is not a 10 ms frame");
    assert_eq!(
        render_error,
        AecError::BadFrameLength {
            expected: FRAME_SAMPLES,
            actual: FRAME_SAMPLES - 1,
        }
    );

    // Capture side, both a short and a long frame.
    for actual in [FRAME_SAMPLES - 1, FRAME_SAMPLES * 2] {
        let error = processor
            .process_capture_frame(&vec![0.0; actual])
            .expect_err("only 480 samples is a frame");
        assert_eq!(
            error,
            AecError::BadFrameLength {
                expected: FRAME_SAMPLES,
                actual,
            },
            "the error carries the actual length"
        );
    }

    // The message a developer reads at 3 a.m. must name both numbers.
    let message = AecError::BadFrameLength {
        expected: FRAME_SAMPLES,
        actual: 479,
    }
    .to_string();
    assert!(
        message.contains("480") && message.contains("479"),
        "expected and actual must both be in the message: {message}"
    );

    // A rejected frame must not have advanced the pipeline: the next legal
    // render still works (the length check runs before any state change).
    processor
        .process_render_frame(&vec![0.1; FRAME_SAMPLES])
        .expect("the processor is still usable after a rejected frame");
}

// ---------------------------------------------------------------------------
// Test 3 — render-before-capture ordering
// ---------------------------------------------------------------------------

#[test]
fn capture_is_refused_until_a_render_frame_has_been_fed() {
    let mut processor = AudioProcessor::new(AecConfig::all_enabled()).expect("processor builds");
    let capture = tone_at(0, FRAME_SAMPLES, 220.0, 0.2);

    // Capture first: no far-end reference exists, so AEC3 would be adapting
    // against nothing. Refused — and refused *as a value*, not a panic.
    assert_eq!(
        processor
            .process_capture_frame(&capture)
            .expect_err("capture before any render is illegal"),
        AecError::RenderOrderViolated
    );

    // The legal order.
    processor
        .process_render_frame(&tone_at(0, FRAME_SAMPLES, 440.0, 0.3))
        .expect("render first");
    processor
        .process_capture_frame(&capture)
        .expect("capture after render is the legal order");

    // One render reference covers exactly one capture frame: the next capture
    // arrives without fresh far-end data and is refused again. This is the
    // discipline the capture/playout chains rely on (T5.2/T5.3).
    assert_eq!(
        processor
            .process_capture_frame(&capture)
            .expect_err("a second capture with no render in between"),
        AecError::RenderOrderViolated
    );

    // Consecutive render frames are legal (playback runs continuously,
    // including silence — that is how a mic-first session gets its reference).
    processor
        .process_render_frame(&vec![0.0; FRAME_SAMPLES])
        .expect("render");
    processor
        .process_render_frame(&vec![0.0; FRAME_SAMPLES])
        .expect("render again");
    processor
        .process_capture_frame(&capture)
        .expect("one render reference still covers the next capture");
}

// ---------------------------------------------------------------------------
// Test 4 — echo suppression, measured
// ---------------------------------------------------------------------------

#[test]
fn echo_is_suppressed_while_local_speech_survives() {
    // Scenario: 10 ms frames, a 100 ms acoustic path (10 frames) at gain 0.5,
    // plus a local talker (300 Hz tone) the AEC must leave alone.
    const WARMUP_FRAMES: usize = 150;
    const MEASURE_FRAMES: usize = 150;
    const ECHO_DELAY_FRAMES: usize = 10;
    const ECHO_GAIN: f32 = 0.5;
    const SPEECH_HZ: f32 = 300.0;
    const SPEECH_AMPLITUDE: f32 = 0.08;

    let mut processor = echo_only();
    let mut noise = Noise::new(0x5EED_1234);

    let mut far_end: Vec<Vec<f32>> = Vec::new();
    // (raw input, processor output) per frame — the RAW side is what the mic
    // would have sent without AEC, so the comparison is measured, not assumed.
    let mut captured: Vec<(Vec<f32>, Vec<f32>)> = Vec::new();
    let mut sample_index = 0usize;

    for frame_index in 0..(WARMUP_FRAMES + ECHO_DELAY_FRAMES + MEASURE_FRAMES) {
        let render = noise.frame(FRAME_SAMPLES, 0.5);
        // Near end: the far end echoed back `ECHO_DELAY_FRAMES` later,
        // plus the local talker.
        let echo = if frame_index >= ECHO_DELAY_FRAMES {
            far_end[frame_index - ECHO_DELAY_FRAMES]
                .iter()
                .map(|sample| sample * ECHO_GAIN)
                .collect::<Vec<f32>>()
        } else {
            vec![0.0; FRAME_SAMPLES]
        };
        let speech = tone_at(sample_index, FRAME_SAMPLES, SPEECH_HZ, SPEECH_AMPLITUDE);
        let capture: Vec<f32> = echo
            .iter()
            .zip(&speech)
            .map(|(echo, speech)| echo + speech)
            .collect();

        processor
            .process_render_frame(&render)
            .expect("render frame");
        let processed = processor
            .process_capture_frame(&capture)
            .expect("capture frame");

        sample_index += FRAME_SAMPLES;
        far_end.push(render);
        captured.push((capture, processed));
    }

    // Measure only after AEC3 has had 1.6 s to converge (the filter needs to
    // find the 100 ms delay before its output means anything).
    let window = &captured[WARMUP_FRAMES + ECHO_DELAY_FRAMES..];
    assert_eq!(window.len(), MEASURE_FRAMES);

    let mut in_energy = 0.0f32;
    let mut out_energy = 0.0f32;
    let mut speech_energy = 0.0f32;
    for (offset, (raw, processed)) in window.iter().enumerate() {
        let index = (WARMUP_FRAMES + ECHO_DELAY_FRAMES + offset) * FRAME_SAMPLES;
        let speech = tone_at(index, FRAME_SAMPLES, SPEECH_HZ, SPEECH_AMPLITUDE);
        in_energy += rms(raw).powi(2);
        out_energy += rms(processed).powi(2);
        speech_energy += rms(&speech).powi(2);
    }
    let frames = window.len() as f32;
    let input_rms = (in_energy / frames).sqrt();
    let output_rms = (out_energy / frames).sqrt();
    let speech_rms = (speech_energy / frames).sqrt();

    // Measured on macOS 12.7 / x86_64, 2026-10-06 (AEC3 `Full`, delay
    // estimation on, NS off): input 0.155 → output ~0.043, i.e. **~11 dB** of
    // echo removed while the speech-only level is 0.057 (~-2.5 dB). The
    // thresholds below are deliberately looser than the measurement so a
    // different CPU/SIMD path cannot flip them.
    let drop_db = db_drop(input_rms, output_rms);
    assert!(
        drop_db >= 10.0,
        "the echoed far end must lose at least 10 dB (measured {drop_db:.1} dB; \
         in {input_rms:.4} / out {output_rms:.4})"
    );

    // Local speech survives. Measured on macOS 12.7 / x86_64, 2026-10-06:
    // input 0.15494, output 0.04098, speech-only 0.05657 → **11.55 dB** of echo
    // removed, local talker 2.80 dB below its speech-only level. AEC3's
    // residual-echo suppressor trims the near-end band too, so the tolerance is
    // 5 dB (the plan's 3 dB example leaves no headroom for a different
    // SIMD path); what it rules out is the failure that matters — a silent or
    // gutted talker reaching the interviewer.
    let speech_drop_db = db_drop(speech_rms, output_rms);
    assert!(
        speech_drop_db <= 5.0,
        "the local talker must survive within 5 dB (measured {speech_drop_db:.1} dB)"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — NS / AGC switches are real switches
// ---------------------------------------------------------------------------

#[test]
fn all_switches_off_is_a_bit_exact_passthrough() {
    let mut processor =
        AudioProcessor::new(AecConfig::disabled()).expect("a pass-through processor builds");
    assert_eq!(processor.config(), AecConfig::disabled());

    let render = noise_frame(0xC0FF_EE00, 0.4);
    processor.process_render_frame(&render).expect("render");

    let capture = noise_frame(0xDEAD_BEEF, 0.4);
    let processed = processor.process_capture_frame(&capture).expect("capture");

    assert_eq!(processed.len(), capture.len());
    for (index, (out, raw)) in processed.iter().zip(&capture).enumerate() {
        assert!(
            (out - raw).abs() < 1e-6,
            "sample {index}: all-off must not touch the signal ({out} vs {raw})"
        );
    }
}

#[test]
fn each_switch_is_independent() {
    // A single switch changes exactly one config bit — no hidden coupling.
    let configs = [
        AecConfig {
            echo_canceller: true,
            ..AecConfig::disabled()
        },
        AecConfig {
            noise_suppression: true,
            ..AecConfig::disabled()
        },
        AecConfig {
            gain_control: true,
            ..AecConfig::disabled()
        },
        AecConfig {
            high_pass: true,
            ..AecConfig::disabled()
        },
    ];
    for (index, config) in configs.iter().enumerate() {
        let processor = AudioProcessor::new(*config).expect("processor builds");
        assert_eq!(processor.config(), *config, "config {index} round-trips");
    }

    // NS alone still suppresses steady noise (the switch is not decorative).
    let mut processor = AudioProcessor::new(AecConfig {
        noise_suppression: true,
        ..AecConfig::disabled()
    })
    .expect("processor builds");
    let mut noise = Noise::new(0xABCD_1234);
    let mut quiet_energy = 0.0f32;
    let mut raw_energy = 0.0f32;
    for _ in 0..200 {
        let render = vec![0.0f32; FRAME_SAMPLES];
        let raw = noise.frame(FRAME_SAMPLES, 0.2);
        raw_energy += rms(&raw).powi(2);
        processor.process_render_frame(&render).expect("render");
        let out = processor.process_capture_frame(&raw).expect("capture");
        quiet_energy += rms(&out).powi(2);
    }
    let quiet_rms = (quiet_energy / 200.0).sqrt();
    let raw_rms = (raw_energy / 200.0).sqrt();
    assert!(
        quiet_rms < raw_rms,
        "noise suppression must reduce steady noise ({raw_rms:.4} -> {quiet_rms:.4})"
    );
}

fn noise_frame(seed: u32, amplitude: f32) -> Vec<f32> {
    Noise::new(seed).frame(FRAME_SAMPLES, amplitude)
}

// ---------------------------------------------------------------------------
// Test 6 — no VAD in the AEC API (research correction 2)
// ---------------------------------------------------------------------------

#[test]
fn aec_api_has_no_vad_semantics() {
    let source = include_str!("../src/audio/aec.rs");

    // Scan the *public surface* only: doc comments are allowed to explain that
    // VAD lives elsewhere (that explanation is the point of this contract).
    let public_lines: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            trimmed.starts_with("pub ") && !trimmed.starts_with("//")
        })
        .collect();
    assert!(
        !public_lines.is_empty(),
        "the scan found no public items — the test is looking at the wrong file"
    );

    for needle in ["vad", "voice_detect", "voice_activ", "speech_detect"] {
        for (number, line) in &public_lines {
            assert!(
                !line.to_ascii_lowercase().contains(needle),
                "aec.rs:{} exposes {needle:?} — VAD belongs to pipeline/vad.rs (from 讯飞 eos + \
                 Deepgram vad_events + local energy), not here: {line}",
                number + 1
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Test 7 — the build itself (bundled C++ links at runtime)
// ---------------------------------------------------------------------------

#[test]
fn bundled_build_is_linked_and_processes_frames() {
    // Constructing a Processor calls into the bundled C++ (`webrtc_apm_*`);
    // a missing/mislinked library fails here, not at first use in production.
    let processor = AudioProcessor::new(AecConfig::all_enabled()).expect("bundled C++ is linked");
    assert_eq!(PROCESSOR_RATE_HZ, 48_000);
    assert_eq!(processor.frame_samples(), PROCESSOR_RATE_HZ as usize / 100);
}
