//! Error types for the agent

use thiserror::Error;

/// Main error type for the agent
#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("Connection pool error: {0}")]
    ConnectionPool(#[from] diesel::r2d2::PoolError),

    #[error("TikHub API error: code={code}, message={message}")]
    TikHubApi { code: i32, message: String },

    #[error("Fixture not found: {0}")]
    FixtureNotFound(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, Error>;
