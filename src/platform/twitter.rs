//! Twitter Platform Implementation
//!
//! Provides unified Twitter platform access combining:
//! - TwitterStrategy for platform-specific behavior
//! - TwitterAdapter for API access

use async_trait::async_trait;
use std::sync::Arc;

use crate::adapters::TwitterAdapter;
use crate::config::platform::get_platform_id;
use crate::domain::errors::GatewayError;
use crate::platform::{Platform, PlatformConfigBase, PlatformStrategy};
use crate::ports::{CommentGateway, ContentGateway};
use crate::strategies::TwitterStrategy;
use crate::tikhub::TikHubClient;

/// Twitter platform configuration
#[derive(Debug, Clone)]
pub struct TwitterConfig {
    /// Base configuration
    pub base: PlatformConfigBase,

    /// TikHub API key
    pub api_key: String,

    /// TikHub base URL
    pub base_url: String,

    /// Default search type (Latest, Top, Media)
    pub default_search_type: String,
}

impl TwitterConfig {
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
            default_search_type: "Latest".to_string(),
        })
    }

    /// Create with explicit credentials
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            base: PlatformConfigBase::default(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.tikhub.io".to_string()),
            default_search_type: "Latest".to_string(),
        }
    }

    /// Set the default search type
    pub fn with_search_type(mut self, search_type: impl Into<String>) -> Self {
        self.default_search_type = search_type.into();
        self
    }
}

/// Twitter platform implementation
pub struct TwitterPlatform {
    config: TwitterConfig,
    strategy: TwitterStrategy,
    adapter: Arc<TwitterAdapter>,
}

impl TwitterPlatform {
    /// Create a new Twitter platform
    pub fn new(config: TwitterConfig) -> Result<Self, GatewayError> {
        let client = TikHubClient::new(&config.api_key, &config.base_url)
            .map_err(|e| GatewayError::AuthFailed(e.to_string()))?;

        let adapter = Arc::new(TwitterAdapter::new(client));
        let strategy = TwitterStrategy::with_search_type(&config.default_search_type);

        Ok(Self {
            config,
            strategy,
            adapter,
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, GatewayError> {
        let config = TwitterConfig::from_env().map_err(GatewayError::AuthFailed)?;
        Self::new(config)
    }

    /// Get the configuration
    pub fn config(&self) -> &TwitterConfig {
        &self.config
    }
}

#[async_trait]
impl Platform for TwitterPlatform {
    fn name(&self) -> &str {
        "twitter"
    }

    fn platform_id(&self) -> i32 {
        get_platform_id(self.name())
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
        // Try a simple search to verify connectivity
        use crate::domain::SearchOptions;
        let opts = SearchOptions::new("test").with_count(1);
        self.adapter.search(&opts).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_twitter_config_new() {
        let config = TwitterConfig::new("test-key", None);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://api.tikhub.io");
        assert_eq!(config.default_search_type, "Latest");
    }

    #[test]
    fn test_twitter_config_with_search_type() {
        let config = TwitterConfig::new("test-key", None).with_search_type("Top");
        assert_eq!(config.default_search_type, "Top");
    }

    #[test]
    fn test_twitter_config_custom_url() {
        let config = TwitterConfig::new("test-key", Some("https://custom.api".to_string()));
        assert_eq!(config.base_url, "https://custom.api");
    }
}
