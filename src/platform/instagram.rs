//! Instagram Platform Implementation
//!
//! Provides unified Instagram platform access combining:
//! - InstagramStrategy for platform-specific behavior
//! - InstagramAdapter for API access

use async_trait::async_trait;
use std::sync::Arc;

use crate::adapters::InstagramAdapter;
use crate::domain::errors::GatewayError;
use crate::platform::{Platform, PlatformConfigBase, PlatformStrategy};
use crate::ports::{CommentGateway, ContentGateway};
use crate::strategies::InstagramStrategy;
use crate::tikhub::TikHubClient;

/// Instagram platform configuration
#[derive(Debug, Clone)]
pub struct InstagramConfig {
    /// Base configuration
    pub base: PlatformConfigBase,

    /// TikHub API key
    pub api_key: String,

    /// TikHub base URL
    pub base_url: String,

    /// Default feed type for hashtag searches (top, recent, reels)
    pub default_feed_type: String,
}

impl InstagramConfig {
    /// Create from environment variables
    pub fn from_env() -> Result<Self, String> {
        let api_key =
            std::env::var("TIKHUB_API_KEY").map_err(|_| "TIKHUB_API_KEY not set".to_string())?;

        let base_url = std::env::var("TIKHUB_BASE_URL")
            .unwrap_or_else(|_| "https://api.tikhub.io".to_string());

        Ok(Self {
            base: PlatformConfigBase::default(),
            api_key,
            base_url,
            default_feed_type: "top".to_string(),
        })
    }

    /// Create with explicit credentials
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            base: PlatformConfigBase::default(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.tikhub.io".to_string()),
            default_feed_type: "top".to_string(),
        }
    }

    /// Set the default feed type for hashtag searches
    pub fn with_feed_type(mut self, feed_type: impl Into<String>) -> Self {
        self.default_feed_type = feed_type.into();
        self
    }
}

/// Instagram platform implementation
pub struct InstagramPlatform {
    config: InstagramConfig,
    strategy: InstagramStrategy,
    adapter: Arc<InstagramAdapter>,
}

impl InstagramPlatform {
    /// Create a new Instagram platform
    pub fn new(config: InstagramConfig) -> Result<Self, GatewayError> {
        let client = TikHubClient::new(&config.api_key, &config.base_url)
            .map_err(|e| GatewayError::AuthFailed(e.to_string()))?;

        let adapter = Arc::new(InstagramAdapter::new(client));
        let strategy = InstagramStrategy::with_feed_type(&config.default_feed_type);

        Ok(Self {
            config,
            strategy,
            adapter,
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, GatewayError> {
        let config = InstagramConfig::from_env().map_err(GatewayError::AuthFailed)?;
        Self::new(config)
    }

    /// Get the configuration
    pub fn config(&self) -> &InstagramConfig {
        &self.config
    }
}

#[async_trait]
impl Platform for InstagramPlatform {
    fn name(&self) -> &str {
        "instagram"
    }

    fn platform_id(&self) -> i32 {
        3
    }

    fn strategy(&self) -> &dyn PlatformStrategy {
        &self.strategy
    }

    fn content_gateway(&self) -> Arc<dyn ContentGateway> {
        self.adapter.clone()
    }

    fn comment_gateway(&self) -> Arc<dyn CommentGateway> {
        self.adapter.clone()
    }

    async fn health_check(&self) -> bool {
        // Try a simple hashtag search to verify connectivity
        use crate::domain::SearchOptions;
        let opts = SearchOptions::new("test").with_count(1);
        self.adapter.search(&opts).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instagram_config_new() {
        let config = InstagramConfig::new("test-key", None);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://api.tikhub.io");
        assert_eq!(config.default_feed_type, "top");
    }

    #[test]
    fn test_instagram_config_with_feed_type() {
        let config = InstagramConfig::new("test-key", None).with_feed_type("reels");
        assert_eq!(config.default_feed_type, "reels");
    }

    #[test]
    fn test_instagram_config_custom_url() {
        let config = InstagramConfig::new("test-key", Some("https://custom.api".to_string()));
        assert_eq!(config.base_url, "https://custom.api");
    }
}
