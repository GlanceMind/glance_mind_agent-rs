//! TikTok Platform Strategy
//!
//! Handles TikTok-specific keyword parsing, search options, and prompt formatting.

use crate::config::platform::get_platform_id;
use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};
use crate::strategies::PlatformStrategy;

/// TikTok platform strategy implementation
pub struct TikTokStrategy {
    /// Default region code
    default_region: String,
}

impl TikTokStrategy {
    /// Create a new TikTok strategy with default settings
    pub fn new() -> Self {
        Self {
            default_region: "US".to_string(),
        }
    }

    /// Create with a custom default region
    pub fn with_region(region: impl Into<String>) -> Self {
        Self {
            default_region: region.into(),
        }
    }
}

impl Default for TikTokStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformStrategy for TikTokStrategy {
    fn name(&self) -> &str {
        "tiktok"
    }

    fn platform_id(&self) -> i32 {
        get_platform_id(self.name())
    }

    fn parse_keyword(&self, keyword: &str) -> KeywordType {
        let keyword = keyword.trim();

        // Check for TikTok-specific prefixes
        if let Some(uid) = keyword.strip_prefix("tiktok_unique_id:") {
            // Remove @ prefix if present
            let uid = uid.trim_start_matches('@');
            return KeywordType::UserId(uid.to_string());
        }

        if let Some(sec_uid) = keyword.strip_prefix("tiktok_sec_user_id:") {
            return KeywordType::SecUserId(sec_uid.to_string());
        }

        if let Some(video_id) = keyword.strip_prefix("tiktok_video_id:") {
            return KeywordType::ContentId(video_id.to_string());
        }

        // Check for @ prefix (username)
        if let Some(username) = keyword.strip_prefix('@') {
            return KeywordType::UserId(username.to_string());
        }

        // Check for # prefix (hashtag)
        if let Some(tag) = keyword.strip_prefix('#') {
            return KeywordType::Hashtag(tag.to_string());
        }

        // Check if it looks like a numeric video ID (all digits, 15+ chars)
        if keyword.len() >= 15 && keyword.chars().all(|c| c.is_ascii_digit()) {
            return KeywordType::ContentId(keyword.to_string());
        }

        // Default: regular search
        KeywordType::Search(keyword.to_string())
    }

    fn build_search_options(&self, config: &TaskConfig, keyword: &KeywordType) -> SearchOptions {
        let query = keyword.value().to_string();

        let mut options = SearchOptions::new(query).with_platform(self.name());

        // Set region from config or use default
        if let Some(ref region) = config.region {
            options = options.with_region(region.clone());
        } else {
            options = options.with_region(self.default_region.clone());
        }

        // Set the desired TOTAL number of videos. The adapter paginates the
        // TikHub search endpoint (<=20 per page) to reach this total, so we
        // must NOT clamp it to the per-page cap here.
        let count = config.max_videos.map(|v| v as u32).unwrap_or(10);
        options = options.with_count(count);

        // Set sort type if specified (0=relevance, 1=most_liked)
        if let Some(sort) = config.sort_type {
            options.sort_type = Some(sort);
        }

        // Set publish time filter if specified (0=all, 1=day, 7=week, 30=month, 90=3months, 180=6months)
        if let Some(publish_time) = config.publish_time {
            options.publish_time = Some(publish_time);
        }

        options
    }

    fn format_analysis_prompt(
        &self,
        content: &Content,
        comments: &[Comment],
        context: &str,
    ) -> String {
        let mut prompt = String::new();

        // Video context
        prompt.push_str("## TikTok Video Information\n\n");
        prompt.push_str(&format!("**Video ID**: {}\n", content.content_id));
        prompt.push_str(&format!("**Author**: @{}\n", content.author));
        if let Some(ref name) = content.author_name {
            prompt.push_str(&format!("**Author Name**: {}\n", name));
        }
        prompt.push_str(&format!("**Description**: {}\n", content.description));
        prompt.push_str(&format!(
            "**Engagement**: {} likes, {} comments, {} shares, {} views\n",
            content.engagement.likes,
            content.engagement.comments,
            content.engagement.shares,
            content.engagement.views
        ));
        prompt.push('\n');

        // Business context
        if !context.is_empty() {
            prompt.push_str("## Business Context\n\n");
            prompt.push_str(context);
            prompt.push_str("\n\n");
        }

        // Comments to analyze
        prompt.push_str("## Comments to Analyze\n\n");
        for (i, comment) in comments.iter().enumerate() {
            prompt.push_str(&format!("### Comment {}\n", i + 1));
            prompt.push_str(&format!("- **ID**: {}\n", comment.comment_id));
            prompt.push_str(&format!("- **User**: @{}", comment.author));
            if let Some(ref name) = comment.author_name {
                prompt.push_str(&format!(" ({})", name));
            }
            prompt.push('\n');
            prompt.push_str(&format!("- **Text**: {}\n", comment.text));
            prompt.push_str(&format!("- **Likes**: {}\n", comment.likes));
            if comment.is_reply {
                prompt.push_str("- **Type**: Reply\n");
            }
            if let Some(ref lang) = comment.language {
                prompt.push_str(&format!("- **Language**: {}\n", lang));
            }
            prompt.push('\n');
        }

        // Analysis instructions
        prompt.push_str("## Analysis Instructions\n\n");
        prompt.push_str("For each comment, please provide:\n");
        prompt.push_str("1. **Intent**: What is the commenter trying to convey? (question, feedback, purchase_intent, etc.)\n");
        prompt.push_str("2. **Sentiment**: Is the comment positive, negative, or neutral?\n");
        prompt.push_str("3. **Suggested Reply**: A friendly, helpful reply that addresses the comment appropriately.\n");
        prompt.push_str("4. **Reason**: Brief explanation of why this reply is appropriate.\n\n");
        prompt.push_str("Please respond in JSON format with an array of analysis objects.\n");

        prompt
    }

