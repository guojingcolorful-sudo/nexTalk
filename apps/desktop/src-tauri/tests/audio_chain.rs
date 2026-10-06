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

// ===========================================================================
// T5.2 — the capture chain and the streaming resampler
// ===========================================================================
//
// The chain is 48 kHz mic blocks → 10 ms frames → AEC → 16 kHz mono PCM16 for
// the STT stage. Every test below drives it through `ScriptedSource`, so there
// is no microphone, no device and no network. The real cpal path is a separate
// `#[ignore]` test at the bottom.

use nextalk_desktop_lib::audio::aec::SharedProcessor;
use nextalk_desktop_lib::audio::bounded::CaptureSink;
use nextalk_desktop_lib::audio::capture::{
    BlockSource, CaptureChain, CaptureError, CpalSource, FrameAssembler, ScriptedSource,
    GRAPH_RATE_HZ, STT_RATE_HZ,
};
use nextalk_desktop_lib::audio::resample::StreamingResampler;
use nextalk_desktop_lib::pipeline::stages::xfyun::{audio_frames, AUDIO_FRAME_BYTES};

/// A sine at the graph rate, phase-continuous across blocks.
fn sine_48k(hz: f32, amplitude: f32, samples: usize) -> Vec<f32> {
    tone_at(0, samples, hz, amplitude)
}

/// The dominant frequency of a periodic signal, from its **rising** zero
/// crossings — the cheap frequency witness the plan asks for.
///
/// Counting both directions is fragile here: `f32::signum()` calls `+0.0`
/// positive, so the trio `[+a, 0.0, -b]` reads as one same-sign step plus one
/// step that starts at zero, and the downward crossing disappears. Rising
/// crossings have no such ambiguity, and one rising crossing is exactly one
/// period.
fn dominant_hz(samples: &[f32], rate_hz: u32) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let rising = samples
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count();
    rising as f32 * rate_hz as f32 / samples.len() as f32
}

/// The chain under test: a scripted 48 kHz source, an all-off AEC (this suite
/// measures the *chain*, and T5.1 already measured the echo canceller) and the
/// real streaming resampler.
fn chain_over(blocks: Vec<Vec<f32>>, capacity: usize) -> CaptureChain {
    let source = ScriptedSource::new(GRAPH_RATE_HZ, blocks).with_capacity(capacity);
    let processor = SharedProcessor::new(AecConfig::disabled()).expect("pass-through processor");
    CaptureChain::start(Box::new(source), processor).expect("the chain starts over a script")
}

// ---------------------------------------------------------------------------
// T5.2 Test 1 — end to end: 48 k sine in, 16 k PCM16 out
// ---------------------------------------------------------------------------

#[test]
fn chain_delivers_a_16k_sine_with_the_right_length_and_frequency() {
    // 0.5 s of 440 Hz at 48 kHz, delivered in 480-sample blocks (10 ms each).
    let hz = 440.0;
    let samples = GRAPH_RATE_HZ as usize / 2;
    let blocks: Vec<Vec<f32>> = sine_48k(hz, 0.5, samples)
        .chunks(480)
        .map(<[f32]>::to_vec)
        .collect();
    let expected_frames = blocks.len();
    let mut chain = chain_over(blocks, 64);

    let output = chain.poll().expect("the chain drains");

    // Length: exactly one third of the input, within one 160-sample chunk (the
    // resampler holds a sub-chunk remainder back until `flush`).
    let expected_out = samples / 3;
    assert!(
        output.pcm16.len() <= expected_out && output.pcm16.len() >= expected_out - 160,
        "expected ≈{expected_out} samples at 16 kHz, got {}",
        output.pcm16.len()
    );

    // Frequency: measured over the middle of the signal. The edges are
    // excluded because the FFT resampler rings there while its filter settles,
    // and ringing around zero is exactly what a crossing count cannot tell
    // from the tone.
    let as_f32: Vec<f32> = output.pcm16.iter().map(|s| *s as f32 / 32_767.0).collect();
    let middle = &as_f32[as_f32.len() / 20..as_f32.len() * 19 / 20];
    let measured_hz = dominant_hz(middle, STT_RATE_HZ);
    assert!(
        (measured_hz - hz).abs() < 2.0,
        "the tone must survive the chain: measured {measured_hz:.1} Hz, wanted {hz} Hz"
    );
    // Amplitude too: a tone that arrived but 20 dB quieter would pass a
    // frequency check and fail the interview.
    assert!(
        (rms(&as_f32) - 0.5 / 2.0_f32.sqrt()).abs() < 0.02,
        "the amplitude must survive the chain: rms {:.4}, wanted {:.4}",
        rms(&as_f32),
        0.5 / 2.0_f32.sqrt()
    );

    // The chain's own bookkeeping agrees with what came out.
    let stats = chain.stats();
    assert_eq!(stats.blocks_received, expected_frames as u64);
    assert_eq!(stats.frames_processed, expected_frames as u64);
    assert_eq!(stats.frames_dropped, 0);
    assert_eq!(stats.output_samples, output.pcm16.len() as u64);

    // The 48 kHz side is handed over too — the segmenter's VAD consumes it.
    assert_eq!(output.frames.len(), expected_frames);
    assert_eq!(output.duration_ms(), 500);
    assert!(output.frames.iter().all(|frame| frame.len() == 480));
}

