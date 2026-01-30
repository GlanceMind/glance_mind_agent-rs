//! Domain layer - Core business entities and errors
//!
//! This module contains platform-agnostic domain models that represent
//! the core business concepts of the agent.

pub mod entities;
pub mod errors;

pub use entities::*;
pub use errors::*;
