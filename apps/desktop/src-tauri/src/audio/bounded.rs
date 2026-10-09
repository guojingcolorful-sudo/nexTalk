//! The realtime-callback discipline (T-02-19), shared by every capture path.
//!
//! A CoreAudio callback runs on a high-priority thread with a hard deadline.
//! Anything that can block — a lock, a mutex, a channel with no capacity, a
//! log write — turns a scheduling hiccup into dropped audio, and a panic turns
//! it into a dead process. So the rule is:
//!
//! > **The callback hands one block to the bounded queue or counts it as
//! > dropped. Nothing else: no logging, no blocking lock, no I/O, no growth
//! > the caller did not ask for.**
//!
//! One allocation per block is accepted **explicitly** (WR-09), not by
//! oversight: the queue's element is a `Vec<f32>` (a 480-sample block is
//! ~1.9 KB) and the element has to exist before the consumer can own it. Two
//! entry points make the cost visible instead of hiding it:
//!
//! - [`CaptureSink::push_owned`] — a callback that already built a buffer
//!   (a downmix, an i16 → f32 conversion) **moves** it in: zero copies, zero
//!   allocations beyond the buffer it had to build anyway;
//! - [`CaptureSink::push`] — a borrowed `&[f32]` device buffer costs exactly
//!   one copy into one fresh `Vec`, and that is the only allocation the
//!   contract permits.
//!
//! A buffer-recycling pool (a return channel the consumer feeds empty buffers
//! back through) would remove the last allocation on the borrowed path. It is
//! deliberately not built yet: it adds a return obligation to every consumer
//! of the chain, and the copy it saves is a small memcpy against a 10 ms
//! deadline. If a measured callback overrun ever puts the allocator in the
//! profile, that pool is the next step — not a second undocumented
//! allocation.
//!
//! [`CaptureSink`] is this discipline in one place. It was written for the
//! 02-04 enrollment take; 02-05 T5.2's session capture chain uses the same
//! implementation (the plan is explicit: 不要复制粘贴两份), so the discipline
//! can be reviewed in exactly one place. The same shape covers the device
//! error callback in [`crate::audio::capture`] — bounded channel, `try_send`,
//! drops counted.
//!
//! Overflows are *counted*, never silenced: a rising [`CaptureSink::overflows`]
//! is the diagnostic that says the consumer stalled, and it is surfaced to the
//! session rather than hidden in a log the audio thread cannot afford to write.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

/// Blocks a realtime callback may queue before it starts counting drops.
/// 256 × 480 samples ≈ 2.6 s at 48 kHz — far above normal scheduling jitter,
/// far below "the consumer is dead and we are leaking memory".
pub const DEFAULT_QUEUE_BLOCKS: usize = 256;

/// The callback's whole job: hand one block to the bounded queue or count it
/// as dropped. Cloneable — one clone goes to the audio thread, one stays with
/// the owner that reads [`Self::overflows`].
#[derive(Clone)]
pub struct CaptureSink {
    sender: SyncSender<Vec<f32>>,
    overflows: Arc<AtomicU64>,
}

impl CaptureSink {
    /// Wrap an existing bounded channel. The receiver belongs to the consumer
    /// (the drain worker / the capture chain's `poll`).
    pub fn new(sender: SyncSender<Vec<f32>>, overflows: Arc<AtomicU64>) -> Self {
        Self { sender, overflows }
    }

    /// Build the pair in one call — the shape both call sites actually want.
    pub fn bounded(capacity_blocks: usize) -> (Self, Receiver<Vec<f32>>) {
        let (sender, receiver) = sync_channel::<Vec<f32>>(capacity_blocks);
        (Self::new(sender, Arc::new(AtomicU64::new(0))), receiver)
    }

    /// Called from the audio callback with a buffer the callback owns: the
    /// block is **moved** in (WR-09), so a callback that had to build its own
    /// buffer — a multi-channel downmix, an i16 → f32 conversion — pays no
    /// second copy on the audio thread.
    ///
    /// Never blocks: a full queue (the consumer stalled) and a closed queue
    /// (the take/session was stopped) both mean "drop this block and count
    /// it".
    pub fn push_owned(&self, samples: Vec<f32>) {
        match self.sender.try_send(samples) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.overflows.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Called from the audio callback with a borrowed device buffer: exactly
    /// one copy into a fresh `Vec` (the one allocation the contract allows —
    /// see the module docs), then [`Self::push_owned`]'s path.
    pub fn push(&self, samples: &[f32]) {
        self.push_owned(samples.to_vec());
    }

    /// Blocks dropped by the callback discipline.
    pub fn overflows(&self) -> u64 {
        self.overflows.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_queue_counts_drops_and_never_blocks() {
        let (sink, receiver) = CaptureSink::bounded(2);
        for _ in 0..5 {
            sink.push(&[0.1, 0.2]);
        }
        assert_eq!(sink.overflows(), 3, "three blocks past the bound");
        assert_eq!(
            receiver.try_iter().count(),
            2,
            "the queue kept its capacity"
        );
    }

    #[test]
    fn a_closed_queue_counts_drops_instead_of_erroring() {
        let (sink, receiver) = CaptureSink::bounded(4);
        sink.push(&[1.0]);
        drop(receiver);
        sink.push(&[1.0]);
        assert_eq!(
            sink.overflows(),
            1,
            "a stopped session counts, never errors"
        );
    }

    /// WR-09: the move-based entry point is the same discipline — same queue,
    /// same shared drop counter, same never-blocks behaviour. The callback that
    /// already owns a buffer must not be forced through a second copy.
    #[test]
    fn push_owned_shares_the_queue_and_the_drop_counter() {
        let (sink, receiver) = CaptureSink::bounded(2);
        // A slice callback and a buffer-owning callback, interleaved.
        sink.push(&[0.1, 0.2]);
        sink.push_owned(vec![0.3, 0.4]);
        // Both landed in one queue, in order, as their own blocks.
        assert_eq!(
            receiver.try_iter().collect::<Vec<_>>(),
            vec![vec![0.1, 0.2], vec![0.3, 0.4]]
        );

        // The counter is shared: a drop through either entry point is one fact.
        sink.push_owned(vec![0.5]);
        sink.push_owned(vec![0.6]);
        sink.push(&[0.7]);
        assert_eq!(sink.overflows(), 1, "bound is 2; three blocks arrived");
    }
}
