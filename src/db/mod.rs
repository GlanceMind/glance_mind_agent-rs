//! Database module using Diesel ORM
//!
//! Provides typed database access for the agent.

mod connection;
pub mod schema;
pub mod models;

pub use connection::{DbPool, establish_connection, establish_pool};
pub use models::*;
