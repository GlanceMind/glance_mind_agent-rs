//! Database module using Diesel ORM
//!
//! Provides typed database access for the agent.

mod connection;
pub mod models;
pub mod schema;

pub use connection::{establish_connection, establish_pool, DbPool};
pub use models::*;
