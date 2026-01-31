//! Twitter Platform Strategy
//!
//! Handles Twitter-specific keyword parsing, search options, and prompt formatting.

use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};
use crate::strategies::PlatformStrategy;

/// Twitter platform strategy implementation
pub struct TwitterStrategy {
    /// Default search type (Latest, Top, Media, etc.)
    default_search_type: String,
}

impl TwitterStrategy {
    /// Create a new Twitter strategy with default settings
    pub fn new() -> Self {
        Self {
            default_search_type: "Latest".to_string(),
        }
    }

    /// Create with a custom default search type
    pub fn with_search_type(search_type: impl Into<String>) -> Self {
        Self {
            default_search_type: search_type.into(),
        }
    }
}

impl Default for TwitterStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformStrategy for TwitterStrategy {
    fn name(&self) -> &str {
        "twitter"
    }

    fn platform_id(&self) -> i32 {
        5 // Must match database platform ID
    }

    fn parse_keyword(&self, keyword: &str) -> KeywordType {
        let keyword = keyword.trim();

        // Check for Twitter-specific prefixes
        if let Some(handle) = keyword.strip_prefix("twitter_handle:") {
            let handle = handle.trim_start_matches('@');
            return KeywordType::UserId(handle.to_string());
        }

        if let Some(rest_id) = keyword.strip_prefix("twitter_rest_id:") {
            return KeywordType::SecUserId(rest_id.to_string());
        }

        if let Some(tweet_id) = keyword.strip_prefix("twitter_tweet_id:") {
            return KeywordType::ContentId(tweet_id.to_string());
        }

        // Check for @ prefix (handle)
        if let Some(handle) = keyword.strip_prefix('@') {
            return KeywordType::UserId(handle.to_string());
        }

        // Check for # prefix (hashtag)
        if let Some(tag) = keyword.strip_prefix('#') {
            return KeywordType::Hashtag(format!("#{}", tag));
        }

        // Check if it looks like a tweet ID (15+ digit number)
        if keyword.len() >= 15 && keyword.chars().all(|c| c.is_ascii_digit()) {
            return KeywordType::ContentId(keyword.to_string());
        }

        // Default: regular search
        KeywordType::Search(keyword.to_string())
    }

