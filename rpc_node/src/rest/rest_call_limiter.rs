use crate::error::Error;
use lru::LruCache;
use move_core_types::account_address::AccountAddress;
use std::num::NonZeroUsize;
use std::time::Instant;
use tracing::debug;
use types::limiting_strategy::{LimitInstance, LimitingStrategy, TLimitingStrategy};

/// REST requests limiter.
/// It caches a value associated with a request and invokes [LimitingStrategy] to decide
/// whether this request should be allowed based on the recently cached value and its submission [Instant].
pub struct RestCallLimiter {
    cache: LruCache<LimitInstance, Instant>,
    limiting_strategy: LimitingStrategy,
}

impl RestCallLimiter {
    const CACHE_SIZE: usize = 10_000;

    pub fn new(limiting_strategy: LimitingStrategy) -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(Self::CACHE_SIZE)
                    .unwrap_or_else(|| panic!("CACHE_SIZE is not non zero")),
            ),
            limiting_strategy,
        }
    }

    /// Registers the provided [AccountAddress] if it's its first occurrence
    /// or replaces old value if [Self::strategy] allows the value.
    /// Return Ok(()) if the value should be allowed and
    /// error if the received hash should be declined.
    pub fn register_account(&mut self, account: AccountAddress) -> Result<(), Error> {
        self.register(LimitInstance::Account(account))
    }

    /// Registers the provided [socrypto::PublicKey] if it's its first occurrence
    /// or replaces old value if [Self::strategy] allows the value.
    /// Return Ok(()) if the value should be allowed and
    /// error if the received hash should be declined.
    pub fn register_public_key(&mut self, public_key: socrypto::PublicKey) -> Result<(), Error> {
        self.register(LimitInstance::PublicKey(public_key))
    }

    /// Registers the provided slice of bytes if it's its first occurrence
    /// or replaces old value if [Self::strategy] allows the value.
    /// Return Ok(()) if the value should be allowed and
    /// error if the received hash should be declined.
    pub fn register_bytes(&mut self, value: &[u8]) -> Result<(), Error> {
        self.register(LimitInstance::Bytes(Vec::from(value)))
    }

    /// Registers the provided [LimitInstance] if it's its first occurrence
    /// or replaces old value if [Self::strategy] allows the value.
    /// Return Ok(()) if the value should be allowed and
    /// error if the received hash should be declined.
    fn register(&mut self, instance: LimitInstance) -> Result<(), Error> {
        let last_seen = self.cache.get(&instance).cloned();
        let first_occurrence = last_seen.is_none();

        if first_occurrence {
            debug!("First occurrence. Caching...");
            self.cache.put(instance.clone(), Instant::now());
        }

        match self.limiting_strategy.should_allow(&instance, last_seen) {
            Ok(()) => {
                if !first_occurrence {
                    debug!("Next allowed occurrence. Updating...");
                    self.cache.put(instance, Instant::now());
                }
                Ok(())
            }
            Err(allow_in) => {
                debug!("Occurrence will be allowed in {allow_in:?}. Ignoring...");
                Err(Error::TooManyRequests(allow_in))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rest::rest_call_limiter::RestCallLimiter;
    use std::time::Duration;
    use supra_logger::LevelFilter;
    use tokio::time;
    use types::limiting_strategy::IntervalStrategy;

    #[tokio::test]
    async fn test_rest_call_limiter() -> anyhow::Result<()> {
        // Initialize logger with `off` level if RUST_LOG is not set.
        let _ = supra_logger::init_default_logger(LevelFilter::OFF);

        let submission_rate = Duration::from_secs(5);
        let limiter_strategy = IntervalStrategy::new(submission_rate)?;

        let mut rest_call_limiter = RestCallLimiter::new(limiter_strategy.into());
        let tx_hash = vec![1u8; 32];

        let res = rest_call_limiter.register_bytes(&tx_hash);
        assert!(res.is_ok());

        // duplicate txn will error
        let res = rest_call_limiter.register_bytes(&tx_hash);
        assert!(res.is_err());

        // duplicate txn after 5 sec will success
        time::sleep(submission_rate).await;
        let res = rest_call_limiter.register_bytes(&tx_hash);
        assert!(res.is_ok());

        Ok(())
    }
}
