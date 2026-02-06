//! Reddit Platform Implementation
//!
//! Provides unified Reddit platform access combining:
//! - RedditStrategy for platform-specific behavior
//! - RedditAdapter for API access

use async_trait::async_trait;
use std::sync::Arc;

use crate::adapters::RedditAdapter;
use crate::config::platform::get_platform_id;
use crate::domain::errors::GatewayError;
use crate::platform::{Platform, PlatformConfigBase, PlatformStrategy};
use crate::ports::{CommentGateway, ContentGateway};
use crate::strategies::RedditStrategy;
use crate::tikhub::TikHubClient;

/// Reddit platform configuration
#[derive(Debug, Clone)]
pub struct RedditConfig {
    /// Base configuration
    pub base: PlatformConfigBase,

    /// TikHub API key
    pub api_key: String,

    /// TikHub base URL
    pub base_url: String,

    /// Default sort order (relevance, hot, top, new)
    pub default_sort: String,

    /// Whether to allow NSFW content
    pub allow_nsfw: bool,
}

impl RedditConfig {
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
            default_sort: "relevance".to_string(),
            allow_nsfw: false,
        })
    }

    /// Create with explicit credentials
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            base: PlatformConfigBase::default(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.tikhub.io".to_string()),
            default_sort: "relevance".to_string(),
            allow_nsfw: false,
        }
    }

    /// Set the default sort order
    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.default_sort = sort.into();
        self
    }

    /// Allow or disallow NSFW content
    pub fn with_nsfw(mut self, allow: bool) -> Self {
        self.allow_nsfw = allow;
        self
    }
}

/// Reddit platform implementation
pub struct RedditPlatform {
    config: RedditConfig,
    strategy: RedditStrategy,
    adapter: Arc<RedditAdapter>,
}

impl RedditPlatform {
    /// Create a new Reddit platform
    pub fn new(config: RedditConfig) -> Result<Self, GatewayError> {
        let client = TikHubClient::new(&config.api_key, &config.base_url)
            .map_err(|e| GatewayError::AuthFailed(e.to_string()))?;

        let adapter = Arc::new(RedditAdapter::new(client));
        let strategy = RedditStrategy::with_options(&config.default_sort, config.allow_nsfw);

        Ok(Self {
            config,
            strategy,
            adapter,
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, GatewayError> {
        let config = RedditConfig::from_env().map_err(GatewayError::AuthFailed)?;
        Self::new(config)
    }

    /// Get the configuration
    pub fn config(&self) -> &RedditConfig {
        &self.config
    }
}

#[async_trait]
impl Platform for RedditPlatform {
    fn name(&self) -> &str {
        "reddit"
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
    fn test_reddit_config_new() {
        let config = RedditConfig::new("test-key", None);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://api.tikhub.io");
        assert_eq!(config.default_sort, "relevance");
        assert!(!config.allow_nsfw);
    }

    #[test]
    fn test_reddit_config_with_options() {
        let config = RedditConfig::new("test-key", None)
            .with_sort("hot")
            .with_nsfw(true);
        assert_eq!(config.default_sort, "hot");
        assert!(config.allow_nsfw);
    }

    #[test]
    fn test_reddit_config_custom_url() {
        let config = RedditConfig::new("test-key", Some("https://custom.api".to_string()));
        assert_eq!(config.base_url, "https://custom.api");
    }
}
