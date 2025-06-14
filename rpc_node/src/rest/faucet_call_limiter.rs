use dashmap::DashMap;
use move_core_types::account_address::AccountAddress;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, error, info};
use types::settings::constants::{
    DEFAULT_COOL_DOWN_PERIOD_FOR_FAUCET_IN_SECONDS, DEFAULT_MAX_DAILY_QUANTS_PER_ACCOUNT,
    SECONDS_IN_A_DAY,
};

pub struct FaucetCallLimiter {
    /// This map needs to be checked and cleared every day.
    cache: DashMap<AccountAddress, FaucetLimiterValue>,
    /// Maximum request per user per day.
    max_daily_request: u64,
    /// Cooldown period in seconds.
    cooldown_period_secs: u64,
    /// Last time when the cache was cleared in seconds since [UNIX_EPOCH].
    last_reset_secs: u64,
}

#[derive(Default)]
pub struct FaucetLimiterValue {
    /// Count of api call by a single user account.
    count_of_usage: u64,
    /// Last time when the user accessed the api in seconds since [UNIX_EPOCH].
    last_used_time: u64,
}

impl FaucetCallLimiter {
    pub fn new(max_daily_request: u64, cooldown_period_secs: u64) -> Self {
        let now = Self::current_time_in_secs();
        info!("Limit Faucet daily({now}) with {max_daily_request} maximum requests per user and cooldown period of {cooldown_period_secs} secs.");
        FaucetCallLimiter {
            cache: DashMap::new(),
            max_daily_request,
            cooldown_period_secs,
            last_reset_secs: now,
        }
    }

    pub fn register_account(&mut self, account: AccountAddress) -> Result<(), FaucetLimiterError> {
        // Trigger reset cache.
        self.reset_cache_if_needed();

        let cache_item = self.cache.entry(account).or_default();
        let now = Self::current_time_in_secs();

        if cache_item.count_of_usage < self.max_daily_request {
            if now >= cache_item.last_used_time + self.cooldown_period_secs {
                // Allow request and trigger increment.
                Ok(())
            } else {
                debug!("Registering account. Cooldown period for {account} has not elapsed.");
                Err(FaucetLimiterError::Cooldown {
                    account,
                    period: Duration::from_secs(
                        cache_item.last_used_time + self.cooldown_period_secs - now,
                    ),
                })
            }
        } else {
            debug!(
                "Registering account. User {account} requested {} times, exceeding the max daily limit of {}",
                cache_item.count_of_usage, self.max_daily_request
            );
            Err(FaucetLimiterError::DailyLimitExceeded {
                account_address: account,
                daily_request_limit: self.max_daily_request,
                retry_in: Self::time_left_for_the_day(),
            })
        }
    }

    fn current_time_in_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
    }

    fn time_left_for_the_day() -> String {
        let seconds_left =
            SECONDS_IN_A_DAY as u64 - (Self::current_time_in_secs() % SECONDS_IN_A_DAY as u64);

        let hours = seconds_left / 3600;
        let minutes = (seconds_left % 3600) / 60;
        let seconds = seconds_left % 60;

        format!("{:02}h : {:02}m :{:02}s", hours, minutes, seconds)
    }

    /// Increments the given [AccountAddress]'s counter if it has yet to exceed the
    /// request limit and cooldown period.
    pub fn increment_account_use_counter_if_under_limit(&mut self, account: AccountAddress) {
        let mut cache_item = self.cache.entry(account).or_default();
        let now = Self::current_time_in_secs();

        if cache_item.count_of_usage < self.max_daily_request {
            if now >= cache_item.last_used_time + self.cooldown_period_secs {
                cache_item.count_of_usage += 1;
                cache_item.last_used_time = now;
            } else {
                debug!("Incrementing use count. Cooldown period for {account} has not elapsed.");
            }
        } else {
            debug!(
                "Incrementing use count. User {account} requested {} times, exceeding the max daily limit of {}",
                cache_item.count_of_usage, self.max_daily_request
            );
        }
    }

    /// Reset the cache every day.
    fn reset_cache_if_needed(&mut self) {
        let now = Self::current_time_in_secs();
        let current_day = now / SECONDS_IN_A_DAY as u64;

        if current_day > self.last_reset_secs / SECONDS_IN_A_DAY as u64 {
            self.cache.clear();
            self.last_reset_secs = now;
            info!("Faucet API cache is cleared at {}", self.last_reset_secs);
        }
    }
}

impl Default for FaucetCallLimiter {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_DAILY_QUANTS_PER_ACCOUNT,
            DEFAULT_COOL_DOWN_PERIOD_FOR_FAUCET_IN_SECONDS,
        )
    }
}

#[derive(Error, Debug)]
pub enum FaucetLimiterError {
    #[error("Account {account} recently received some tokens. Please try again in {period:?}.")]
    Cooldown {
        account: AccountAddress,
        period: Duration,
    },
    #[error("The faucet request count for account {account_address} exceeds the daily limit of {daily_request_limit}. Come back after {retry_in:?}")]
    DailyLimitExceeded {
        account_address: AccountAddress,
        daily_request_limit: u64,
        retry_in: String,
    },
}

#[cfg(test)]
mod tests {
    use crate::rest::faucet_call_limiter::{FaucetCallLimiter, FaucetLimiterError};
    use move_core_types::account_address::AccountAddress;
    use std::time::Duration;
    use supra_logger::LevelFilter;
    use tokio::time;

    #[tokio::test]
    async fn test_faucet_call_limiter() -> anyhow::Result<()> {
        // Initialize logger with `off` level if RUST_LOG is not set.
        let _ = supra_logger::init_default_logger(LevelFilter::OFF);

        let max_daily_requests = 2;
        let cooldown_period_secs = 1;

        // Allow only [max_daily_requests] requests at least [cooldown_period_secs] second apart.
        let mut faucet_limiter = FaucetCallLimiter::new(max_daily_requests, cooldown_period_secs);

        let account_address = AccountAddress::random();

        // First call must be successful.
        assert!(faucet_limiter.register_account(account_address).is_ok());
        // Account a successful call.
        faucet_limiter.increment_account_use_counter_if_under_limit(account_address);

        // Second immediate call must fail.
        assert!(matches!(
            faucet_limiter.register_account(account_address),
            Err(FaucetLimiterError::Cooldown { .. })
        ));
        // Unsuccessful calls must not be accounted.

        // Retrying the second call must succeed if attempted after [cooldown_period_secs] + 100ms.
        time::sleep(Duration::from_millis(cooldown_period_secs * 1000 + 100)).await;
        assert!(faucet_limiter.register_account(account_address).is_ok());
        // Account a successful call.
        faucet_limiter.increment_account_use_counter_if_under_limit(account_address);

        // Third call must fail even after [cooldown_period_secs] + 100ms.
        time::sleep(Duration::from_millis(cooldown_period_secs * 1000 + 100)).await;
        assert!(matches!(
            faucet_limiter.register_account(account_address),
            Err(FaucetLimiterError::DailyLimitExceeded { .. })
        ));

        Ok(())
    }
}
