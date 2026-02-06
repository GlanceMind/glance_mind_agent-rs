//! TikTok Platform Implementation
//!
//! Provides unified TikTok platform access combining:
//! - TikTokStrategy for platform-specific behavior
//! - TikHubAdapter for API access

use async_trait::async_trait;
use std::sync::Arc;

use crate::adapters::TikHubAdapter;
use crate::config::platform::get_platform_id;
use crate::domain::errors::GatewayError;
use crate::platform::{Platform, PlatformConfigBase, PlatformStrategy};
use crate::ports::{CommentGateway, ContentGateway};
use crate::strategies::TikTokStrategy;
use crate::tikhub::TikHubClient;

/// TikTok platform configuration
#[derive(Debug, Clone)]
pub struct TikTokConfig {
    /// Base configuration
    pub base: PlatformConfigBase,

    /// TikHub API key
    pub api_key: String,

    /// TikHub base URL
    pub base_url: String,
}

impl TikTokConfig {
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
        })
    }

    /// Create with explicit credentials
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            base: PlatformConfigBase::default(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.tikhub.io".to_string()),
        }
    }
}

/// TikTok platform implementation
pub struct TikTokPlatform {
    config: TikTokConfig,
    strategy: TikTokStrategy,
    adapter: Arc<TikHubAdapter>,
}

impl TikTokPlatform {
    /// Create a new TikTok platform
    pub fn new(config: TikTokConfig) -> Result<Self, GatewayError> {
        let client = TikHubClient::new(&config.api_key, &config.base_url)
            .map_err(|e| GatewayError::AuthFailed(e.to_string()))?;

        let adapter = Arc::new(TikHubAdapter::new(client));
        let strategy = TikTokStrategy::with_region(&config.base.default_region);

        Ok(Self {
            config,
            strategy,
            adapter,
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, GatewayError> {
        let config = TikTokConfig::from_env().map_err(GatewayError::AuthFailed)?;
        Self::new(config)
    }

    /// Get the configuration
    pub fn config(&self) -> &TikTokConfig {
        &self.config
    }
}

#[async_trait]
impl Platform for TikTokPlatform {
    fn name(&self) -> &str {
        "tiktok"
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
    fn test_tiktok_config_new() {
        let config = TikTokConfig::new("test-key", None);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://api.tikhub.io");
    }

    #[test]
    fn test_tiktok_config_custom_url() {
        let config = TikTokConfig::new("test-key", Some("https://custom.api".to_string()));
        assert_eq!(config.base_url, "https://custom.api");
    }
}