// ---------------------------------------------------------------------------
// T5.2 Test 2 — arbitrary block seams duplicate and drop nothing
// ---------------------------------------------------------------------------

#[test]
fn arbitrary_block_seams_are_exactly_equivalent_to_aligned_ones() {
    // The same signal, fed two ways. If a seam lost or repeated a sample the
    // two outputs would diverge; bit-equality is the strongest statement the
    // plan's "无重复/丢样本" can be given.
    let samples = sine_48k(330.0, 0.4, GRAPH_RATE_HZ as usize);
    let aligned: Vec<Vec<f32>> = samples.chunks(480).map(<[f32]>::to_vec).collect();

    // 137 / 251 / 480 cycled — none of them a divisor of 480 except the last.
    const SEAM_SPLITS: [usize; 3] = [137, 251, 480];
    let mut ragged: Vec<Vec<f32>> = Vec::new();
    let mut position = 0usize;
    let mut split = 0usize;
    while position < samples.len() {
        let take = SEAM_SPLITS[split % SEAM_SPLITS.len()].min(samples.len() - position);
        ragged.push(samples[position..position + take].to_vec());
        position += take;
        split += 1;
    }
    assert!(
        ragged.len() > aligned.len(),
        "the ragged feed is not ragged"
    );

    let mut chain_a = chain_over(aligned, 512);
    let mut chain_b = chain_over(ragged, 512);
    let out_a = chain_a.poll().expect("aligned chain");
    let out_b = chain_b.poll().expect("ragged chain");

    assert_eq!(
        out_a.pcm16, out_b.pcm16,
        "block seams changed the output — a sample was duplicated or dropped"
    );
    // And the total is the exact 1/3 ratio (1 s in, 16 000 out).
    assert_eq!(
        out_a.pcm16.len(),
        16_000,
        "1 s at 48 kHz is 16 000 at 16 kHz"
    );
}

#[test]
fn the_streaming_resampler_keeps_the_ratio_across_a_short_feed() {
    // A feed shorter than one chunk produces nothing yet — and then reappears,
    // rather than being lost. This is the carry-over the chain depends on.
    let mut resampler = StreamingResampler::to_stt().expect("48k -> 16k builds");
    let chunk = resampler.input_chunk_frames();
    assert!(chunk >= 480, "the chunk must hold at least one AEC frame");

    let input: Vec<f32> = (0..chunk).map(|n| (n as f32 / 48_000.0).sin()).collect();
    let first = resampler.process(&input[..chunk / 2]);
    assert!(first.is_empty(), "half a chunk produces nothing yet");
    assert_eq!(resampler.pending_frames(), chunk / 2, "the half is kept");

    let second = resampler.process(&input[chunk / 2..]);
    assert_eq!(
        second.len(),
        resampler.expected_output_len(chunk),
        "the carried half is converted with the rest — nothing is lost"
    );
}

// ---------------------------------------------------------------------------
// T5.2 Test 3 — the rubato 5 API path, and the 4.x API that no longer exists
// ---------------------------------------------------------------------------

