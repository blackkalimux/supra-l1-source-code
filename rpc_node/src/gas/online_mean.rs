use super::circular_buf::MaxCircularBuffer;

// Store the gas of past 1000 transactions
const MAX_GAS_HISTORY: usize = 1000;

#[derive(Debug)]
pub struct OnlineMean {
    cur_mean: u64,
    history: MaxCircularBuffer<u64>,
}

impl OnlineMean {
    pub fn new(start_value: u64) -> Self {
        let mut this = Self {
            cur_mean: 0,
            history: MaxCircularBuffer::new(
                MAX_GAS_HISTORY
                    .try_into()
                    .expect("failed to convert MAX_GAS_HISTORY"),
            ),
        };
        // Fill the buffer with the start value.
        this.push(vec![start_value; MAX_GAS_HISTORY]);
        this
    }

    /// Push a new value, updating mean & max
    pub fn push(&mut self, values: Vec<u64>) {
        self.history.push(values);
        let len = self.history.len() as u64;

        // Rounded to nearest integer
        self.cur_mean = (self.history.sum().saturating_add(len / 2)) / len;
    }

    /// Get the max value
    pub fn max(&self) -> u64 {
        self.history.max()
    }

    /// Get the mean value
    pub fn mean(&self) -> u64 {
        self.cur_mean
    }

    /// Get the median value
    pub fn median(&self) -> u64 {
        self.history.median()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_online_mean() {
        let mut mean = OnlineMean::new(101);
        assert_eq!(mean.mean(), 101);
        assert_eq!(mean.max(), 101);
        assert_eq!(mean.median(), 101);

        // Fill the buffer with 0s
        mean.push(vec![0; MAX_GAS_HISTORY]);

        assert_eq!(mean.mean(), 0);
        assert_eq!(mean.max(), 0);
        assert_eq!(mean.median(), 0);

        // Fill the buffer with 1s
        mean.push(vec![1; MAX_GAS_HISTORY]);

        assert_eq!(mean.mean(), 1);
        assert_eq!(mean.max(), 1);
        assert_eq!(mean.median(), 1);

        // Fill the buffer with 100..1
        for i in (0..MAX_GAS_HISTORY).rev() {
            mean.push(vec![i as u64 + 1]);
        }
        // Rounding of 50.5 is 51
        assert_eq!(mean.mean(), 501);
        assert_eq!(mean.max(), 1000);
        assert_eq!(mean.median(), 501);

        // Push 900 and pop 1000
        mean.push(vec![900]);
        assert_eq!(mean.mean(), 500);
        assert_eq!(mean.max(), 999);
        assert_eq!(mean.median(), 501);
    }

    #[test]
    fn test_online_mean_batch_update() {
        let mut mean = OnlineMean::new(100);
        assert_eq!(mean.mean(), 100);
        assert_eq!(mean.max(), 100);
        assert_eq!(mean.median(), 100);

        // Push empty batch
        mean.push(vec![]);
        assert_eq!(mean.mean(), 100);
        assert_eq!(mean.max(), 100);
        assert_eq!(mean.median(), 100);

        // Push one value < max
        mean.push(vec![1]);
        assert_eq!(mean.mean(), 100);
        assert_eq!(mean.max(), 100);
        assert_eq!(mean.median(), 100);

        // Push one value > max
        mean.push(vec![1199]);
        assert_eq!(mean.mean(), 101);
        assert_eq!(mean.max(), 1199);
        assert_eq!(mean.median(), 100);

        // Fill in half of the buffer with 1s
        mean.push(vec![1; MAX_GAS_HISTORY / 2]);

        // (100 * 498 + 1 + 1199 + 1 * 500) / 1000 = 51.5
        assert_eq!(mean.mean(), 52);
        assert_eq!(mean.max(), 1199);
        assert_eq!(mean.median(), 1);

        // Fill in another half of the buffer with 2s
        mean.push(vec![2; MAX_GAS_HISTORY / 2]);
        // (2 * 500 + 1 * 500) / 1000 = 1.5
        assert_eq!(mean.mean(), 2);
        // Max is out of the buffer, so it should be 2
        assert_eq!(mean.max(), 2);
        assert_eq!(mean.median(), 2);

        // Fill in a batch size >= the buffer capacity
        // Input is 0..1000, the buffer should only keep the last 1000 elements 1..1000
        mean.push((0..(MAX_GAS_HISTORY as u64 + 1)).collect());

        // sum of 1..1000 = 500,500 / 1000 = 500.5 -> 501
        assert_eq!(mean.mean(), 501);
        assert_eq!(mean.max(), 1000);
        assert_eq!(mean.median(), 501);
    }
}
