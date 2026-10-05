//! T4.1 真实录音采集与校验 — integration suite (02-04).
//!
//! The enrollment take is the one piece of this plan where the microphone and
//! the file system meet: 48 kHz float blocks arrive from a realtime callback,
//! the first 500 ms (device startup click) are dropped, the take is validated
//! (1–3 minutes, ≤ 10 MB, not mostly silence, ≥ 10 s of speech — all with
//! Chinese messages) and lands as a 16 kHz mono PCM16 WAV with owner-only
//! permissions under `<app data>/enroll/<sessionId>.wav`.
//!
//! Everything here runs against [`ScriptedCapture`], the deterministic backend
//! (T4.1 Test 7). The real CoreAudio path exists as [`CpalCapture`] and is only
//! exercised by the `#[ignore]`d manual test at the bottom — a CI box has no
//! microphone, and a test that needs one is a test that flakes.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use nextalk_desktop_lib::audio::resample::downsample_to_16k_mono;
use nextalk_desktop_lib::enroll::capture::{
    finish_capture, CaptureBackend, CaptureError, CaptureGuard, CaptureResult, CaptureSession,
    CaptureSink, CaptureStats, CpalCapture, ScriptedCapture, CAPTURE_QUEUE_BLOCKS,
    ENROLLMENT_SAMPLE_RATE_HZ, MAX_RECORDING_SECS, MAX_SAMPLE_BYTES, MAX_SILENCE_RATIO,
    MIN_RECORDING_SECS, STARTUP_DISCARD_MS,
};
use nextalk_desktop_lib::pipeline::vad::FRAME_SAMPLES;

const RATE: u32 = ENROLLMENT_SAMPLE_RATE_HZ;

// ---------------------------------------------------------------------------
// deterministic audio
// ---------------------------------------------------------------------------

