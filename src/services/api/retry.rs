use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

use crate::types::{AtomError, Result};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub retryable_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            retryable_status_codes: vec![429, 500, 502, 503, 529],
        }
    }
}

/// Execute an async operation with retry logic
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt == config.max_retries || !is_retryable(&e, config) {
                    return Err(e);
                }

                let delay = calculate_delay(attempt, config);
                warn!(
                    attempt = attempt + 1,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "Retrying after error"
                );
                sleep(delay).await;
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or(AtomError::Api {
        message: "Retry exhausted".into(),
        status: None,
    }))
}

fn is_retryable(error: &AtomError, config: &RetryConfig) -> bool {
    match error {
        AtomError::Api {
            status: Some(code), ..
        } => config.retryable_status_codes.contains(code),
        AtomError::Http(_) => true,
        AtomError::Io(_) => true,
        _ => false,
    }
}

fn calculate_delay(attempt: u32, config: &RetryConfig) -> Duration {
    let delay_ms = config.base_delay_ms * 2u64.pow(attempt);
    let delay_ms = delay_ms.min(config.max_delay_ms);
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert!(config.retryable_status_codes.contains(&429));
        assert!(config.retryable_status_codes.contains(&529));
    }

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig::default();
        assert_eq!(calculate_delay(0, &config), Duration::from_millis(500));
        assert_eq!(calculate_delay(1, &config), Duration::from_millis(1000));
        assert_eq!(calculate_delay(2, &config), Duration::from_millis(2000));
    }

    #[test]
    fn test_delay_cap() {
        let config = RetryConfig {
            max_delay_ms: 1000,
            ..Default::default()
        };
        assert_eq!(calculate_delay(10, &config), Duration::from_millis(1000));
    }

    #[test]
    fn test_retryable_errors() {
        let config = RetryConfig::default();
        assert!(is_retryable(
            &AtomError::Api {
                message: "".into(),
                status: Some(429)
            },
            &config
        ));
        assert!(is_retryable(
            &AtomError::Api {
                message: "".into(),
                status: Some(529)
            },
            &config
        ));
        assert!(!is_retryable(
            &AtomError::Api {
                message: "".into(),
                status: Some(400)
            },
            &config
        ));
        assert!(!is_retryable(
            &AtomError::PermissionDenied {
                tool_name: "".into()
            },
            &config
        ));
    }

    #[tokio::test]
    async fn test_with_retry_success() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 1,
            ..Default::default()
        };
        let count = Arc::new(AtomicU32::new(0));
        let count_clone = count.clone();

        let result = with_retry(&config, || {
            let c = count_clone.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(AtomError::Api {
                        message: "fail".into(),
                        status: Some(429),
                    })
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 2,
            base_delay_ms: 1,
            ..Default::default()
        };

        let result: Result<i32> = with_retry(&config, || async {
            Err(AtomError::Api {
                message: "always fail".into(),
                status: Some(429),
            })
        })
        .await;

        assert!(result.is_err());
    }
}