    fn build_search_options(&self, config: &TaskConfig, keyword: &KeywordType) -> SearchOptions {
        let query = keyword.value().to_string();

        let mut options = SearchOptions::new(query).with_platform(self.name());

        // Store search type in region field
        options = options.with_region(self.default_search_type.clone());

        // Set count from config
        let count = config.max_videos.map(|v| v.min(100) as u32).unwrap_or(20);
        options = options.with_count(count);

        // Set sort type if specified
        if let Some(sort) = config.sort_type {
            options.sort_type = Some(sort);
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

        // Tweet context
        prompt.push_str("## Twitter/X Post Information\n\n");
        prompt.push_str(&format!("**Tweet ID**: {}\n", content.content_id));
        prompt.push_str(&format!("**Author**: @{}\n", content.author));
        if let Some(ref name) = content.author_name {
            prompt.push_str(&format!("**Display Name**: {}\n", name));
        }
        prompt.push_str(&format!("**Text**: {}\n", content.description));
        prompt.push_str(&format!(
            "**Engagement**: {} likes, {} replies, {} retweets, {} views\n",
            content.engagement.likes,
            content.engagement.comments,
            content.engagement.shares,
            content.engagement.views
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

        // Replies to analyze (Twitter's comments are called replies)
        prompt.push_str("## Replies to Analyze\n\n");
        for (i, comment) in comments.iter().enumerate() {
            prompt.push_str(&format!("### Reply {}\n", i + 1));
            prompt.push_str(&format!("- **Tweet ID**: {}\n", comment.comment_id));
            prompt.push_str(&format!("- **User**: @{}", comment.author));
            if let Some(ref name) = comment.author_name {
                prompt.push_str(&format!(" ({})", name));
            }
            prompt.push('\n');
            prompt.push_str(&format!("- **Text**: {}\n", comment.text));
            prompt.push_str(&format!("- **Likes**: {}\n", comment.likes));
            if comment.reply_count > 0 {
                prompt.push_str(&format!("- **Replies**: {}\n", comment.reply_count));
            }
            if let Some(ref lang) = comment.language {
                prompt.push_str(&format!("- **Language**: {}\n", lang));
            }
            prompt.push('\n');
        }

        // Analysis instructions
        prompt.push_str("## Analysis Instructions\n\n");
        prompt.push_str("For each Twitter reply, please provide:\n");
        prompt.push_str("1. **Intent**: What is the user trying to convey? (question, support, criticism, engagement, etc.)\n");
        prompt.push_str("2. **Sentiment**: Is the reply positive, negative, or neutral?\n");
        prompt.push_str("3. **Suggested Reply**: A concise, engaging reply that fits Twitter's character limit and fast-paced nature.\n");
        prompt.push_str("4. **Reason**: Brief explanation of why this reply is appropriate.\n\n");
        prompt.push_str("Note: Twitter/X values concise, witty, and authentic responses. Keep replies under 280 characters when possible.\n");
        prompt.push_str("Please respond in JSON format with an array of analysis objects.\n");

        prompt
    }

    fn default_region(&self) -> &str {
        &self.default_search_type
    }

    fn max_videos_per_search(&self) -> u32 {
        100 // Twitter API limit
    }

    fn max_comments_per_video(&self) -> u32 {
        50 // Default limit per request
    }
}

/// Twitter-specific search types
pub mod search_type {
    /// Latest tweets (chronological)
    pub const LATEST: &str = "Latest";
    /// Top tweets (by engagement)
    pub const TOP: &str = "Top";
    /// Media tweets (with images/videos)
    pub const MEDIA: &str = "Media";
    /// People search
    pub const PEOPLE: &str = "People";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_regular_search() {
        let strategy = TwitterStrategy::new();
        let keyword = strategy.parse_keyword("rust programming");
        assert!(matches!(keyword, KeywordType::Search(_)));
        assert_eq!(keyword.value(), "rust programming");
    }

    #[test]
    fn test_parse_handle() {
        let strategy = TwitterStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("twitter_handle:@testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");

        // With @ prefix only
        let keyword = strategy.parse_keyword("@testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");
    }

    #[test]
    fn test_parse_rest_id() {
        let strategy = TwitterStrategy::new();
        let keyword = strategy.parse_keyword("twitter_rest_id:123456789");
        assert!(matches!(keyword, KeywordType::SecUserId(_)));
        assert_eq!(keyword.value(), "123456789");
    }

    #[test]
    fn test_parse_tweet_id() {
        let strategy = TwitterStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("twitter_tweet_id:1234567890123456789");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
        assert_eq!(keyword.value(), "1234567890123456789");

        // Numeric string detection (15+ digits)
        let keyword = strategy.parse_keyword("1234567890123456789");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
    }

    #[test]
    fn test_parse_hashtag() {
        let strategy = TwitterStrategy::new();
        let keyword = strategy.parse_keyword("#rustlang");
        assert!(matches!(keyword, KeywordType::Hashtag(_)));
        assert_eq!(keyword.value(), "#rustlang");
    }

    #[test]
    fn test_build_search_options() {
        let strategy = TwitterStrategy::new();
        let config = TaskConfig::new(1, "twitter").with_max_videos(50);
        let keyword = KeywordType::Search("rust".to_string());

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.query, "rust");
        assert_eq!(options.count, 50);
    }

    #[test]
    fn test_format_analysis_prompt() {
        let strategy = TwitterStrategy::new();

        let content = Content::new("twitter", "1234567890")
            .with_author("testuser")
            .with_description("Just released a new Rust library! Check it out 🦀");

        let comments = vec![Comment::new("twitter", "reply123", "1234567890")
            .with_author("replier")
            .with_text("This is awesome! How do I get started?")];

        let prompt = strategy.format_analysis_prompt(&content, &comments, "We build Rust tools");

        assert!(prompt.contains("Twitter/X Post Information"));
        assert!(prompt.contains("@testuser"));
        assert!(prompt.contains("How do I get started"));
        assert!(prompt.contains("Rust tools"));
        assert!(prompt.contains("280 characters"));
    }

    #[test]
    fn test_is_user_keyword() {
        let strategy = TwitterStrategy::new();

        assert!(strategy.is_user_keyword("@username"));
        assert!(strategy.is_user_keyword("twitter_handle:user"));
        assert!(strategy.is_user_keyword("twitter_rest_id:123"));
        assert!(!strategy.is_user_keyword("rust"));
        assert!(!strategy.is_user_keyword("#hashtag"));
    }

    #[test]
    fn test_is_content_keyword() {
        let strategy = TwitterStrategy::new();

        assert!(strategy.is_content_keyword("twitter_tweet_id:123"));
        assert!(strategy.is_content_keyword("1234567890123456789"));
        assert!(!strategy.is_content_keyword("@username"));
        assert!(!strategy.is_content_keyword("rust"));
    }
}