/// The VAD reads AC RMS, so a 440 Hz tone is "speech" (0.3/√2 ≈ 0.21 ≥ 0.02)
/// and zeros are silence. Blocks are 10 ms — exactly one VAD frame.
fn tone_frames(seconds: u64, amplitude: f32) -> Vec<Vec<f32>> {
    let blocks = seconds as usize * 100;
    (0..blocks)
        .map(|block| {
            (0..FRAME_SAMPLES)
                .map(|i| {
                    let n = block * FRAME_SAMPLES + i;
                    let t = n as f32 / RATE as f32;
                    (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
                })
                .collect()
        })
        .collect()
}

fn speech(seconds: u64) -> Vec<Vec<f32>> {
    tone_frames(seconds, 0.3)
}

fn silence(seconds: u64) -> Vec<Vec<f32>> {
    vec![vec![0.0; FRAME_SAMPLES]; seconds as usize * 100]
}

fn concat(mut parts: Vec<Vec<Vec<f32>>>) -> Vec<Vec<f32>> {
    let mut blocks = Vec::new();
    for part in parts.drain(..) {
        blocks.extend(part);
    }
    blocks
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("nextalk-02-04-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// The whole scripted take: start the backend, record, stop, validate, save.
fn run_scripted(
    blocks: Vec<Vec<f32>>,
    root: &Path,
    session_id: &str,
) -> Result<CaptureResult, CaptureError> {
    let mut backend = ScriptedCapture::new(RATE, blocks);
    let session = CaptureSession::begin(&mut backend).expect("the scripted backend starts");
    finish_capture(
        session,
        &mut backend,
        &CaptureGuard::default(),
        root,
        session_id,
    )
}

// ---------------------------------------------------------------------------
// Test 1 — duration validation
// ---------------------------------------------------------------------------

#[test]
fn a_fifteen_second_take_is_rejected_with_the_one_minute_prompt() {
    let root = temp_root("short");
    let error = run_scripted(speech(15), &root, "s15").expect_err("15 s is under the floor");

    match &error {
        CaptureError::TooShort { seconds, min_secs } => {
            assert!((*seconds - 15.0).abs() < 0.5, "{seconds}");
            assert_eq!(*min_secs, MIN_RECORDING_SECS);
        }
        other => panic!("expected TooShort, got {other:?}"),
    }
    let message = error.message();
    assert!(message.contains("录音太短"), "{message}");
    assert!(message.contains("1 分钟"), "{message}");
    assert!(
        !root.join("enroll").join("s15.wav").exists(),
        "a rejected take leaves no artifact"
    );
}

#[test]
fn takes_of_one_to_three_minutes_pass() {
    for seconds in [60u64, 120, 180] {
        assert!(seconds <= MAX_RECORDING_SECS);
        let root = temp_root(&format!("ok-{seconds}"));
        let result = run_scripted(speech(seconds), &root, &format!("s{seconds}"))
            .unwrap_or_else(|error| panic!("{seconds} s must pass: {error:?}"));

        // The reported duration is the *written* audio: the 500 ms startup trim
        // is gone from it (the guard itself counts the captured seconds, so a
        // take stopped at exactly 1:00 is not failed by the trim).
        let expected = seconds as f64 - STARTUP_DISCARD_MS as f64 / 1000.0;
        assert!(
            (result.duration_s - expected).abs() < 0.2,
            "{seconds} s reported as {} s",
            result.duration_s
        );
        assert!(result.bytes > 0);
        assert!(result.path.exists());
    }
}

#[test]
fn a_take_past_three_minutes_is_rejected() {
    // The max is enforced from captured stats — a real 200 s take would only be
    // reachable by a UI bug, which is exactly what this guards.
    let stats = CaptureStats {
        duration_s: (MAX_RECORDING_SECS + 20) as f64,
        bytes: 1_000_000,
        silence_ratio: 0.1,
        speech_secs: 200.0,
    };
    let error = validate(&stats);
    match error {
        CaptureError::TooLong { seconds, max_secs } => {
            assert!(seconds > max_secs as f64);
        }
        other => panic!("expected TooLong, got {other:?}"),
    }
}

fn validate(stats: &CaptureStats) -> CaptureError {
    nextalk_desktop_lib::enroll::capture::validate_capture(stats, &CaptureGuard::default())
        .expect_err("the stats violate a guard")
}

// ---------------------------------------------------------------------------
// Test 2 — the 10 MB hard cap (voice_clone's upload limit)
// ---------------------------------------------------------------------------

#[test]
fn a_sample_over_ten_megabytes_is_refused() {
    let base = CaptureStats {
        duration_s: 100.0,
        bytes: MAX_SAMPLE_BYTES + 1,
        silence_ratio: 0.1,
        speech_secs: 90.0,
    };
    match validate(&base) {
        CaptureError::TooLarge { bytes, max_bytes } => {
            assert_eq!(bytes, MAX_SAMPLE_BYTES + 1);
            assert_eq!(max_bytes, MAX_SAMPLE_BYTES);
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }

    // Exactly at the cap passes the size check (the check is `>`, not `>=`).
    let at_cap = CaptureStats {
        bytes: MAX_SAMPLE_BYTES,
        ..base
    };
    assert!(
        nextalk_desktop_lib::enroll::capture::validate_capture(&at_cap, &CaptureGuard::default())
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// Test 3 — silence validation
// ---------------------------------------------------------------------------

#[test]
fn a_completely_silent_take_names_the_microphone() {
    let root = temp_root("silent");
    let error = run_scripted(silence(60), &root, "silent").expect_err("nothing was recorded");

    match &error {
        CaptureError::TooSilent { silence_ratio } => {
            assert!(*silence_ratio > MAX_SILENCE_RATIO, "{silence_ratio}");
        }
        other => panic!("expected TooSilent, got {other:?}"),
    }
    let message = error.message();
    assert!(message.contains("没有录到声音"), "{message}");
    assert!(message.contains("请检查麦克风"), "{message}");
}

#[test]
fn more_than_sixty_percent_silence_is_rejected_and_the_boundary_passes() {
    // 20 s speech + 40 s silence = 66.7 % silence → refused.
    let root = temp_root("mostly-silent");
    let error = run_scripted(
        concat(vec![speech(20), silence(40)]),
        &root,
        "mostly-silent",
    )
    .expect_err("two thirds of the take were quiet");
    assert!(matches!(error, CaptureError::TooSilent { .. }), "{error:?}");

    // Exactly 60 % silence is *not* "more than 60 %" → inside the guard.
    let stats = CaptureStats {
        duration_s: 100.0,
        bytes: 1_000_000,
        silence_ratio: MAX_SILENCE_RATIO,
        speech_secs: 40.0,
    };
    assert!(
        nextalk_desktop_lib::enroll::capture::validate_capture(&stats, &CaptureGuard::default())
            .is_ok(),
        "the boundary itself is accepted"
    );
    let over = CaptureStats {
        silence_ratio: 0.61,
        speech_secs: 39.0,
        ..stats
    };
    assert!(matches!(validate(&over), CaptureError::TooSilent { .. }));
}

#[test]
fn speech_below_the_minimum_is_rejected() {
    // Unreachable with the default guard (≥ 40 % of a 60 s take is already
    // above the 10 s floor), so the check is exercised with the guard a future
    // tuning pass would produce.
    let guard = CaptureGuard {
        min_speech_secs: 30,
        ..CaptureGuard::default()
    };
    let stats = CaptureStats {
        duration_s: 70.0,
        bytes: 1_000_000,
        silence_ratio: 0.5,
        speech_secs: 25.0,
    };
    let error = nextalk_desktop_lib::enroll::capture::validate_capture(&stats, &guard)
        .expect_err("25 s of speech is under a 30 s floor");
    assert!(matches!(error, CaptureError::TooLittleSpeech { .. }), "{error:?}");
    assert!(error.message().contains("有效朗读不足"), "{}", error.message());
}

// ---------------------------------------------------------------------------
// Test 4 — resampling correctness (48 k → 16 k)
// ---------------------------------------------------------------------------

#[test]
fn resampling_keeps_the_tone_at_a_third_of_the_length_without_clipping() {
    let seconds = 2usize;
    let input: Vec<f32> = (0..RATE as usize * seconds)
        .map(|n| {
            let t = n as f32 / RATE as f32;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5
        })
        .collect();

    let out = downsample_to_16k_mono(&input, RATE);
    assert_eq!(out.len(), input.len() / 3, "16 kHz is a third of 48 kHz");

    // No clipping: a 0.5-amplitude input must stay far from the i16 rail, so
    // the clamp never had to engage.
    let peak = out.iter().map(|s| s.abs()).max().expect("samples");
    assert!(
        peak < (0.6 * i16::MAX as f32) as i16,
        "peak {peak} suggests clipping"
    );

    // Frequency unchanged: count zero crossings over the steady middle 80 %.
    let mid = &out[out.len() / 10..out.len() * 9 / 10];
    let mut crossings = 0u32;
    for pair in mid.windows(2) {
        if (pair[0] >= 0) != (pair[1] >= 0) {
            crossings += 1;
        }
    }
    let measured_hz = crossings as f32 * 16_000.0 / (2.0 * mid.len() as f32);
    assert!(
        (measured_hz - 440.0).abs() < 440.0 * 0.02,
        "measured {measured_hz} Hz"
    );
}

#[test]
fn resampling_an_empty_or_identical_rate_slice_is_still_mono_pcm16() {
    assert!(downsample_to_16k_mono(&[], RATE).is_empty());
    assert!(downsample_to_16k_mono(&[0.0; 3], 0).is_empty(), "rate 0");
}

// ---------------------------------------------------------------------------
// Test 5 — realtime callback discipline
// ---------------------------------------------------------------------------

#[test]
fn the_callback_drops_instead_of_blocking_when_the_consumer_stalls() {
    let overflows = Arc::new(AtomicU64::new(0));
    let (sender, receiver) = sync_channel(CAPTURE_QUEUE_BLOCKS);
    let sink = CaptureSink::new(sender, Arc::clone(&overflows));

    // The consumer never reads while we push: the queue fills, and every push
    // past capacity must count an overflow instead of blocking the "callback".
    for _ in 0..CAPTURE_QUEUE_BLOCKS + 7 {
        sink.push(&[0.3; FRAME_SAMPLES]);
    }
    assert_eq!(sink.overflows(), 7, "exactly the blocks that did not fit");

    let queued: Vec<_> = receiver.try_iter().collect();
    assert_eq!(queued.len(), CAPTURE_QUEUE_BLOCKS);

    // A closed consumer (the take was cancelled) is not a panic either.
    drop(receiver);
    sink.push(&[0.3; FRAME_SAMPLES]);
    assert_eq!(sink.overflows(), 8);
}

// ---------------------------------------------------------------------------
// Test 6 — the 500 ms startup trim
// ---------------------------------------------------------------------------

#[test]
fn the_first_500ms_of_device_noise_are_dropped_without_shifting_the_take() {
    // The startup click: half a second at a much higher level than the speech
    // that follows, then a quiet gap, then speech at t = 9.5 s of the take.
    let click = tone_frames(1, 0.9) // 1 s of loud tone, only its first 500 ms matter
        .into_iter()
        .take(50)
        .collect::<Vec<_>>();
    let blocks = concat(vec![click, silence(9), speech(51)]);
    let root = temp_root("trim");
    let result = run_scripted(blocks, &root, "trim").expect("the take is valid");

    let mut reader = hound::WavReader::open(&result.path).expect("wav opens");
    assert_eq!(reader.spec().sample_rate, 16_000);
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.spec().bits_per_sample, 16);
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / 32_768.0)
        .collect();

    let rms = |from_s: f64, to_s: f64| -> f32 {
        let from = (from_s * 16_000.0) as usize;
        let to = ((to_s * 16_000.0) as usize).min(samples.len());
        let slice = &samples[from..to];
        (slice.iter().map(|s| s * s).sum::<f32>() / slice.len() as f32).sqrt()
    };

    // The click is gone: the loudest part of the first second sits far below
    // the 0.9 it was recorded at.
    assert!(rms(0.0, 1.0) < 0.1, "startup click survived: {}", rms(0.0, 1.0));
    // The gap is still silent and the speech still starts where it was spoken
    // (9 s after the trim), so the take was not shifted earlier.
    assert!(rms(1.0, 8.5) < 0.01, "gap was shifted: {}", rms(1.0, 8.5));
    assert!(rms(10.5, 11.5) > 0.1, "speech moved: {}", rms(10.5, 11.5));
}

// ---------------------------------------------------------------------------
// Test 7 — the backend is replaceable; the real one is manual
// ---------------------------------------------------------------------------

#[test]
fn the_scripted_backend_produces_byte_identical_takes() {
    let first_root = temp_root("determinism-a");
    let second_root = temp_root("determinism-b");
    let script = || concat(vec![speech(60), silence(3)]);

    let first = run_scripted(script(), &first_root, "same").expect("first");
    let second = run_scripted(script(), &second_root, "same").expect("second");
    assert_eq!(
        std::fs::read(&first.path).expect("first bytes"),
        std::fs::read(&second.path).expect("second bytes"),
        "the scripted backend is deterministic"
    );
}

#[test]
#[ignore = "needs a real microphone + TCC permission; run manually with --ignored"]
fn a_real_microphone_take_records_and_saves_a_wav() {
    let root = temp_root("live-mic");
    let mut backend = CpalCapture::new().expect("an input device");
    let rate = backend.sample_rate();
    assert!(rate > 0);

    let session = CaptureSession::begin(&mut backend).expect("the stream starts");
    std::thread::sleep(std::time::Duration::from_millis(700));

    // A short manual take is valid under a one-second-lower guard: the point is
    // that the real CoreAudio path records and lands a file, not that a human
    // read a paragraph.
    let guard = CaptureGuard {
        min_secs: 0,
        min_speech_secs: 0,
        ..CaptureGuard::default()
    };
    let result = finish_capture(session, &mut backend, &guard, &root, "live-mic")
        .expect("the take is saved");
    assert!(result.bytes > 44, "a WAV with a header and samples");
    assert!(result.path.exists());
}

// ---------------------------------------------------------------------------
// Test 8 — the file lands under enroll/ with owner-only permissions
// ---------------------------------------------------------------------------

#[test]
fn the_sample_lands_under_enroll_with_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_root("perms");
    let result = run_scripted(speech(60), &root, "take-1").expect("the take is valid");

    assert_eq!(result.path, root.join("enroll").join("take-1.wav"));
    let mode = std::fs::metadata(&result.path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the sample is biometric data");

    let bytes = std::fs::read(&result.path).expect("bytes");
    let text = String::from_utf8_lossy(&bytes);
    for needle in ["VOLC", "X-Api-Key", "sk-"] {
        assert!(!text.contains(needle), "{needle} leaked into the sample");
    }
}
