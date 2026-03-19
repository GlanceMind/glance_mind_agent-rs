//! Twitter Platform Strategy
//!
//! Handles Twitter-specific keyword parsing, search options, and prompt formatting.

use serde_json::json;

use crate::config::platform::get_platform_id;
use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};
use crate::strategies::PlatformStrategy;

pub mod extra_keys {
    pub const MODE: &str = "mode";
    pub const SEARCH_TYPE: &str = "search_type";
}

pub mod mode {
    pub const SEARCH: &str = "search";
    pub const HASHTAG: &str = "hashtag";
    pub const HANDLE: &str = "handle";
    pub const REST_ID: &str = "rest_id";
    pub const TWEET_ID: &str = "tweet_id";
}

/// Twitter platform strategy implementation
pub struct TwitterStrategy {
    /// Default region value kept for TaskConfig/SearchOptions parity.
    default_region: String,
    /// Default search type (Latest, Top, Media, etc.)
    default_search_type: String,
}

impl TwitterStrategy {
    /// Create a new Twitter strategy with default settings
    pub fn new() -> Self {
        Self {
            default_region: "GLOBAL".to_string(),
            default_search_type: "Latest".to_string(),
        }
    }

    /// Create with a custom default search type
    pub fn with_search_type(search_type: impl Into<String>) -> Self {
        Self {
            default_region: "GLOBAL".to_string(),
            default_search_type: Self::normalize_search_type(search_type.into()),
        }
    }

    fn normalize_search_type(search_type: impl AsRef<str>) -> String {
        match search_type.as_ref().trim().to_ascii_lowercase().as_str() {
            "top" => search_type::TOP.to_string(),
            "media" => search_type::MEDIA.to_string(),
            "people" => search_type::PEOPLE.to_string(),
            "lists" => search_type::LISTS.to_string(),
            _ => search_type::LATEST.to_string(),
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
        get_platform_id(self.name())
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
        let region = config
            .region
            .clone()
            .unwrap_or_else(|| self.default_region.clone());
        let query = keyword.value().to_string();

        let mut options = SearchOptions::new(query)
            .with_platform(self.name())
            .with_region(region);

        // Set count from config
        let count = config.max_videos.map(|v| v.min(100) as u32).unwrap_or(20);
        options = options.with_count(count);

        let configured_search_type = config
            .extra
            .get(extra_keys::SEARCH_TYPE)
            .and_then(|value| value.as_str())
            .map(Self::normalize_search_type)
            .unwrap_or_else(|| self.default_search_type.clone());
        options = options.with_extra_value(extra_keys::SEARCH_TYPE, json!(configured_search_type));

        // Set sort type if specified
        if let Some(sort) = config.sort_type {
            options.sort_type = Some(sort);
        }

        options = match keyword {
            KeywordType::Search(query) => options
                .with_query(query.clone())
                .with_extra_value(extra_keys::MODE, json!(mode::SEARCH)),
            KeywordType::Hashtag(hashtag) => options
                .with_query(hashtag.clone())
                .with_extra_value(extra_keys::MODE, json!(mode::HASHTAG)),
            KeywordType::UserId(handle) => options
                .with_query(handle.clone())
                .with_extra_value(extra_keys::MODE, json!(mode::HANDLE)),
            KeywordType::SecUserId(rest_id) => options
                .with_query(rest_id.clone())
                .with_extra_value(extra_keys::MODE, json!(mode::REST_ID)),
            KeywordType::ContentId(tweet_id) => options
                .with_query(tweet_id.clone())
                .with_extra_value(extra_keys::MODE, json!(mode::TWEET_ID)),
        };

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
        &self.default_region
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
    /// Lists search
    pub const LISTS: &str = "Lists";
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
        let mut config = TaskConfig::new(1, "twitter").with_max_videos(50);
        config
            .extra
            .insert(extra_keys::SEARCH_TYPE.to_string(), json!("top"));
        let keyword = KeywordType::Search("rust".to_string());

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.query, "rust");
        assert_eq!(options.count, 50);
        assert_eq!(
            options.extra.get(extra_keys::MODE).and_then(|v| v.as_str()),
            Some(mode::SEARCH)
        );
        assert_eq!(
            options
                .extra
                .get(extra_keys::SEARCH_TYPE)
                .and_then(|v| v.as_str()),
            Some(search_type::TOP)
        );
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

    #[test]
    fn test_build_search_options_for_tweet_id_mode() {
        let strategy = TwitterStrategy::new();
        let config = TaskConfig::new(1, "twitter").with_max_videos(1);
        let keyword = strategy.parse_keyword("twitter_tweet_id:1808168603721650364");

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.query, "1808168603721650364");
        assert_eq!(
            options.extra.get(extra_keys::MODE).and_then(|value| value.as_str()),
            Some(mode::TWEET_ID)
        );
        assert_eq!(
            options
                .extra
                .get(extra_keys::SEARCH_TYPE)
                .and_then(|value| value.as_str()),
            Some(search_type::LATEST)
        );
    }
}
