//! Database connection management

use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use tracing::info;

use crate::error::{Error, Result};

/// Type alias for the connection pool
pub type DbPool = Pool<ConnectionManager<PgConnection>>;

/// Type alias for a pooled connection
pub type PooledConn = PooledConnection<ConnectionManager<PgConnection>>;

/// Establish a single database connection
/// 
/// # Arguments
/// * `database_url` - PostgreSQL connection URL
pub fn establish_connection(database_url: &str) -> Result<PgConnection> {
    use diesel::Connection;
    
    PgConnection::establish(database_url)
        .map_err(|e| Error::Config(format!("Failed to connect to database: {}", e)))
}

/// Establish a connection pool
/// 
/// # Arguments
/// * `database_url` - PostgreSQL connection URL
/// * `max_size` - Maximum number of connections in the pool (default: 10)
pub fn establish_pool(database_url: &str, max_size: Option<u32>) -> Result<DbPool> {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    
    let pool = Pool::builder()
        .max_size(max_size.unwrap_or(10))
        .build(manager)?;
    
    info!("Database connection pool established (max_size={})", max_size.unwrap_or(10));
    
    Ok(pool)
}

/// Establish a connection pool from environment variables
/// 
/// Expects DATABASE_URL environment variable to be set.
pub fn establish_pool_from_env() -> Result<DbPool> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| Error::Config("DATABASE_URL must be set".to_string()))?;
    
    establish_pool(&database_url, None)
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    
    /// Get a test database URL
    /// 
    /// Uses TEST_DATABASE_URL if set, otherwise falls back to DATABASE_URL
    pub fn get_test_database_url() -> Option<String> {
        std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()
    }
    
    /// Establish a test connection pool
    pub fn establish_test_pool() -> Option<DbPool> {
        get_test_database_url()
            .and_then(|url| establish_pool(&url, Some(5)).ok())
    }
}
