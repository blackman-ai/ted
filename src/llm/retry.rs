// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Retry logic for LLM API calls with exponential backoff

use crate::error::{ApiError, Result, TedError};
use rand::Rng;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration with smart defaults
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay in milliseconds (exponentially increased)
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Jitter percentage (0.0 to 1.0)
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 1000, // Start at 1 second
            max_delay_ms: 16000, // Cap at 16 seconds
            jitter: 0.25,        // ±25% randomization
        }
    }
}

impl RetryConfig {
    /// Calculate delay for a given attempt number
    fn calculate_delay(&self, attempt: u32) -> Duration {
        // Exponential backoff: base * 2^attempt
        let exponential_ms = self.base_delay_ms * 2u64.pow(attempt);
        let capped_ms = exponential_ms.min(self.max_delay_ms);

        // Add jitter
        let jitter_range = (capped_ms as f64 * self.jitter) as i64;
        let mut rng = rand::rng();
        let jitter_ms = rng.random_range(-jitter_range..=jitter_range);

        let final_ms = (capped_ms as i64 + jitter_ms).max(0) as u64;
        Duration::from_millis(final_ms)
    }
}

/// Determine if an error is retryable
pub fn is_retryable(error: &TedError) -> bool {
    match error {
        TedError::Api(api_error) => match api_error {
            // Retry on transient failures
            ApiError::Network(_) => true,
            ApiError::RateLimited(_) => true,
            ApiError::Timeout => true,
            ApiError::ServerError { status, .. } => {
                // Retry on 5xx errors
                *status >= 500 && *status < 600
            }
            ApiError::StreamError(_) => true,

            // Don't retry on client errors
            ApiError::AuthenticationFailed => false,
            ApiError::ModelNotFound(_) => false,
            ApiError::ContextTooLong { .. } => false,
            ApiError::InvalidResponse(_) => false,
        },
        _ => false,
    }
}

/// Retry a function with exponential backoff
///
/// # Arguments
/// * `operation` - The async operation to retry
/// * `config` - Retry configuration (uses default if None)
/// * `operation_name` - Name of the operation for logging
///
/// # Returns
/// Result of the operation after retries
pub async fn with_retry<F, Fut, T>(
    mut operation: F,
    config: Option<RetryConfig>,
    operation_name: &str,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let config = config.unwrap_or_default();
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    eprintln!(
                        "[RETRY] {} succeeded after {} attempts",
                        operation_name,
                        attempt + 1
                    );
                }
                return Ok(result);
            }
            Err(error) => {
                if !is_retryable(&error) {
                    eprintln!(
                        "[RETRY] {} failed with non-retryable error: {}",
                        operation_name, error
                    );
                    return Err(error);
                }

                if attempt >= config.max_retries {
                    eprintln!(
                        "[RETRY] {} exhausted all {} retries",
                        operation_name, config.max_retries
                    );
                    return Err(error);
                }

                let delay = config.calculate_delay(attempt);
                eprintln!(
                    "[RETRY] {} failed (attempt {}/{}): {}. Retrying in {:.1}s...",
                    operation_name,
                    attempt + 1,
                    config.max_retries,
                    error,
                    delay.as_secs_f64()
                );

                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 16000);
        assert!((config.jitter - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_calculate_delay() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 1000,
            max_delay_ms: 16000,
            jitter: 0.0, // No jitter for predictable testing
        };

        // Attempt 0: 1000ms
        let delay0 = config.calculate_delay(0);
        assert_eq!(delay0.as_millis(), 1000);

        // Attempt 1: 2000ms
        let delay1 = config.calculate_delay(1);
        assert_eq!(delay1.as_millis(), 2000);

        // Attempt 2: 4000ms
        let delay2 = config.calculate_delay(2);
        assert_eq!(delay2.as_millis(), 4000);

        // Attempt 3: 8000ms
        let delay3 = config.calculate_delay(3);
        assert_eq!(delay3.as_millis(), 8000);

        // Attempt 4: 16000ms (capped)
        let delay4 = config.calculate_delay(4);
        assert_eq!(delay4.as_millis(), 16000);

        // Attempt 5: 16000ms (still capped)
        let delay5 = config.calculate_delay(5);
        assert_eq!(delay5.as_millis(), 16000);
    }

    #[test]
    fn test_is_retryable() {
        // Retryable errors
        assert!(is_retryable(&TedError::Api(ApiError::Network(
            "timeout".to_string()
        ))));
        assert!(is_retryable(&TedError::Api(ApiError::RateLimited(60))));
        assert!(is_retryable(&TedError::Api(ApiError::Timeout)));
        assert!(is_retryable(&TedError::Api(ApiError::ServerError {
            status: 500,
            message: "Internal error".to_string(),
        })));
        assert!(is_retryable(&TedError::Api(ApiError::StreamError(
            "connection lost".to_string()
        ))));

        // Non-retryable errors
        assert!(!is_retryable(&TedError::Api(
            ApiError::AuthenticationFailed
        )));
        assert!(!is_retryable(&TedError::Api(ApiError::ModelNotFound(
            "model".to_string()
        ))));
        assert!(!is_retryable(&TedError::Api(ApiError::ContextTooLong {
            current: 10000,
            limit: 8000,
        })));
        assert!(!is_retryable(&TedError::Api(ApiError::InvalidResponse(
            "bad json".to_string()
        ))));
    }

    #[tokio::test]
    async fn test_with_retry_success_first_try() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = with_retry(
            || async {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, TedError>(42)
            },
            None,
            "test_operation",
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_success_after_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = with_retry(
            || async {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(TedError::Api(ApiError::Network("timeout".to_string())))
                } else {
                    Ok(42)
                }
            },
            Some(RetryConfig {
                max_retries: 5,
                base_delay_ms: 10, // Fast retries for testing
                max_delay_ms: 100,
                jitter: 0.0,
            }),
            "test_operation",
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3); // Failed 2 times, succeeded on 3rd
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = with_retry(
            || async {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(TedError::Api(ApiError::AuthenticationFailed))
            },
            None,
            "test_operation",
        )
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Should not retry
    }

    #[tokio::test]
    async fn test_with_retry_exhausts_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = with_retry(
            || async {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(TedError::Api(ApiError::Network("timeout".to_string())))
            },
            Some(RetryConfig {
                max_retries: 3,
                base_delay_ms: 10,
                max_delay_ms: 100,
                jitter: 0.0,
            }),
            "test_operation",
        )
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 4); // Initial + 3 retries
    }
}
