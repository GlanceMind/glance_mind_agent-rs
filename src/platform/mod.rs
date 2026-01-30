//! Platform Management - Unified platform handling
//!
//! This module provides a unified interface for managing different social media platforms.
//! Each platform has:
//! - A strategy for platform-specific behavior (keyword parsing, prompt formatting)
//! - An API adapter for content and comment fetching
//! - Configuration for platform-specific settings
//!
//! ## Supported Platforms
//! - TikTok (via TikHub API)
//!
//! ## Architecture
//! ```text
//! Platform
//! ├── Strategy (keyword parsing, prompt building)
//! ├── ContentAdapter (implements ContentGateway)
//! └── CommentAdapter (implements CommentGateway)
//! ```

pub mod tiktok;
pub mod registry;

use async_trait::async_trait;
use std::sync::Arc;

// Re-export platform strategy trait from strategies module
pub use crate::strategies::PlatformStrategy;

// Re-export TikTok components
pub use tiktok::{TikTokPlatform, TikTokConfig};

// Re-export registry
pub use registry::PlatformRegistry;

use crate::ports::{ContentGateway, CommentGateway};

/// Platform trait combining strategy and adapters
#[async_trait]
pub trait Platform: Send + Sync {
    /// Get the platform name
    fn name(&self) -> &str;

    /// Get the platform ID
    fn platform_id(&self) -> i32;

    /// Get the platform strategy
    fn strategy(&self) -> &dyn PlatformStrategy;

    /// Get the content gateway
    fn content_gateway(&self) -> Arc<dyn ContentGateway>;

    /// Get the comment gateway
    fn comment_gateway(&self) -> Arc<dyn CommentGateway>;

    /// Check if the platform is healthy
    async fn health_check(&self) -> bool;
}

/// Platform configuration base
#[derive(Debug, Clone)]
pub struct PlatformConfigBase {
    /// Whether the platform is enabled
    pub enabled: bool,
    
    /// Default region
    pub default_region: String,
    
    /// Max videos per search
    pub max_videos_per_search: u32,
    
    /// Max comments per video
    pub max_comments_per_video: u32,
    
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for PlatformConfigBase {
    fn default() -> Self {
        Self {
            enabled: true,
            default_region: "US".to_string(),
            max_videos_per_search: 20,
            max_comments_per_video: 100,
            timeout_secs: 30,
        }
    }
}
