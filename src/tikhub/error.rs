//! TikHub API specific errors
//!
//! Error handling based on TikHub HTTP Status Codes:
//! - 400: Bad Request - request format incorrect or parameters invalid
//! - 401: Unauthorized - API token issues
//! - 402: Payment Required - insufficient balance
//! - 403: Forbidden - permission issues
//! - 404: Not Found - data not found
//! - 429: Too Many Requests - rate limited
//! - 500: Internal Server Error

use thiserror::Error;

/// TikHub API error with detailed categorization
#[derive(Error, Debug, Clone)]
pub enum TikHubError {
    // ==================== Fatal Errors (Immediately terminate task) ====================
    /// 401 - API Token invalid/missing/expired
    #[error("TikHub authentication failed: {message}")]
    Unauthorized { message: String },

    /// 402 - Insufficient balance
    #[error("TikHub payment required: {message}")]
    PaymentRequired { message: String },

    /// 403 - Permission denied / Account disabled
    #[error("TikHub access forbidden: {message}")]
    Forbidden { message: String },

    /// Missing API key in configuration
    #[error("TikHub API key not configured")]
    MissingApiKey,

    // ==================== Retryable Errors ====================
    /// 429 - Rate limited
    #[error("TikHub rate limited, retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },

    /// 500+ - Server internal error
    #[error("TikHub server error ({status}): {message}")]
    ServerError { status: u16, message: String },

    /// Network error (timeout, connection failed, etc.)
    #[error("TikHub network error: {message}")]
    NetworkError { message: String },

    // ==================== Skippable Errors ====================
    /// 400 - Bad request (might be temporary, retry once)
    #[error("TikHub bad request: {message}")]
    BadRequest { message: String },

    /// 404 - Data not found
    #[error("TikHub data not found: {message}")]
    NotFound { message: String },

    /// API returned empty data
    #[error("TikHub returned empty data")]
    EmptyData,

    // ==================== Other Errors ====================
    /// JSON parsing failed
    #[error("TikHub response parse error: {0}")]
    ParseError(String),

    /// Invalid parameter provided
    #[error("TikHub invalid parameter: {0}")]
    InvalidParam(String),
}

impl TikHubError {
    /// Create error from HTTP status code and message
    pub fn from_status(status: u16, message: impl Into<String>) -> Self {
        let message = message.into();
        match status {
            400 => TikHubError::BadRequest { message },
            401 => TikHubError::Unauthorized { message },
            402 => TikHubError::PaymentRequired { message },
            403 => TikHubError::Forbidden { message },
            404 => TikHubError::NotFound { message },
            429 => TikHubError::RateLimited {
                retry_after_secs: Some(60),
            },
            500..=599 => TikHubError::ServerError { status, message },
            _ => TikHubError::ServerError {
                status,
                message: format!("HTTP {}: {}", status, message),
            },
        }
    }

    /// Create error from API response code (TikHub's internal code field)
    pub fn from_api_code(code: i32, message: impl Into<String>) -> Self {
        // TikHub uses HTTP-like codes in response body
        Self::from_status(code as u16, message)
    }

    /// Legacy: Create an API error (for backward compatibility)
    pub fn api(code: i32, message: impl Into<String>) -> Self {
        Self::from_api_code(code, message)
    }

    /// Check if this is a fatal error (should immediately terminate the task)
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            TikHubError::Unauthorized { .. }
                | TikHubError::PaymentRequired { .. }
                | TikHubError::Forbidden { .. }
                | TikHubError::MissingApiKey
        )
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            TikHubError::RateLimited { .. }
                | TikHubError::ServerError { .. }
                | TikHubError::NetworkError { .. }
                | TikHubError::BadRequest { .. } // 400 might be temporary server issue
        )
    }

    /// Check if this error can be skipped (continue with next item)
    pub fn is_skippable(&self) -> bool {
        matches!(
            self,
            TikHubError::NotFound { .. } | TikHubError::BadRequest { .. } | TikHubError::EmptyData
        )
    }

    /// Check if this is a rate limit error
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, TikHubError::RateLimited { .. })
    }

    /// Get recommended retry delay in milliseconds
    pub fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            TikHubError::RateLimited { retry_after_secs } => {
                Some(retry_after_secs.unwrap_or(60) * 1000)
            }
            TikHubError::ServerError { .. } => Some(2000),
            TikHubError::NetworkError { .. } => Some(1000),
            TikHubError::BadRequest { .. } => Some(500),
            _ => None,
        }
    }

    /// Get maximum retry attempts for this error type
    pub fn max_retries(&self) -> u32 {
        match self {
            TikHubError::RateLimited { .. } => 3,
            TikHubError::ServerError { .. } => 3,
            TikHubError::NetworkError { .. } => 3,
            TikHubError::BadRequest { .. } => 1, // Only retry once for bad request
            _ => 0,
        }
    }

    /// Legacy: Check if this error is recoverable (for backward compatibility)
    pub fn is_recoverable(&self) -> bool {
        self.is_retryable()
    }
}

