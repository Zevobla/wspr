//! Preroll ring buffer for audio capture.
//!
//! This module prevents losing the first samples of an audio capture by retaining
//! a ring buffer of the most recent audio samples from BEFORE the capture trigger.
//! When capture begins, these pre-trigger samples are prepended to the captured audio,
//! ensuring the first word or sound is never truncated. This is essential for
//! voice-dictation workflows where users press a hotkey and speak immediately.

use std::collections::VecDeque;

/// Default preroll duration in milliseconds (~300ms).
/// At 16kHz, this retains ~4800 samples before the trigger.
pub const DEFAULT_PREROLL_MS: usize = 300;

/// A fixed-capacity ring buffer that retains the most recent audio samples.
///
/// This buffer is used to store samples from BEFORE a capture is triggered,
/// so that when recording begins, those pre-trigger samples can be prepended
/// to the captured audio. This prevents losing the initial part of an utterance
/// when a user presses the hotkey and speaks immediately (criterion E-10).
///
/// The buffer maintains an invariant: it never holds more than `capacity` samples.
/// When a new sample would exceed capacity, the oldest sample is dropped.
#[derive(Debug, Clone)]
pub struct PrerollBuffer {
    /// Maximum number of samples this buffer can hold
    capacity: usize,
    /// Ring buffer storing the most recent `capacity` samples (or fewer)
    samples: VecDeque<f32>,
}

impl PrerollBuffer {
    /// Creates a new preroll buffer with the given capacity (in samples).
    ///
    /// # Arguments
    /// * `preroll_samples` - the maximum number of samples to retain
    ///
    /// # Example
    /// ```
    /// use whspr_audio::PrerollBuffer;
    /// let buf = PrerollBuffer::new(4800); // ~300ms at 16kHz
    /// ```
    pub fn new(preroll_samples: usize) -> Self {
        PrerollBuffer {
            capacity: preroll_samples,
            samples: VecDeque::with_capacity(preroll_samples),
        }
    }

    /// Creates a new preroll buffer from a duration in milliseconds and sample rate.
    ///
    /// Computes `samples = (ms * sample_rate) / 1000` and creates a buffer
    /// with that capacity.
    ///
    /// # Arguments
    /// * `ms` - preroll duration in milliseconds
    /// * `sample_rate` - sample rate in Hz (e.g., 16000)
    ///
    /// # Example
    /// ```
    /// use whspr_audio::PrerollBuffer;
    /// let buf = PrerollBuffer::from_ms(300, 16000); // 300ms at 16kHz = 4800 samples
    /// ```
    pub fn from_ms(ms: usize, sample_rate: u32) -> Self {
        let preroll_samples = (ms as u32 * sample_rate) / 1000;
        PrerollBuffer::new(preroll_samples as usize)
    }

    /// Appends a single sample to the buffer, maintaining the fixed capacity.
    ///
    /// If the buffer is at capacity, the oldest sample is dropped.
    ///
    /// # Arguments
    /// * `sample` - the audio sample to append
    pub fn push(&mut self, sample: f32) {
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Appends multiple samples to the buffer, maintaining the fixed capacity.
    ///
    /// Samples are added one by one; if this causes the buffer to exceed capacity,
    /// the oldest samples are dropped to maintain the invariant.
    ///
    /// # Arguments
    /// * `slice` - audio samples to append
    pub fn push_slice(&mut self, slice: &[f32]) {
        for &sample in slice {
            self.push(sample);
        }
    }

    /// Drains the preroll buffer and returns all retained samples.
    ///
    /// Returns samples in chronological order (oldest first), suitable for
    /// prepending to newly captured audio. After this call, the buffer is empty.
    ///
    /// This is typically called when the user presses the hotkey to start recording,
    /// allowing the first word to be captured in full (criterion E-10).
    ///
    /// # Example
    /// ```
    /// use whspr_audio::PrerollBuffer;
    /// let mut buf = PrerollBuffer::new(100);
    /// buf.push_slice(&[0.1, 0.2, 0.3, 0.4, 0.5]);
    /// let preroll = buf.drain_preroll();
    /// assert_eq!(preroll, vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    /// assert!(buf.is_empty());
    /// ```
    pub fn drain_preroll(&mut self) -> Vec<f32> {
        self.samples.drain(..).collect()
    }

    /// Returns the number of samples currently in the buffer.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns `true` if the buffer contains no samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preroll_from_ms_computation() {
        // 300ms at 16000 Hz = (300 * 16000) / 1000 = 4800 samples
        let buf = PrerollBuffer::from_ms(300, 16000);
        assert_eq!(buf.capacity, 4800, "300ms at 16kHz should be 4800 samples");

        // 100ms at 16000 Hz = (100 * 16000) / 1000 = 1600 samples
        let buf = PrerollBuffer::from_ms(100, 16000);
        assert_eq!(buf.capacity, 1600, "100ms at 16kHz should be 1600 samples");

        // 50ms at 16000 Hz = (50 * 16000) / 1000 = 800 samples
        let buf = PrerollBuffer::from_ms(50, 16000);
        assert_eq!(buf.capacity, 800, "50ms at 16kHz should be 800 samples");
    }