    fn default_region(&self) -> &str {
        &self.default_region
    }

    fn max_videos_per_search(&self) -> u32 {
        20 // TikHub per-PAGE size (the adapter paginates to reach larger totals)
    }

    fn max_comments_per_video(&self) -> u32 {
        100 // TikHub API limit per request
    }
}

/// TikTok-specific sort types
pub mod sort_type {
    /// Sort by relevance (default)
    pub const RELEVANCE: u8 = 0;
    /// Sort by likes
    pub const LIKES: u8 = 1;
    /// Sort by newest
    pub const NEWEST: u8 = 2;
}

/// TikTok-specific time filters
pub mod publish_time {
    /// All time
    pub const ALL: u8 = 0;
    /// Last 24 hours
    pub const DAY: u8 = 1;
    /// Last week
    pub const WEEK: u8 = 2;
    /// Last month
    pub const MONTH: u8 = 3;
    /// Last 3 months
    pub const THREE_MONTHS: u8 = 4;
    /// Last 6 months
    pub const SIX_MONTHS: u8 = 5;
    /// Last year
    pub const YEAR: u8 = 6;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_regular_search() {
        let strategy = TikTokStrategy::new();
        let keyword = strategy.parse_keyword("fitness workout");
        assert!(matches!(keyword, KeywordType::Search(_)));
        assert_eq!(keyword.value(), "fitness workout");
    }

    #[test]
    fn test_parse_unique_id() {
        let strategy = TikTokStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("tiktok_unique_id:@testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");

        // With @ prefix only
        let keyword = strategy.parse_keyword("@testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");
    }

    #[test]
    fn test_parse_sec_user_id() {
        let strategy = TikTokStrategy::new();
        let keyword = strategy.parse_keyword("tiktok_sec_user_id:MS4wLjABAAAA...");
        assert!(matches!(keyword, KeywordType::SecUserId(_)));
        assert_eq!(keyword.value(), "MS4wLjABAAAA...");
    }

    #[test]
    fn test_parse_video_id() {
        let strategy = TikTokStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("tiktok_video_id:7123456789012345678");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
        assert_eq!(keyword.value(), "7123456789012345678");

        // Numeric string detection
        let keyword = strategy.parse_keyword("7123456789012345678");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
    }

    #[test]
    fn test_parse_hashtag() {
        let strategy = TikTokStrategy::new();
        let keyword = strategy.parse_keyword("#fitness");
        assert!(matches!(keyword, KeywordType::Hashtag(_)));
        assert_eq!(keyword.value(), "fitness");
    }

    #[test]
    fn test_build_search_options() {
        let strategy = TikTokStrategy::new();
        let config = TaskConfig::new(1, "tiktok")
            .with_region("GB")
            .with_max_videos(15);
        let keyword = KeywordType::Search("fitness".to_string());

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.query, "fitness");
        assert_eq!(options.region, Some("GB".to_string()));
        assert_eq!(options.count, 15);
    }

    #[test]
    fn test_build_search_options_default_region() {
        let strategy = TikTokStrategy::with_region("JP");
        let config = TaskConfig::new(1, "tiktok");
        let keyword = KeywordType::Search("test".to_string());

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.region, Some("JP".to_string()));
    }

    #[test]
    fn test_format_analysis_prompt() {
        let strategy = TikTokStrategy::new();

        let content = Content::new("tiktok", "123456")
            .with_author("testuser")
            .with_description("Test video");

        let comments = vec![Comment::new("tiktok", "c1", "123456")
            .with_author("commenter1")
            .with_text("Great video!")];

        let prompt =
            strategy.format_analysis_prompt(&content, &comments, "We sell fitness equipment");

        assert!(prompt.contains("TikTok Video Information"));
        assert!(prompt.contains("@testuser"));
        assert!(prompt.contains("Great video!"));
        assert!(prompt.contains("fitness equipment"));
    }

    #[test]
    fn test_is_user_keyword() {
        let strategy = TikTokStrategy::new();

        assert!(strategy.is_user_keyword("@username"));
        assert!(strategy.is_user_keyword("tiktok_unique_id:user"));
        assert!(strategy.is_user_keyword("tiktok_sec_user_id:xxx"));
        assert!(!strategy.is_user_keyword("fitness"));
        assert!(!strategy.is_user_keyword("#hashtag"));
    }

    #[test]
    fn test_is_content_keyword() {
        let strategy = TikTokStrategy::new();

        assert!(strategy.is_content_keyword("tiktok_video_id:123"));
        assert!(strategy.is_content_keyword("7123456789012345678"));
        assert!(!strategy.is_content_keyword("@username"));
        assert!(!strategy.is_content_keyword("fitness"));
    }
}
