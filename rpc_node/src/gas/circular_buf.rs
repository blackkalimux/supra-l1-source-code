use num_traits::{SaturatingAdd, SaturatingSub};
use std::num::NonZeroUsize;
/// Simple Fixed Size Circular buffer.
#[derive(Debug)]
pub struct MaxCircularBuffer<T> {
    inner: Vec<T>,
    // Index for next insertion
    index: usize,
    // Index of the current max value
    max_index: usize,
    curr_sum: T,
    capacity: usize,
    median: T,
}

impl<T> MaxCircularBuffer<T>
where
    T: Default + Ord + Copy + SaturatingAdd + SaturatingSub,
{
    pub fn new(capacity: NonZeroUsize) -> Self {
        let inner = vec![T::default(); capacity.get()];
        Self {
            inner,
            max_index: 0,
            index: 0,
            curr_sum: T::default(),
            capacity: capacity.get(),
            median: T::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    fn update_current_sum(&mut self) {
        self.curr_sum = T::default();
        for value in &self.inner {
            self.curr_sum = self.curr_sum.saturating_add(value);
        }
    }

    // Calculate and update the median
    pub fn update_median(&mut self) {
        if self.inner.is_empty() {
            self.median = T::default();
            return;
        }

        // Clone and sort the buffer to calculate the median
        let mut sorted = self.inner.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("failed to compare"));

        // Middle element(chooses the larger index if buffer is of even length)
        self.median = sorted[sorted.len() / 2]
    }

    fn update_max_index(&mut self) {
        self.max_index = self
            .inner
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| *v)
            .map(|(idx, _)| idx)
            .unwrap_or_default();
    }

    pub fn push(&mut self, values: Vec<T>) {
        if values.is_empty() {
            return;
        }
        if values.len() < self.capacity {
            // If max index within the updating range, it means the
            // max value is overwritten by the new values, we need to
            // re-calculate the max index.
            // This allow us to do only one recalculation for a batch of values.
            let updating_max = (self.index <= self.max_index
                && self.max_index < (self.index + values.len()))
                || (self.max_index < self.index
                    && self.max_index < (self.index + values.len()) % self.capacity);
            for val in values {
                self.insert(val);
            }
            // Update the max index if overwritten
            if updating_max {
                self.update_max_index();
            }
        } else {
            // Exceed the capacity, only keep the last `capacity` elements
            let start = values.len() - self.capacity;
            self.inner.clone_from_slice(&values[start..]);
            // Reset to the first element
            self.index = 0;
            self.update_current_sum();
            self.update_max_index();
        }

        self.update_median();
    }

    fn insert(&mut self, val: T) {
        let old_val = self.inner[self.index];
        self.curr_sum = self.curr_sum.saturating_add(&val).saturating_sub(&old_val);
        // Update max but does not consider if the max value is overwritten.
        if val >= self.max() {
            self.max_index = self.index;
        }
        // Insert the new value
        self.inner[self.index] = val;
        // Move index to next position
        self.index = (self.index + 1) % self.capacity;
    }

    /// Find the largest value in the buffer
    pub fn max(&self) -> T {
        self.inner[self.max_index]
    }

    /// Find the sum of all values in the buffer
    pub fn sum(&self) -> T {
        self.curr_sum
    }

    /// Find the median from the values in the buffer
    pub fn median(&self) -> T {
        self.median
    }
}