    #[test]
    fn test_preroll_ring_eviction() {
        // Create a small buffer with capacity 5
        let mut buf = PrerollBuffer::new(5);

        // Push 7 samples; only the last 5 should remain
        buf.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);

        assert_eq!(buf.len(), 5, "buffer should retain exactly capacity samples");

        // Drain and verify we kept the last 5 (oldest first)
        let drained = buf.drain_preroll();
        assert_eq!(drained, vec![3.0, 4.0, 5.0, 6.0, 7.0],
                   "should keep the last 5 samples (3-7)");
    }

    #[test]
    fn test_preroll_drain_empties_buffer() {
        let mut buf = PrerollBuffer::new(10);
        buf.push_slice(&[0.1, 0.2, 0.3, 0.4, 0.5]);

        assert_eq!(buf.len(), 5, "buffer should have 5 samples");

        let drained = buf.drain_preroll();
        assert_eq!(drained.len(), 5, "drained should return 5 samples");
        assert!(buf.is_empty(), "buffer should be empty after drain");
        assert_eq!(buf.len(), 0, "len() should be 0 after drain");
    }

    #[test]
    fn test_preroll_empty_drain() {
        let mut buf = PrerollBuffer::new(100);

        // Drain from an empty buffer
        let drained = buf.drain_preroll();
        assert_eq!(drained, Vec::<f32>::new(), "draining empty buffer should yield empty vec");
        assert!(buf.is_empty(), "buffer should still be empty");
    }

    #[test]
    fn test_preroll_single_push() {
        let mut buf = PrerollBuffer::new(10);

        buf.push(0.5);
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());

        let drained = buf.drain_preroll();
        assert_eq!(drained, vec![0.5]);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_preroll_push_then_push_slice() {
        let mut buf = PrerollBuffer::new(10);

        buf.push(1.0);
        buf.push_slice(&[2.0, 3.0, 4.0]);
        buf.push(5.0);

        assert_eq!(buf.len(), 5);

        let drained = buf.drain_preroll();
        assert_eq!(drained, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_preroll_gradual_eviction() {
        // Test that eviction works gradually as we push beyond capacity
        let mut buf = PrerollBuffer::new(3);

        buf.push(1.0);
        assert_eq!(buf.len(), 1);

        buf.push(2.0);
        assert_eq!(buf.len(), 2);

        buf.push(3.0);
        assert_eq!(buf.len(), 3);

        // Adding a 4th sample should evict the 1st
        buf.push(4.0);
        assert_eq!(buf.len(), 3);
        let drained = buf.drain_preroll();
        assert_eq!(drained, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_preroll_chronological_order() {
        // Verify that drained samples are in chronological order (oldest first)
        let mut buf = PrerollBuffer::new(100);

        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        buf.push_slice(&samples);

        let drained = buf.drain_preroll();
        assert_eq!(drained, samples, "samples should be in chronological order");
    }
}