#[test]
fn the_resampler_runs_on_rubato_5_and_not_on_the_4x_constructor() {
    let resampler = StreamingResampler::to_stt().expect("to_stt builds");
    assert_eq!(
        (resampler.in_rate(), resampler.out_rate()),
        (48_000, 16_000)
    );

    // The 5.0 shape: the chunk to feed is whatever `input_frames_next()`
    // reported at construction, and it is an exact 3:1 ratio, so one chunk in
    // is one chunk out with no internal buffering.
    let chunk = resampler.input_chunk_frames();
    assert_eq!(
        resampler.expected_output_len(chunk),
        chunk / 3,
        "48k -> 16k is exactly 3:1"
    );

    let from_tts = StreamingResampler::from_tts().expect("from_tts builds");
    assert_eq!((from_tts.in_rate(), from_tts.out_rate()), (24_000, 48_000));
    assert_eq!(
        from_tts.expected_output_len(from_tts.input_chunk_frames()),
        from_tts.input_chunk_frames() * 2,
        "24k -> 48k is exactly 2:1"
    );

    // Research correction 3, pinned: rubato 5 deleted `FftFixedIn`, and this
    // suite would not compile if the implementation used it. The source scan
    // keeps the *name* from creeping back in as a comment-copied snippet.
    let source = include_str!("../src/audio/resample.rs");
    for (number, line) in source.lines().enumerate() {
        if line.contains("FftFixedIn") {
            assert!(
                line.trim_start().starts_with("//"),
                "resample.rs:{} uses the rubato 4.x constructor in code — it does not exist in 5.0: {line}",
                number + 1
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T5.2 Test 4 — the realtime callback discipline
// ---------------------------------------------------------------------------

#[test]
fn a_stalled_consumer_overflows_the_queue_without_blocking_the_callback() {
    // Ten blocks arrive while nobody is draining; the queue holds four.
    let blocks: Vec<Vec<f32>> = (0..10)
        .map(|index| vec![index as f32 / 10.0; 480])
        .collect();
    let source = ScriptedSource::new(GRAPH_RATE_HZ, blocks).with_capacity(4);
    let processor = SharedProcessor::new(AecConfig::disabled()).expect("processor");

    // `ScriptedSource` bursts everything through the real `CaptureSink` on
    // start — i.e. through exactly the code an audio callback runs.
    let mut chain = CaptureChain::start(Box::new(source), processor).expect("chain starts");
    let overflowed = chain.take_source_overflows();
    assert_eq!(
        overflowed, 6,
        "four blocks fit, six were counted as dropped"
    );

    // The callback never blocked: the four that fit are all still playable.
    let output = chain.poll().expect("the chain drains what it kept");
    assert_eq!(output.frames.len(), 4, "the queue kept its capacity");
    assert_eq!(output.pcm16.len(), 4 * 480 / 3);

    // The discipline itself is one implementation, shared with the 02-04
    // enrollment take — the plan's 不要复制粘贴两份.
    let (sink, receiver) = CaptureSink::bounded(1);
    sink.push(&[0.0]);
    sink.push(&[0.0]);
    assert_eq!(sink.overflows(), 1);
    assert_eq!(receiver.try_iter().count(), 1);
}

// ---------------------------------------------------------------------------
// T5.2 Test 5 — the error channel
// ---------------------------------------------------------------------------

#[test]
fn backend_failures_surface_through_the_error_channel_and_never_panic() {
    let source = ScriptedSource::new(GRAPH_RATE_HZ, vec![vec![0.0; 480]])
        .report_error(CaptureError::Stream("device went away".to_string()));
    let processor = SharedProcessor::new(AecConfig::disabled()).expect("processor");
    let mut chain = CaptureChain::start(Box::new(source), processor).expect("chain starts");

    // Errors arrive with the drain, not with a panic on the audio thread.
    chain
        .poll()
        .expect("a reported backend error is not a chain failure");
    let errors = chain.drain_errors();
    assert_eq!(errors.len(), 1, "the backend error was not swallowed");
    assert_eq!(errors[0].code(), "stream");
    assert!(
        !errors[0].message().is_empty(),
        "every error carries the locked Chinese UI copy"
    );

    // Drained means drained — the same failure is reported once.
    assert!(chain.drain_errors().is_empty());

    // A chain that cannot open its device fails at `start`, as a value.
    struct NoDevice;
    impl BlockSource for NoDevice {
        fn sample_rate(&self) -> u32 {
            GRAPH_RATE_HZ
        }
        fn start(&mut self) -> Result<std::sync::mpsc::Receiver<Vec<f32>>, CaptureError> {
            Err(CaptureError::NoInputDevice)
        }
        fn stop(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }
    }
    let refused = CaptureChain::start(
        Box::new(NoDevice),
        SharedProcessor::new(AecConfig::disabled()).expect("processor"),
    );
    assert_eq!(
        refused.expect_err("no device is an error").code(),
        "no_input_device"
    );
}

// ---------------------------------------------------------------------------
// T5.2 Test 6 — the STT handoff
// ---------------------------------------------------------------------------

#[test]
fn the_stt_handoff_satisfies_the_xfyun_frame_contract() {
    // 1 s of capture is 16 000 samples == 32 000 bytes — 25 frames of
    // 1280 bytes (40 ms) exactly.
    let blocks: Vec<Vec<f32>> = sine_48k(220.0, 0.3, GRAPH_RATE_HZ as usize)
        .chunks(480)
        .map(<[f32]>::to_vec)
        .collect();
    let mut chain = chain_over(blocks, 512);
    let output = chain.poll().expect("chain drains");
    let tail = chain.flush();
    assert!(
        tail.is_empty() || tail.len() < 640,
        "the flush is a remainder"
    );

    // The chain hands over PCM16; the *client* frames it. The contract the
    // test pins is the one the vendor's 10163 error depends on.
    let mut pcm16_le = Vec::with_capacity(output.pcm16.len() * 2);
    for sample in &output.pcm16 {
        pcm16_le.extend_from_slice(&sample.to_le_bytes());
    }
    let frames = audio_frames(&pcm16_le);
    assert_eq!(frames[0].len(), AUDIO_FRAME_BYTES);
    assert_eq!(
        AUDIO_FRAME_BYTES,
        640 * 2,
        "40 ms at 16 kHz mono 16-bit is 1280 bytes"
    );
    assert!(
        frames.iter().all(|frame| frame.len() <= AUDIO_FRAME_BYTES),
        "no frame may exceed the 40 ms cap"
    );
    assert!(
        frames.last().is_some_and(|frame| !frame.is_empty()),
        "an empty tail frame would be a wasted round trip"
    );
}

// ---------------------------------------------------------------------------
// T5.2 Test 7 — session lifecycle
// ---------------------------------------------------------------------------

#[test]
fn stopping_a_session_releases_the_queue_and_a_restart_resets_the_counters() {
    let blocks: Vec<Vec<f32>> = vec![vec![0.25; 480]; 8];
    let source = ScriptedSource::new(GRAPH_RATE_HZ, blocks).with_capacity(4);
    let processor = SharedProcessor::new(AecConfig::disabled()).expect("processor");
    let mut chain = CaptureChain::start(Box::new(source), processor).expect("chain starts");
    chain.poll().expect("drain");
    assert!(chain.stats().frames_processed > 0);

    // `stop` drops the receiver: a late audio callback then counts an overflow
    // instead of blocking on a queue nobody reads. That is what "the thread
    // exited" has to mean here — the chain owns no thread of its own, the
    // callback is the device's.
    chain.stop().expect("stop");
    assert!(!chain.is_running());
    assert_eq!(
        chain
            .poll()
            .expect_err("a stopped chain refuses to drain")
            .code(),
        "not_running"
    );
    assert_eq!(
        chain.take_source_overflows(),
        4,
        "the four blocks that never fit are visible as overflows"
    );

    // A restart is a fresh chain: a fresh source, fresh counters, no residue.
    let mut restarted = chain_over(vec![vec![0.25; 480]; 2], 8);
    let stats = restarted.stats();
    assert_eq!(
        (
            stats.blocks_received,
            stats.frames_processed,
            stats.overflows
        ),
        (0, 0, 0),
        "a new session starts at zero"
    );
    assert_eq!(restarted.poll().expect("drain").frames.len(), 2);
    restarted.stop().expect("stop");
}

#[test]
fn the_frame_assembler_never_loses_a_sample_at_a_block_seam() {
    // A device buffer is whatever the hardware likes; 512 is common and is not
    // a multiple of 480.
    let mut assembler = FrameAssembler::for_processor();
    let first = assembler.push(&vec![1.0; 512]);
    assert_eq!(first.len(), 1, "480 of the 512 complete one frame");
    assert_eq!(assembler.pending_samples(), 32, "the remainder is carried");

    let second = assembler.push(&vec![2.0; 448]);
    assert_eq!(second.len(), 1, "32 carried + 448 = one more frame");
    assert_eq!(second[0][32], 2.0, "the seam sample is the new block's");
    assert_eq!(assembler.pending_samples(), 0);

    // Clearing is a session stop: the partial frame belongs to no session.
    assembler.push(&vec![3.0; 100]);
    assembler.clear();
    assert_eq!(assembler.pending_samples(), 0);
}

// ---------------------------------------------------------------------------
// T5.2 — the real device (manual; needs a microphone)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real microphone; run with --ignored on a machine with one"]
fn a_real_microphone_feeds_the_chain() {
    let source = CpalSource::new().expect("a default input device exists");
    let rate = source.sample_rate();
    let processor = SharedProcessor::new(AecConfig::all_enabled()).expect("processor");
    let mut chain = CaptureChain::start(Box::new(source), processor).expect("chain starts");
    assert_eq!(chain.device_rate_hz(), rate);

    std::thread::sleep(std::time::Duration::from_millis(500));
    let output = chain.poll().expect("the device produced something");
    chain.stop().expect("stop");

    assert!(
        !output.frames.is_empty(),
        "half a second of real audio is more than zero frames"
    );
    assert!(output.pcm16.len() > 1_000, "≈8 000 samples at 16 kHz");
}
