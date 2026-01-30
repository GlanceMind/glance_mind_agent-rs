//! Domain errors - Business logic error types
//!
//! These errors represent failures in the domain layer and workflow execution,
//! independent of specific infrastructure implementations.

use thiserror::Error;

/// Main workflow error type
#[derive(Error, Debug)]
pub enum WorkflowError {
    /// Error from content gateway (fetching videos/posts)
    #[error("Content gateway error: {0}")]
    Gateway(#[from] GatewayError),

    /// Error from AI analyzer
    #[error("AI analysis error: {0}")]
    Ai(#[from] AiError),

    /// Error from database operations
    #[error("Database error: {0}")]
    Database(#[from] DbError),

    /// Error from task queue
    #[error("Queue error: {0}")]
    Queue(#[from] QueueError),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Platform not supported
    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    /// Task was cancelled or stopped
    #[error("Task cancelled: {0}")]
    Cancelled(String),

    /// Task timeout
    #[error("Task timeout after {0}ms")]
    Timeout(u64),

    /// Invalid task data
    #[error("Invalid task data: {0}")]
    InvalidTask(String),
}

/// Gateway errors (content and comment fetching)
#[derive(Error, Debug)]
pub enum GatewayError {
    /// HTTP/network error
    #[error("Network error: {0}")]
    Network(String),

    /// API returned an error response
    #[error("API error (code={code}): {message}")]
    Api { code: i32, message: String },

    /// Rate limited by the API
    #[error("Rate limited, retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    /// No data returned from API
    #[error("Empty response from API")]
    EmptyResponse,

    /// Failed to parse API response
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// Invalid parameters provided
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// Content not found
    #[error("Content not found: {0}")]
    NotFound(String),
}

impl GatewayError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            GatewayError::Network(_) | GatewayError::RateLimited { .. }
        )
    }

    /// Check if this is a rate limit error
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, GatewayError::RateLimited { .. })
    }
}

/// AI analyzer errors
#[derive(Error, Debug)]
pub enum AiError {
    /// HTTP/network error connecting to AI service
    #[error("Network error: {0}")]
    Network(String),

    /// AI service returned an error
    #[error("AI service error: {0}")]
    ServiceError(String),

    /// Rate limited by AI service
    #[error("AI rate limited")]
    RateLimited,

    /// Failed to parse AI response
    #[error("Failed to parse AI response: {0}")]
    ParseError(String),

    /// Invalid prompt or input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Token limit exceeded
    #[error("Token limit exceeded: {used} > {limit}")]
    TokenLimitExceeded { used: i32, limit: i32 },

    /// Model not available
    #[error("Model not available: {0}")]
    ModelUnavailable(String),

    /// Content filtered by safety system
    #[error("Content filtered: {0}")]
    ContentFiltered(String),
}

impl AiError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AiError::Network(_) | AiError::RateLimited | AiError::ServiceError(_)
        )
    }
}

/// Database errors
#[derive(Error, Debug)]
pub enum DbError {
    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Query execution error
    #[error("Query error: {0}")]
    Query(String),

    /// Record not found
    #[error("Record not found: {0}")]
    NotFound(String),

    /// Duplicate record
    #[error("Duplicate record: {0}")]
    Duplicate(String),

    /// Constraint violation
    #[error("Constraint violation: {0}")]
    Constraint(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Pool exhausted
    #[error("Connection pool exhausted")]
    PoolExhausted,

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl DbError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DbError::Connection(_) | DbError::PoolExhausted | DbError::Transaction(_)
        )
    }
}

/// Queue errors (Redis task queue)
#[derive(Error, Debug)]
pub enum QueueError {
    /// Connection error
    #[error("Queue connection error: {0}")]
    Connection(String),

    /// Failed to deserialize task
    #[error("Failed to deserialize task: {0}")]
    Deserialization(String),

    /// Failed to serialize task result
    #[error("Failed to serialize result: {0}")]
    Serialization(String),

    /// Queue is empty
    #[error("Queue is empty")]
    Empty,

    /// Task acknowledgment failed
    #[error("Task acknowledgment failed: {0}")]
    AckFailed(String),

    /// Task not found in queue
    #[error("Task not found: {0}")]
    TaskNotFound(String),
}

impl QueueError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(self, QueueError::Connection(_))
    }
}

/// Result type for workflow operations
pub type WorkflowResult<T> = std::result::Result<T, WorkflowError>;

/// Result type for gateway operations
pub type GatewayResult<T> = std::result::Result<T, GatewayError>;

/// Result type for AI operations
pub type AiResult<T> = std::result::Result<T, AiError>;

/// Result type for database operations
pub type DbResult<T> = std::result::Result<T, DbError>;

/// Result type for queue operations
pub type QueueResult<T> = std::result::Result<T, QueueError>;

// ============================================================
// Error Conversion Utilities
// ============================================================

impl From<reqwest::Error> for GatewayError {
    fn from(err: reqwest::Error) -> Self {
        GatewayError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for GatewayError {
    fn from(err: serde_json::Error) -> Self {
        GatewayError::ParseError(err.to_string())
    }
}

impl From<diesel::result::Error> for DbError {
    fn from(err: diesel::result::Error) -> Self {
        match err {
            diesel::result::Error::NotFound => DbError::NotFound("Record not found".into()),
            diesel::result::Error::DatabaseError(kind, info) => match kind {
                diesel::result::DatabaseErrorKind::UniqueViolation => {
                    DbError::Duplicate(info.message().to_string())
                }
                diesel::result::DatabaseErrorKind::ForeignKeyViolation
                | diesel::result::DatabaseErrorKind::CheckViolation
                | diesel::result::DatabaseErrorKind::NotNullViolation => {
                    DbError::Constraint(info.message().to_string())
                }
                _ => DbError::Query(info.message().to_string()),
            },
            _ => DbError::Query(err.to_string()),
        }
    }
}

impl From<diesel::r2d2::PoolError> for DbError {
    fn from(err: diesel::r2d2::PoolError) -> Self {
        DbError::Connection(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_error_retryable() {
        assert!(GatewayError::Network("timeout".into()).is_retryable());
        assert!(GatewayError::RateLimited {
            retry_after_secs: Some(60)
        }
        .is_retryable());
        assert!(!GatewayError::AuthFailed("invalid key".into()).is_retryable());
        assert!(!GatewayError::NotFound("video".into()).is_retryable());
    }

    #[test]
    fn test_ai_error_retryable() {
        assert!(AiError::Network("timeout".into()).is_retryable());
        assert!(AiError::RateLimited.is_retryable());
        assert!(!AiError::InvalidInput("bad prompt".into()).is_retryable());
    }

    #[test]
    fn test_db_error_retryable() {
        assert!(DbError::Connection("closed".into()).is_retryable());
        assert!(DbError::PoolExhausted.is_retryable());
        assert!(!DbError::NotFound("record".into()).is_retryable());
    }

    #[test]
    fn test_workflow_error_from_gateway() {
        let gateway_err = GatewayError::NotFound("video123".into());
        let workflow_err: WorkflowError = gateway_err.into();
        assert!(matches!(workflow_err, WorkflowError::Gateway(_)));
    }
}