impl From<reqwest::Error> for TikHubError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            TikHubError::NetworkError {
                message: "Request timeout".to_string(),
            }
        } else if err.is_connect() {
            TikHubError::NetworkError {
                message: format!("Connection failed: {}", err),
            }
        } else {
            TikHubError::NetworkError {
                message: err.to_string(),
            }
        }
    }
}

impl From<serde_json::Error> for TikHubError {
    fn from(err: serde_json::Error) -> Self {
        TikHubError::ParseError(err.to_string())
    }
}

/// Retry configuration for TikHub API calls
#[derive(Debug, Clone)]
pub struct TikHubRetryConfig {
    /// Maximum retry attempts (default: 3)
    pub max_retries: u32,
    /// Initial delay in milliseconds (default: 1000)
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds (default: 60000)
    pub max_delay_ms: u64,
    /// Backoff multiplier (default: 2.0)
    pub backoff_multiplier: f64,
}

impl Default for TikHubRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
        }
    }
}

impl TikHubRetryConfig {
    /// Get the effective max retries for a specific error
    pub fn max_retries_for_error(&self, error: &TikHubError) -> u32 {
        error.max_retries().min(self.max_retries)
    }

    /// Calculate delay for a given attempt number
    pub fn delay_for_attempt(&self, attempt: u32, error: &TikHubError) -> u64 {
        // Use error-specific delay if available, otherwise use exponential backoff
        let base_delay = error.retry_delay_ms().unwrap_or(self.initial_delay_ms);
        let delay = (base_delay as f64 * self.backoff_multiplier.powi(attempt as i32)) as u64;
        delay.min(self.max_delay_ms)
    }
}

/// Result of a fetch operation that may have partial data
#[derive(Debug)]
pub struct PartialFetchResult<T> {
    /// The fetched data
    pub data: Vec<T>,
    /// Whether the fetch was partial (interrupted before completion)
    pub is_partial: bool,
    /// Error that caused partial fetch (if any)
    pub error: Option<TikHubError>,
}

impl<T> PartialFetchResult<T> {
    /// Create a successful complete result
    pub fn complete(data: Vec<T>) -> Self {
        Self {
            data,
            is_partial: false,
            error: None,
        }
    }

    /// Create a partial result with error
    pub fn partial(data: Vec<T>, error: TikHubError) -> Self {
        Self {
            data,
            is_partial: true,
            error: Some(error),
        }
    }

    /// Check if we have any data
    pub fn has_data(&self) -> bool {
        !self.data.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_from_status() {
        let err = TikHubError::from_status(401, "Invalid token");
        assert!(err.is_fatal());
        assert!(!err.is_retryable());

        let err = TikHubError::from_status(429, "Rate limited");
        assert!(!err.is_fatal());
        assert!(err.is_retryable());
        assert!(err.is_rate_limited());

        let err = TikHubError::from_status(404, "Not found");
        assert!(!err.is_fatal());
        assert!(err.is_skippable());

        let err = TikHubError::from_status(500, "Server error");
        assert!(!err.is_fatal());
        assert!(err.is_retryable());
    }

    #[test]
    fn test_retry_delay() {
        let err = TikHubError::RateLimited {
            retry_after_secs: Some(30),
        };
        assert_eq!(err.retry_delay_ms(), Some(30000));

        let err = TikHubError::RateLimited {
            retry_after_secs: None,
        };
        assert_eq!(err.retry_delay_ms(), Some(60000)); // Default 60s

        let err = TikHubError::ServerError {
            status: 500,
            message: "".to_string(),
        };
        assert_eq!(err.retry_delay_ms(), Some(2000));
    }

    #[test]
    fn test_max_retries() {
        assert_eq!(
            TikHubError::RateLimited {
                retry_after_secs: None
            }
            .max_retries(),
            3
        );
        assert_eq!(
            TikHubError::BadRequest {
                message: "".to_string()
            }
            .max_retries(),
            1
        );
        assert_eq!(
            TikHubError::Unauthorized {
                message: "".to_string()
            }
            .max_retries(),
            0
        );
    }

    #[test]
    fn test_retry_config() {
        let config = TikHubRetryConfig::default();

        let err = TikHubError::ServerError {
            status: 500,
            message: "".to_string(),
        };
        assert_eq!(config.max_retries_for_error(&err), 3);

        // Test exponential backoff
        let delay1 = config.delay_for_attempt(0, &err);
        let delay2 = config.delay_for_attempt(1, &err);
        assert!(delay2 > delay1);
    }
}
