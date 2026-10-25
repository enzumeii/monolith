use config::ConfigError;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct CheckoutRecoveryConfig {
    pub enabled: bool,
    pub wake_secs: u64,
    pub batch_size: usize,
    pub concurrency: usize,
    pub budget_secs: u64,
    pub lease_secs: u32,
    pub retry_backoff_secs: u64,
    pub max_retry_backoff_secs: u64,
    pub max_failures: u32,
}

impl CheckoutRecoveryConfig {
    pub(super) fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=86400).contains(&self.wake_secs)
            || !(1..=100).contains(&self.batch_size)
            || !(1..=32).contains(&self.concurrency)
            || self.concurrency > self.batch_size
            || !(1..=240).contains(&self.budget_secs)
            || !(1..=86400).contains(&self.lease_secs)
            || !(1..=86400).contains(&self.retry_backoff_secs)
            || !(self.retry_backoff_secs..=86400).contains(&self.max_retry_backoff_secs)
            || !(1..=100).contains(&self.max_failures)
        {
            return Err(ConfigError::Message(
                "Checkout recovery intervals, limits, and concurrency must be positive and bounded"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/config/checkout_recovery.rs"]
mod tests;
