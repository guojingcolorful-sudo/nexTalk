//! The realtime-callback discipline (T-02-19), shared by every capture path.
//!
//! A CoreAudio callback runs on a high-priority thread with a hard deadline.
//! Anything that can block — a lock, a mutex, a channel with no capacity, a
//! log write — turns a scheduling hiccup into dropped audio, and a panic turns
//! it into a dead process. So the rule is one line long:
//!
//! > **The callback copies into a bounded queue or counts the block as
//! > dropped. Nothing else.**
//!
//! [`CaptureSink`] is that one line. It was written for the 02-04 enrollment
//! take; 02-05 T5.2's session capture chain uses the same implementation (the
//! plan is explicit: 不要复制粘贴两份), so the discipline can be reviewed in
//! exactly one place.
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

/// The callback's whole job: copy one block into the bounded queue or count it
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

    /// Called from the audio callback. Never blocks: a full queue (the consumer
    /// stalled) and a closed queue (the take/session was stopped) both mean
    /// "drop this block and count it".
    pub fn push(&self, samples: &[f32]) {
        match self.sender.try_send(samples.to_vec()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.overflows.fetch_add(1, Ordering::Relaxed);
            }
        }
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
}
