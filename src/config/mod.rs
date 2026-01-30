//! Configuration - Application-wide configuration loaded at startup
//!
//! This module provides centralized configuration management, including:
//! - Platform registry (loaded from database)
//! - Application settings
//!
//! All configuration is initialized once at startup and shared across components.

pub mod platform;

pub use platform::{PlatformInfo, PlatformLookup, PlatformRegistry};
