//! Platform Strategies - Platform-specific behavior implementations
//!
//! Each platform has unique characteristics for:
//! - Keyword parsing (user IDs, hashtags, search terms)
//! - Search option building (region, sort type)
//! - Prompt formatting for AI analysis
//!
//! Supported platforms:
//! - Facebook
//! - TikTok
//! - Instagram
//! - Reddit
//! - Twitter

pub mod facebook;
pub mod instagram;
pub mod reddit;
pub mod tiktok;
pub mod twitter;

use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};

/// Cross-platform extra option keys shared across strategies
pub mod extra_keys {
    /// Per-page fetch size hint passed to adapter pagination loops
    pub const PAGE_SIZE: &str = "page_size";
}

pub use facebook::FacebookStrategy;
pub use instagram::InstagramStrategy;
pub use reddit::RedditStrategy;
pub use tiktok::TikTokStrategy;
pub use twitter::TwitterStrategy;

/// Platform strategy trait defining platform-specific behavior
pub trait PlatformStrategy: Send + Sync {
    /// Get the platform name (e.g., "tiktok", "instagram")
    fn name(&self) -> &str;

    /// Get the platform ID (must match database)
    fn platform_id(&self) -> i32;

    /// Parse a keyword string into a typed keyword
    ///
    /// Examples:
    /// - "fitness" -> KeywordType::Search("fitness")
    /// - "tiktok_unique_id:@username" -> KeywordType::UserId("username")
    /// - "tiktok_sec_user_id:xxx" -> KeywordType::SecUserId("xxx")
    /// - "#workout" -> KeywordType::Hashtag("workout")
    fn parse_keyword(&self, keyword: &str) -> KeywordType;

    /// Build search options from task configuration
    fn build_search_options(&self, config: &TaskConfig, keyword: &KeywordType) -> SearchOptions;

    /// Format a prompt for AI analysis
    ///
    /// This creates a platform-specific prompt that includes
    /// content context, comment details, and analysis instructions.
    fn format_analysis_prompt(
        &self,
        content: &Content,
        comments: &[Comment],
        context: &str,
    ) -> String;

    /// Get default region for this platform
    fn default_region(&self) -> &str {
        "US"
    }

    /// Get maximum videos per search
    fn max_videos_per_search(&self) -> u32 {
        20
    }

    /// Get maximum comments per video
    fn max_comments_per_video(&self) -> u32 {
        100
    }

    /// Check if a keyword is a user-based keyword for this platform
    fn is_user_keyword(&self, keyword: &str) -> bool {
        let parsed = self.parse_keyword(keyword);
        parsed.is_user_based()
    }

    /// Check if a keyword is a content ID for this platform
    fn is_content_keyword(&self, keyword: &str) -> bool {
        let parsed = self.parse_keyword(keyword);
        parsed.is_content_based()
    }
}

/// Platform strategy registry
pub struct StrategyRegistry {
    strategies: std::collections::HashMap<String, Box<dyn PlatformStrategy>>,
}

impl StrategyRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            strategies: std::collections::HashMap::new(),
        }
    }

    /// Create a registry with default strategies
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(FacebookStrategy::new()));
        registry.register(Box::new(TikTokStrategy::new()));
        registry.register(Box::new(InstagramStrategy::new()));
        registry.register(Box::new(RedditStrategy::new()));
        registry.register(Box::new(TwitterStrategy::new()));
        registry
    }

    /// Register a platform strategy
    pub fn register(&mut self, strategy: Box<dyn PlatformStrategy>) {
        self.strategies
            .insert(strategy.name().to_string(), strategy);
    }

    /// Get a strategy by platform name
    pub fn get(&self, platform: &str) -> Option<&dyn PlatformStrategy> {
        self.strategies.get(platform).map(|s| s.as_ref())
    }

    /// Get a strategy by platform ID
    pub fn get_by_id(&self, platform_id: i32) -> Option<&dyn PlatformStrategy> {
        self.strategies
            .values()
            .find(|s| s.platform_id() == platform_id)
            .map(|s| s.as_ref())
    }

    /// List all registered platform names
    pub fn platforms(&self) -> Vec<&str> {
        self.strategies.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for StrategyRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_registry() {
        let registry = StrategyRegistry::with_defaults();

        // Check all platforms are registered
        assert!(registry.get("facebook").is_some());
        assert!(registry.get("tiktok").is_some());
        assert!(registry.get("instagram").is_some());
        assert!(registry.get("reddit").is_some());
        assert!(registry.get("twitter").is_some());

        // Check platform IDs (must match actual platform_id() implementations)
        let reddit = registry.get_by_id(1);
        assert!(reddit.is_some());
        assert_eq!(reddit.unwrap().name(), "reddit");

        let facebook = registry.get_by_id(3);
        assert!(facebook.is_some());
        assert_eq!(facebook.unwrap().name(), "facebook");

        let tiktok = registry.get_by_id(2);
        assert!(tiktok.is_some());
        assert_eq!(tiktok.unwrap().name(), "tiktok");

        let instagram = registry.get_by_id(4);
        assert!(instagram.is_some());
        assert_eq!(instagram.unwrap().name(), "instagram");

        let twitter = registry.get_by_id(5);
        assert!(twitter.is_some());
        assert_eq!(twitter.unwrap().name(), "twitter");
    }
}
