//! Fixed-capacity sample storage.
//!
//! A live plot is fed faster than it is drawn and must not grow without bound,
//! so samples land in a ring that overwrites the oldest value once it is full.
//! Pushing is O(1) and never allocates after construction.

/// A ring of samples with a fixed capacity.
///
/// Iteration always runs oldest to newest, whatever the write position is.
#[derive(Debug, Clone)]
pub struct SampleBuffer {
    samples: Vec<f64>,
    /// Where the next push goes.
    head: usize,
    /// How many slots hold a real sample; stops growing at capacity.
    filled: usize,
    /// Total pushes since construction. Survives wrap-around, so it doubles as
    /// a sample clock for time-based views.
    written: u64,
}

impl SampleBuffer {
    /// Creates a buffer holding at most `capacity` samples.
    ///
    /// A capacity of zero is allowed and accepts pushes that go nowhere, which
    /// keeps callers from having to special-case an empty channel.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity],
            head: 0,
            filled: 0,
            written: 0,
        }
    }

    /// Adds a sample, dropping the oldest one when full.
    pub fn push(&mut self, value: f64) {
        self.written = self.written.saturating_add(1);
        if self.samples.is_empty() {
            return;
        }
        self.samples[self.head] = value;
        self.head = (self.head + 1) % self.samples.len();
        if self.filled < self.samples.len() {
            self.filled += 1;
        }
    }

    /// Adds many samples in order.
    pub fn extend(&mut self, values: impl IntoIterator<Item = f64>) {
        for value in values {
            self.push(value);
        }
    }

    /// How many samples are held.
    pub fn len(&self) -> usize {
        self.filled
    }

    /// Whether no sample has been kept.
    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    /// The most samples that can be held.
    pub fn capacity(&self) -> usize {
        self.samples.len()
    }

    /// Total pushes since construction, including ones that were overwritten.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Drops every sample. Capacity and the write count are kept.
    pub fn clear(&mut self) {
        self.head = 0;
        self.filled = 0;
    }

    /// The sample `index` places after the oldest one.
    pub fn get(&self, index: usize) -> Option<f64> {
        if index >= self.filled {
            return None;
        }
        // Oldest sits `filled` slots behind the head, modulo the ring.
        let start = (self.head + self.samples.len() - self.filled) % self.samples.len();
        self.samples
            .get((start + index) % self.samples.len())
            .copied()
    }

    /// The newest sample.
    pub fn last(&self) -> Option<f64> {
        self.get(self.filled.checked_sub(1)?)
    }

    /// Samples from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.filled).filter_map(|i| self.get(i))
    }

    /// Smallest and largest sample held, or `None` when empty.
    ///
    /// NaN samples are skipped rather than poisoning the result, because one
    /// bad reading from an instrument should not blank the whole plot.
    pub fn extent(&self) -> Option<(f64, f64)> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for value in self.iter().filter(|v| v.is_finite()) {
            min = min.min(value);
            max = max.max(value);
        }
        if min.is_finite() {
            Some((min, max))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_samples_in_order() {
        let mut buf = SampleBuffer::with_capacity(4);
        buf.extend([1.0, 2.0, 3.0]);

        assert_eq!(buf.len(), 3);
        assert_eq!(buf.iter().collect::<Vec<_>>(), vec![1.0, 2.0, 3.0]);
        assert_eq!(buf.last(), Some(3.0));
    }

    #[test]
    fn overwrites_the_oldest_when_full() {
        let mut buf = SampleBuffer::with_capacity(3);
        buf.extend([1.0, 2.0, 3.0, 4.0, 5.0]);

        assert_eq!(buf.len(), 3);
        assert_eq!(buf.capacity(), 3);
        assert_eq!(buf.iter().collect::<Vec<_>>(), vec![3.0, 4.0, 5.0]);
        assert_eq!(buf.written(), 5, "the write count counts what was dropped");
    }

    #[test]
    fn wraps_many_times_without_losing_order() {
        let mut buf = SampleBuffer::with_capacity(3);
        buf.extend((0..100).map(f64::from));

        assert_eq!(buf.iter().collect::<Vec<_>>(), vec![97.0, 98.0, 99.0]);
    }

    #[test]
    fn a_zero_capacity_buffer_swallows_pushes() {
        let mut buf = SampleBuffer::with_capacity(0);
        buf.push(1.0);

        assert!(buf.is_empty());
        assert_eq!(buf.last(), None);
        assert_eq!(buf.extent(), None);
        assert_eq!(buf.written(), 1);
    }

    #[test]
    fn clear_keeps_capacity() {
        let mut buf = SampleBuffer::with_capacity(4);
        buf.extend([1.0, 2.0]);
        buf.clear();

        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 4);
        assert_eq!(buf.last(), None);
    }

    #[test]
    fn extent_ignores_non_finite_samples() {
        let mut buf = SampleBuffer::with_capacity(8);
        buf.extend([1.0, f64::NAN, -3.0, f64::INFINITY, 2.0]);

        assert_eq!(buf.extent(), Some((-3.0, 2.0)));
    }

    #[test]
    fn extent_of_only_bad_samples_is_none() {
        let mut buf = SampleBuffer::with_capacity(4);
        buf.extend([f64::NAN, f64::NAN]);

        assert_eq!(buf.extent(), None);
    }
}
