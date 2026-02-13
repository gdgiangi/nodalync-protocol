//! Retry logic with exponential backoff.

use std::future::Future;
use std::time::Duration;

use tokio::time::sleep;
use tracing::{debug, warn};

use crate::config::RetryConfig;
use crate::error::{SettleError, SettleResult};

/// Retry policy with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including the initial attempt)
    max_attempts: u32,
    /// Base delay between retries
    base_delay: Duration,
    /// Maximum delay between retries
    max_delay: Duration,
}

impl RetryPolicy {
    /// Create a new retry policy.
    pub fn new(max_attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts,
            base_delay,
            max_delay,
        }
    }

    /// Create from retry config.
    pub fn from_config(config: &RetryConfig) -> Self {
        Self {
            max_attempts: config.max_attempts,
            base_delay: config.base_delay,
            max_delay: config.max_delay,
        }
    }

    /// Calculate the delay for a given attempt (0-indexed).
    ///
    /// Uses exponential backoff with +-25% jitter to prevent thundering herd.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        // Exponential backoff: base_delay * 2^(attempt-1)
        let multiplier = 2u64.saturating_pow(attempt - 1);
        let base = self.base_delay.saturating_mul(multiplier as u32);

        // Cap at max_delay
        let capped = std::cmp::min(base, self.max_delay);

        // Add +-25% jitter
        let jitter_range = capped.as_millis() as u64 / 4;
        if jitter_range == 0 {
            return capped;
        }
        let jitter = rand::random::<u64>() % (jitter_range * 2);
        let jittered_ms = (capped.as_millis() as u64)
            .saturating_sub(jitter_range)
            .saturating_add(jitter);
        Duration::from_millis(jittered_ms)
    }

    /// Execute an async operation with retry logic.
    ///
    /// Only retries on retryable errors (network, timeout).
    /// Returns immediately on non-retryable errors.
    pub async fn execute<F, Fut, T>(&self, mut operation: F) -> SettleResult<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = SettleResult<T>>,
    {
        let mut last_error = None;

        for attempt in 0..self.max_attempts {
            // Wait before retry (no wait for first attempt)
            let delay = self.delay_for_attempt(attempt);
            if !delay.is_zero() {
                debug!(attempt, ?delay, "Retrying after delay");
                sleep(delay).await;
            }

            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if e.is_retryable() && attempt + 1 < self.max_attempts {
                        warn!(
                            attempt = attempt + 1,
                            max_attempts = self.max_attempts,
                            error = %e,
                            "Retryable error, will retry"
                        );
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // Should not reach here, but return last error just in case
        Err(last_error.unwrap_or(SettleError::timeout("max retries exceeded")))
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
        }
    }
}

/// Check if an error is retryable.
#[allow(dead_code)]
pub fn is_retryable(error: &SettleError) -> bool {
    error.is_retryable()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_delay_for_attempt() {
        let policy = RetryPolicy::new(5, Duration::from_millis(100), Duration::from_secs(5));

        // First attempt has no delay
        assert_eq!(policy.delay_for_attempt(0), Duration::ZERO);

        // Exponential backoff with +-25% jitter
        // Base: 100ms, range: [75, 125]
        let d1 = policy.delay_for_attempt(1);
        assert!(
            d1 >= Duration::from_millis(75) && d1 <= Duration::from_millis(125),
            "Attempt 1 delay {:?} should be within +-25% of 100ms",
            d1
        );

        // Base: 200ms, range: [150, 250]
        let d2 = policy.delay_for_attempt(2);
        assert!(
            d2 >= Duration::from_millis(150) && d2 <= Duration::from_millis(250),
            "Attempt 2 delay {:?} should be within +-25% of 200ms",
            d2
        );

        // Base: 400ms, range: [300, 500]
        let d3 = policy.delay_for_attempt(3);
        assert!(
            d3 >= Duration::from_millis(300) && d3 <= Duration::from_millis(500),
            "Attempt 3 delay {:?} should be within +-25% of 400ms",
            d3
        );
    }

    #[test]
    fn test_delay_capped_at_max() {
        let policy = RetryPolicy::new(10, Duration::from_millis(100), Duration::from_millis(500));

        // Should be capped at max_delay (with jitter: +-25% of 500 = [375, 625])
        let d5 = policy.delay_for_attempt(5);
        assert!(
            d5 >= Duration::from_millis(375) && d5 <= Duration::from_millis(625),
            "Attempt 5 delay {:?} should be within +-25% of 500ms cap",
            d5
        );

        let d10 = policy.delay_for_attempt(10);
        assert!(
            d10 >= Duration::from_millis(375) && d10 <= Duration::from_millis(625),
            "Attempt 10 delay {:?} should be within +-25% of 500ms cap",
            d10
        );
    }

    #[test]
    fn test_jitter_varies_between_calls() {
        // Verify that jitter actually produces different values
        let policy = RetryPolicy::new(5, Duration::from_millis(1000), Duration::from_secs(60));

        let mut delays: Vec<Duration> = (0..10).map(|_| policy.delay_for_attempt(1)).collect();
        delays.dedup();
        // With 1000ms base and 250ms jitter range, it's extremely unlikely
        // all 10 values are identical
        assert!(delays.len() > 1, "Jitter should produce varying delays");
    }

    #[tokio::test]
    async fn test_execute_success_first_attempt() {
        let policy = RetryPolicy::default();
        let attempts = Arc::new(AtomicU32::new(0));

        let result = policy
            .execute(|| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, SettleError>(42)
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_retry_on_network_error() {
        let policy = RetryPolicy::new(3, Duration::from_millis(10), Duration::from_millis(100));
        let attempts = Arc::new(AtomicU32::new(0));

        let result = policy
            .execute(|| {
                let attempts = Arc::clone(&attempts);
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Err(SettleError::network("connection refused"))
                    } else {
                        Ok::<_, SettleError>(42)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_execute_no_retry_on_non_retryable_error() {
        let policy = RetryPolicy::new(5, Duration::from_millis(10), Duration::from_millis(100));
        let attempts = Arc::new(AtomicU32::new(0));

        let result = policy
            .execute(|| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(SettleError::EmptyBatch)
                }
            })
            .await;

        assert!(matches!(result, Err(SettleError::EmptyBatch)));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_max_retries_exceeded() {
        let policy = RetryPolicy::new(3, Duration::from_millis(10), Duration::from_millis(100));
        let attempts = Arc::new(AtomicU32::new(0));

        let result = policy
            .execute(|| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(SettleError::network("always fails"))
                }
            })
            .await;

        assert!(matches!(result, Err(SettleError::Network(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
