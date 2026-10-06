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

// ---------------------------------------------------------------------------
// streaming resampler (02-05 T5.2)
// ---------------------------------------------------------------------------

/// A resampler that survives being fed arbitrary block sizes.
///
/// The 02-04 helper above takes a whole take at once; the live chain cannot —
/// CoreAudio hands over whatever the device buffer happens to be (often 512
/// frames, which is not a multiple of the 480-sample AEC frame). This type
/// carries the remainder across calls, so the output stream is continuous:
/// no duplicated samples, no dropped ones, and the long-run ratio is exactly
/// `out_rate / in_rate`.
///
/// Built on rubato 5's `Fft` with [`FixedSync::Both`], which fixes both the
/// input and the output chunk size — the 48k→16k (3:1) and 24k→48k (2:1)
/// cases are exact ratios, so no internal buffering is needed. **The 4.x
/// `FftFixedIn` constructor does not exist in rubato 5** (research correction
/// 3); the chunk to feed is whatever [`Resampler::input_frames_next`] says.
pub struct StreamingResampler {
    resampler: Fft<f32>,
    /// Input frames carried over from the previous call, shorter than one chunk.
    pending: Vec<f32>,
    /// Scratch for one input chunk (the resampler consumes exactly one).
    chunk_in: Vec<f32>,
    in_rate: u32,
    out_rate: u32,
}

impl StreamingResampler {
    /// Build the converter. `chunk_frames` is a *request*; rubato rounds it to
    /// whatever fits the ratio, and [`Self::input_chunk_frames`] reports the
    /// real value. A rate pair rubato refuses is a programming error, reported
    /// as an `Err` rather than a silent empty stream.
    pub fn new(in_rate: u32, out_rate: u32, chunk_frames: usize) -> Result<Self, ResampleError> {
        if in_rate == 0 || out_rate == 0 {
            return Err(ResampleError::BadRates { in_rate, out_rate });
        }
        if in_rate == out_rate {
            return Err(ResampleError::SameRate { rate: in_rate });
        }
        let resampler = Fft::<f32>::new(
            in_rate as usize,
            out_rate as usize,
            chunk_frames.max(2),
            1,
            FixedSync::Both,
        )
        .map_err(|error| ResampleError::Unsupported {
            in_rate,
            out_rate,
            detail: error.to_string(),
        })?;
        let chunk_in = resampler.input_frames_next();
        Ok(Self {
            resampler,
            pending: Vec::with_capacity(chunk_in * 2),
            chunk_in: vec![0.0; chunk_in],
            in_rate,
            out_rate,
        })
    }

    /// The 48 kHz graph → 16 kHz STT converter (3:1).
    pub fn to_stt() -> Result<Self, ResampleError> {
        Self::new(GRAPH_RATE_HZ, OUTPUT_RATE_HZ, 480)
    }

    /// 火山's 24 kHz synthesised speech → the 48 kHz playout graph (2:1).
    pub fn from_tts() -> Result<Self, ResampleError> {
        Self::new(24_000, GRAPH_RATE_HZ, 240)
    }

    pub fn in_rate(&self) -> u32 {
        self.in_rate
    }

    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    /// Frames the next [`Self::process`] call will consume (rubato's own count
    /// after rounding `chunk_frames` to the ratio).
    pub fn input_chunk_frames(&self) -> usize {
        self.chunk_in.len()
    }

    /// Input frames held over from the previous call.
    pub fn pending_frames(&self) -> usize {
        self.pending.len()
    }

    /// Feed any number of mono `f32` frames; get the resampled ones back.
    ///
    /// Output is produced only in whole chunks, so a short call may return
    /// nothing and the samples reappear on the next call — the carry-over is
    /// what makes arbitrary block sizes seamless.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.pending.extend_from_slice(input);

        let chunk = self.chunk_in.len();
        let chunk_out = self.resampler.output_frames_next();
        let mut out = Vec::with_capacity((self.pending.len() / chunk + 1) * chunk_out);

