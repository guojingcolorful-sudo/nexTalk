//! Sample-rate conversion at the provider boundary (02-04 T4.1).
//!
//! Every sample feed in the system wants a different rate: the local audio
//! graph (and the VAD's frame geometry) runs at 48 kHz, 火山's voice_clone
//! training and the STT feeds want 16 kHz mono. This module is the single
//! conversion point, built on rubato 5's FFT resampler.
//!
//! rubato 5.0 is an API rewrite — `FftFixedIn` no longer exists. The shape used
//! here: [`Fft::new`] with [`FixedSync::Both`] fixes the chunk size on both
//! sides, [`Resampler::process`] consumes one block wrapped in an
//! [`InterleavedSlice`] and returns owned interleaved samples, and the final
//! partial block is zero-padded then the output truncated to the arithmetic
//! length (`in_len × out_rate / in_rate`) so callers can assert lengths.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

/// The rate every speech feed expects: 16 kHz mono (voice_clone, STT).
pub const OUTPUT_RATE_HZ: u32 = 16_000;
/// The local audio graph's rate — the VAD's frame geometry assumes it
/// ([`crate::pipeline::vad::SAMPLE_RATE_HZ`]).
pub const GRAPH_RATE_HZ: u32 = 48_000;
/// Requested frames per processing chunk; the actual fixed size is whatever
/// [`Resampler::input_frames_next`] reports after construction.
pub const RESAMPLE_CHUNK_FRAMES: usize = 1024;

/// Resample one mono `f32` stream. Infallible by design — the rates are
/// provider constants, so a construction failure is a programming error and is
/// reported loudly rather than silently swallowed... if it ever happens the
/// empty result makes the caller's length assertion fail.
pub fn resample_mono(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if input.is_empty() || in_rate == 0 || out_rate == 0 {
        return Vec::new();
    }
    if in_rate == out_rate {
        return input.to_vec();
    }

    let target_len = input.len() * out_rate as usize / in_rate as usize;
    if target_len == 0 {
        return Vec::new();
    }

    let mut resampler = match Fft::<f32>::new(
        in_rate as usize,
        out_rate as usize,
        RESAMPLE_CHUNK_FRAMES,
        1,
        FixedSync::Both,
    ) {
        Ok(resampler) => resampler,
        Err(error) => {
            eprintln!("[resample] {in_rate} -> {out_rate} Hz is not a usable pair: {error}");
            return Vec::new();
        }
    };

    let chunk_in = resampler.input_frames_next();
    let chunk_out = resampler.output_frames_next();
    let mut out: Vec<f32> = Vec::with_capacity(target_len + chunk_out);
    let mut block: Vec<f32> = vec![0.0; chunk_in];
    let mut position = 0usize;

    while position < input.len() {
        let take = (input.len() - position).min(chunk_in);
        block[..take].copy_from_slice(&input[position..position + take]);
        // Zero-pad the final partial block; the padded tail is truncated below.
        block[take..].fill(0.0);

        let Ok(adapter) = InterleavedSlice::new(&block, 1, chunk_in) else {
            eprintln!("[resample] chunk buffer rejected by the adapter");
            return Vec::new();
        };
        match resampler.process(&adapter, None) {
            Ok(produced) => out.extend_from_slice(&produced.take_data()),
            Err(error) => {
                eprintln!("[resample] processing failed: {error}");
                return Vec::new();
            }
        }
        position += take;
    }

    out.truncate(target_len);
    out
}

/// Resample one mono `f32` stream straight to signed 16-bit PCM.
pub fn resample_mono_to_pcm16(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<i16> {
    f32_to_pcm16(&resample_mono(input, in_rate, out_rate))
}

/// The enrollment path: 48 kHz capture → 16 kHz mono PCM16 (the voice_clone
/// input format).
pub fn downsample_to_16k_mono(input: &[f32], in_rate: u32) -> Vec<i16> {
    resample_mono_to_pcm16(input, in_rate, OUTPUT_RATE_HZ)
}

/// `f32` [-1, 1] → PCM16. Clamped, then rounded: the clamp is the last line of
/// defence, so no sample ever wraps around the i16 rail.
pub fn f32_to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
        .collect()
}

/// PCM16 → `f32`[-1, 1]. Divides by 32 768 so the mapping is exactly inverse
/// for the positive rail the playback writer produces from [`f32_to_pcm16`].
pub fn pcm16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|sample| *sample as f32 / 32_768.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, seconds: f32, rate: u32, amplitude: f32) -> Vec<f32> {
        let len = (seconds * rate as f32) as usize;
        (0..len)
            .map(|n| {
                let t = n as f32 / rate as f32;
                (2.0 * std::f32::consts::PI * hz * t).sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn identical_rates_pass_through_untouched() {
        let input = tone(440.0, 0.1, 48_000, 0.5);
        assert_eq!(resample_mono(&input, 48_000, 48_000), input);
    }

    #[test]
    fn zero_rates_or_empty_input_produce_nothing() {
        assert!(resample_mono(&[], 48_000, 16_000).is_empty());
        assert!(resample_mono(&[0.1, 0.2], 0, 16_000).is_empty());
        assert!(resample_mono(&[0.1, 0.2], 48_000, 0).is_empty());
        // Shorter than a single output frame.
        assert!(downsample_to_16k_mono(&[0.1, 0.2], 48_000).is_empty());
    }

    #[test]
    fn pcm16_round_trip_keeps_the_positive_rail() {
        let pcm = f32_to_pcm16(&[0.0, 1.0, -1.0, 2.0, -2.0]);
        assert_eq!(pcm[0], 0);
        assert_eq!(pcm[1], i16::MAX);
        assert_eq!(pcm[2], -i16::MAX, "clamped, never wrapped");
        assert_eq!(pcm[3], i16::MAX, "the clamp holds above 1.0");
        assert_eq!(pcm[4], -i16::MAX);
        assert!((pcm16_to_f32(&pcm)[1] - 1.0).abs() < 0.0001);
    }

    #[test]
    fn forty_eight_to_sixteen_thousand_keeps_the_tone() {
        let input = tone(1_000.0, 1.0, 48_000, 0.25);
        let out = downsample_to_16k_mono(&input, 48_000);
        assert_eq!(out.len(), 16_000);
        let peak = out.iter().map(|s| s.abs()).max().unwrap();
        assert!(peak as f32 > 0.2 * i16::MAX as f32, "tone survived: {peak}");
    }
}
