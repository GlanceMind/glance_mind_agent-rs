//! Instagram Platform Strategy
//!
//! Handles Instagram-specific keyword parsing, search options, and prompt formatting.

use crate::config::platform::get_platform_id;
use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};
use crate::strategies::PlatformStrategy;

/// Instagram platform strategy implementation
pub struct InstagramStrategy {
    /// Default feed type for hashtag searches (top, recent, reels)
    default_feed_type: String,
}

impl InstagramStrategy {
    /// Create a new Instagram strategy with default settings
    pub fn new() -> Self {
        Self {
            default_feed_type: "top".to_string(),
        }
    }

    /// Create with a custom default feed type
    pub fn with_feed_type(feed_type: impl Into<String>) -> Self {
        Self {
            default_feed_type: feed_type.into(),
        }
    }
}

impl Default for InstagramStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformStrategy for InstagramStrategy {
    fn name(&self) -> &str {
        "instagram"
    }

    fn platform_id(&self) -> i32 {
        get_platform_id(self.name())
    }

    fn parse_keyword(&self, keyword: &str) -> KeywordType {
        let keyword = keyword.trim();

        // Check for Instagram-specific prefixes
        if let Some(username) = keyword.strip_prefix("instagram_username:") {
            let username = username.trim_start_matches('@');
            return KeywordType::UserId(username.to_string());
        }

        if let Some(user_id) = keyword.strip_prefix("instagram_user_id:") {
            return KeywordType::SecUserId(user_id.to_string());
        }

        if let Some(code) = keyword.strip_prefix("instagram_code:") {
            return KeywordType::ContentId(code.to_string());
        }

        if let Some(post_id) = keyword.strip_prefix("instagram_post_id:") {
            return KeywordType::ContentId(post_id.to_string());
        }

        // Check for @ prefix (username)
        if let Some(username) = keyword.strip_prefix('@') {
            return KeywordType::UserId(username.to_string());
        }

        // Check for # prefix (hashtag)
        if let Some(tag) = keyword.strip_prefix('#') {
            return KeywordType::Hashtag(tag.to_string());
        }

        // Check if it looks like an Instagram shortcode (11 alphanumeric chars)
        if keyword.len() == 11
            && keyword
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return KeywordType::ContentId(keyword.to_string());
        }

        // Default: hashtag search (Instagram's primary search mechanism)
        KeywordType::Hashtag(keyword.to_string())
    }

    fn build_search_options(&self, config: &TaskConfig, keyword: &KeywordType) -> SearchOptions {
        let query = keyword.value().to_string();

        let mut options = SearchOptions::new(query).with_platform(self.name());

        // Instagram doesn't use regions, but we can store feed_type in region field
        options = options.with_region(self.default_feed_type.clone());

        // Set count from config
        let count = config.max_videos.map(|v| v.min(50) as u32).unwrap_or(20);
        options = options.with_count(count);

        options
    }

    fn format_analysis_prompt(
        &self,
        content: &Content,
        comments: &[Comment],
        context: &str,
    ) -> String {
        let mut prompt = String::new();

        // Post context
        prompt.push_str("## Instagram Post Information\n\n");
        prompt.push_str(&format!("**Post ID**: {}\n", content.content_id));
        prompt.push_str(&format!("**Author**: @{}\n", content.author));
        if let Some(ref name) = content.author_name {
            prompt.push_str(&format!("**Author Name**: {}\n", name));
        }
        prompt.push_str(&format!("**Caption**: {}\n", content.description));
        prompt.push_str(&format!(
            "**Engagement**: {} likes, {} comments, {} views\n",
            content.engagement.likes, content.engagement.comments, content.engagement.views
        ));
        if let Some(ref url) = content.url {
            prompt.push_str(&format!("**URL**: {}\n", url));
        }
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
            prompt.push('\n');
        }

        // Analysis instructions
        prompt.push_str("## Analysis Instructions\n\n");
        prompt.push_str("For each Instagram comment, please provide:\n");
        prompt.push_str("1. **Intent**: What is the commenter trying to convey? (question, feedback, purchase_intent, compliment, etc.)\n");
        prompt.push_str("2. **Sentiment**: Is the comment positive, negative, or neutral?\n");
        prompt.push_str("3. **Suggested Reply**: A friendly, engaging reply that matches Instagram's casual tone.\n");
        prompt.push_str("4. **Reason**: Brief explanation of why this reply is appropriate.\n\n");
        prompt.push_str("Please respond in JSON format with an array of analysis objects.\n");

        prompt
    }

    fn default_region(&self) -> &str {
        &self.default_feed_type
    }

    fn max_videos_per_search(&self) -> u32 {
        50 // Instagram API limit
    }

    fn max_comments_per_video(&self) -> u32 {
        50 // Instagram API limit per request
    }
}

/// Instagram-specific feed types for hashtag search
pub mod feed_type {
    /// Top posts (default)
    pub const TOP: &str = "top";
    /// Recent posts
    pub const RECENT: &str = "recent";
    /// Reels only
    pub const REELS: &str = "reels";
}