        while self.pending.len() >= chunk {
            self.chunk_in.copy_from_slice(&self.pending[..chunk]);
            self.pending.drain(..chunk);
            let Ok(adapter) = InterleavedSlice::new(&self.chunk_in, 1, chunk) else {
                // Unreachable: the buffer is exactly one chunk by construction.
                break;
            };
            match self.resampler.process(&adapter, None) {
                Ok(produced) => out.extend_from_slice(&produced.take_data()),
                Err(error) => {
                    // A failed conversion must not silently shorten the stream:
                    // drop the chunk and let the caller's counters see a gap.
                    eprintln!("[resample] streaming conversion failed: {error}");
                    break;
                }
            }
        }
        out
    }

    /// Flush the carry-over: zero-pad the tail to one last chunk and convert it.
    /// Used when a stream ends (session stop) — the padded tail is *not*
    /// trimmed here, because the caller knows the take's expected length; the
    /// live path only needs the samples that were already spoken for.
    pub fn flush(&mut self) -> Vec<f32> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let chunk = self.chunk_in.len();
        let take = self.pending.len().min(chunk);
        let valid_in = take;
        let mut block = vec![0.0; chunk];
        block[..take].copy_from_slice(&self.pending[..take]);
        self.pending.clear();
        let Ok(adapter) = InterleavedSlice::new(&block, 1, chunk) else {
            return Vec::new();
        };
        match self.resampler.process(&adapter, None) {
            Ok(produced) => {
                let data = produced.take_data();
                let valid_out = self.expected_output_len(valid_in).min(data.len());
                data[..valid_out].to_vec()
            }
            Err(error) => {
                eprintln!("[resample] streaming flush failed: {error}");
                Vec::new()
            }
        }
    }

    /// The exact number of output frames `input_frames` will eventually produce
    /// across calls (integer ratio arithmetic — 3:1 keeps every sample).
    pub fn expected_output_len(&self, input_frames: usize) -> usize {
        input_frames * self.out_rate as usize / self.in_rate as usize
    }

    /// For tests and diagnostics: everything still held back, unconverted.
    pub fn pending(&self) -> &[f32] {
        &self.pending
    }
}

/// rubato's `Fft` carries no `Debug`, so this reports the converter's contract
/// (rates, chunk geometry, carry-over) rather than its internal FFT tables.
impl std::fmt::Debug for StreamingResampler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamingResampler")
            .field("in_rate", &self.in_rate)
            .field("out_rate", &self.out_rate)
            .field("chunk_frames", &self.chunk_in.len())
            .field("pending_frames", &self.pending.len())
            .finish()
    }
}

/// Why a resampler could not be built — the caller's rate pair is the problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResampleError {
    BadRates {
        in_rate: u32,
        out_rate: u32,
    },
    SameRate {
        rate: u32,
    },
    Unsupported {
        in_rate: u32,
        out_rate: u32,
        detail: String,
    },
}

impl std::fmt::Display for ResampleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRates { in_rate, out_rate } => {
                write!(formatter, "采样率无效：{in_rate} -> {out_rate}")
            }
            Self::SameRate { rate } => write!(formatter, "{rate} Hz 到 {rate} Hz 无需重采样"),
            Self::Unsupported {
                in_rate,
                out_rate,
                detail,
            } => write!(
                formatter,
                "不支持的重采样比率 {in_rate} -> {out_rate}：{detail}"
            ),
        }
    }
}

impl std::error::Error for ResampleError {}

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

    // -----------------------------------------------------------------------
    // the streaming resampler (02-05 T5.2)
    // -----------------------------------------------------------------------

    #[test]
    fn a_rate_pair_the_converter_cannot_use_is_an_error_not_an_empty_stream() {
        assert_eq!(
            StreamingResampler::new(48_000, 48_000, 480).unwrap_err(),
            ResampleError::SameRate { rate: 48_000 }
        );
        assert_eq!(
            StreamingResampler::new(0, 16_000, 480).unwrap_err(),
            ResampleError::BadRates {
                in_rate: 0,
                out_rate: 16_000,
            }
        );
    }

    #[test]
    fn the_streaming_resampler_feeds_whole_chunks_and_keeps_the_remainder() {
        let mut resampler = StreamingResampler::to_stt().expect("48k -> 16k");
        let chunk = resampler.input_chunk_frames();
        assert_eq!(chunk % 3, 0, "a 3:1 converter needs a chunk divisible by 3");
        assert_eq!(
            resampler.expected_output_len(chunk),
            chunk / 3,
            "one chunk in, exactly one chunk out"
        );

        // A feed that is not a whole number of chunks leaves the remainder for
        // the next call instead of dropping it.
        let input = tone(440.0, 0.1, 48_000, 0.5);
        let first = resampler.process(&input);
        assert_eq!(resampler.pending_frames(), input.len() % chunk);
        let total = first.len() + resampler.flush().len();
        assert!(
            total.abs_diff(resampler.expected_output_len(input.len())) <= chunk / 3,
            "the long-run ratio holds: {total} vs {}",
            resampler.expected_output_len(input.len())
        );
    }

    #[test]
    fn tts_output_is_doubled_not_tripled() {
        // 火山 synthesises at 24 kHz; the playout graph runs at 48 kHz.
        let mut resampler = StreamingResampler::from_tts().expect("24k -> 48k");
        let chunk = resampler.input_chunk_frames();
        assert_eq!(resampler.expected_output_len(chunk), chunk * 2);

        let input = tone(220.0, 0.05, 24_000, 0.5);
        let mut out = resampler.process(&input);
        out.extend(resampler.flush());
        assert!(
            out.len().abs_diff(input.len() * 2) <= chunk,
            "2:1 holds: {} vs {}",
            out.len(),
            input.len() * 2
        );
    }
}