/// Instagram-specific comment sort options
pub mod comment_sort {
    /// Sort by relevance (default)
    pub const RELEVANT: &str = "relevant";
    /// Sort by most recent
    pub const RECENT: &str = "recent";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_regular_search() {
        let strategy = InstagramStrategy::new();
        let keyword = strategy.parse_keyword("fitness workout");
        assert!(matches!(keyword, KeywordType::Hashtag(_)));
        assert_eq!(keyword.value(), "fitness workout");
    }

    #[test]
    fn test_parse_username() {
        let strategy = InstagramStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("instagram_username:@testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");

        // With @ prefix only
        let keyword = strategy.parse_keyword("@testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");
    }

    #[test]
    fn test_parse_user_id() {
        let strategy = InstagramStrategy::new();
        let keyword = strategy.parse_keyword("instagram_user_id:123456789");
        assert!(matches!(keyword, KeywordType::SecUserId(_)));
        assert_eq!(keyword.value(), "123456789");
    }

    #[test]
    fn test_parse_post_code() {
        let strategy = InstagramStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("instagram_code:ABC123xyz-_");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
        assert_eq!(keyword.value(), "ABC123xyz-_");

        // Shortcode detection (11 chars)
        let keyword = strategy.parse_keyword("ABC123xyz-_");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
    }

    #[test]
    fn test_parse_hashtag() {
        let strategy = InstagramStrategy::new();
        let keyword = strategy.parse_keyword("#fitness");
        assert!(matches!(keyword, KeywordType::Hashtag(_)));
        assert_eq!(keyword.value(), "fitness");
    }

    #[test]
    fn test_build_search_options() {
        let strategy = InstagramStrategy::new();
        let config = TaskConfig::new(1, "instagram").with_max_videos(30);
        let keyword = KeywordType::Hashtag("fitness".to_string());

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.query, "fitness");
        assert_eq!(options.count, 30);
    }

    // ===== M5-T2 测试载荷(m5-instagram-p2.md §3 M5-T2;镜像 M4-T1/facebook 四条形状) =====
    // 覆盖 ID:R-006(count 语义前提)、T-001(ig)、D-02 豁免注释。
    // 断言 = 计划原文「count = 总量目标,不被任何单页上限截断」。

    /// M5-T2 测试 1(R-006/T-001 主断言):max_videos=137 → options.count == 137。
    /// 预期 RED(现状 instagram.rs:99 `v.min(50)`):assertion `left == right` failed,
    /// left: 50, right: 137。
    /// 反作弊:不得修改断言;不得引入新上限(把 50 换成 137 之外的任何硬 cap 仍须红)。
    #[test]
    fn search_count_equals_max_videos_137() {
        let strategy = InstagramStrategy::new();
        let config = TaskConfig::new(1, "instagram").with_max_videos(137);
        let keyword = KeywordType::Hashtag("fitness".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(
            options.count, 137,
            "options.count must equal max_videos (137) without any single-page cap; \
             got {}",
            options.count
        );
    }

    /// M5-T2 测试 2:max_videos=30 → options.count == 30。
    /// 钉死「count = 总量」语义,防实现者把 50 换成另一个低硬上限(例如 20)。
    #[test]
    fn search_count_equals_max_videos_30() {
        let strategy = InstagramStrategy::new();
        let config = TaskConfig::new(1, "instagram").with_max_videos(30);
        let keyword = KeywordType::Hashtag("fitness".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(
            options.count, 30,
            "options.count must equal max_videos (30) without any cap; got {}",
            options.count
        );
    }

    /// M5-T2 测试 3(现状默认值回归保护;允许先绿 AG-006:cap 行在 diff 内、
    /// AG-012 变异覆盖兜底):max_videos=None → options.count == 20。
    #[test]
    fn missing_max_videos_defaults_unchanged() {
        let strategy = InstagramStrategy::new();
        // max_videos 不设,使用 None
        let config = TaskConfig::new(1, "instagram");
        let keyword = KeywordType::Hashtag("fitness".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        // ASSERTION-CHANGE-JUSTIFIED: N/A — this is the original default (unwrap_or(20)),
        // remains unchanged; test verifies regression protection only.
        assert_eq!(
            options.count, 20,
            "missing max_videos should default to count=20 (unwrap_or(20) preserved); got {}",
            options.count
        );
    }

    #[test]
    fn test_format_analysis_prompt() {
        let strategy = InstagramStrategy::new();

        let content = Content::new("instagram", "ABC123")
            .with_author("testuser")
            .with_description("Test post caption #fitness");

        let comments = vec![Comment::new("instagram", "c1", "ABC123")
            .with_author("commenter1")
            .with_text("Love this! 😍")];

        let prompt = strategy.format_analysis_prompt(&content, &comments, "We sell fitness gear");

        assert!(prompt.contains("Instagram Post Information"));
        assert!(prompt.contains("@testuser"));
        assert!(prompt.contains("Love this! 😍"));
        assert!(prompt.contains("fitness gear"));
    }

    #[test]
    fn test_is_user_keyword() {
        let strategy = InstagramStrategy::new();

        assert!(strategy.is_user_keyword("@username"));
        assert!(strategy.is_user_keyword("instagram_username:user"));
        assert!(strategy.is_user_keyword("instagram_user_id:123"));
        assert!(!strategy.is_user_keyword("fitness"));
        assert!(!strategy.is_user_keyword("#hashtag"));
    }
}
