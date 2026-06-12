//! Reddit Platform Strategy
//!
//! Handles Reddit-specific keyword parsing, search options, and prompt formatting.

use crate::config::platform::get_platform_id;
use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};
use crate::strategies::PlatformStrategy;

/// Reddit platform strategy implementation
pub struct RedditStrategy {
    /// Default sort order for search results
    default_sort: String,
    /// Whether to include NSFW content
    #[allow(dead_code)]
    allow_nsfw: bool,
}

impl RedditStrategy {
    /// Create a new Reddit strategy with default settings
    pub fn new() -> Self {
        Self {
            default_sort: "relevance".to_string(),
            allow_nsfw: false,
        }
    }

    /// Create with custom settings
    pub fn with_options(sort: impl Into<String>, allow_nsfw: bool) -> Self {
        Self {
            default_sort: sort.into(),
            allow_nsfw,
        }
    }
}

impl Default for RedditStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformStrategy for RedditStrategy {
    fn name(&self) -> &str {
        "reddit"
    }

    fn platform_id(&self) -> i32 {
        get_platform_id(self.name())
    }

    fn parse_keyword(&self, keyword: &str) -> KeywordType {
        let keyword = keyword.trim();

        // Check for Reddit-specific prefixes
        if let Some(username) = keyword.strip_prefix("reddit_user:") {
            let username = username.trim_start_matches("u/");
            return KeywordType::UserId(username.to_string());
        }

        if let Some(post_id) = keyword.strip_prefix("reddit_post:") {
            // Remove t3_ prefix if present
            let post_id = post_id.trim_start_matches("t3_");
            return KeywordType::ContentId(post_id.to_string());
        }

        if let Some(subreddit) = keyword.strip_prefix("reddit_subreddit:") {
            let subreddit = subreddit.trim_start_matches("r/");
            // Treat subreddit search as a hashtag-like search
            return KeywordType::Hashtag(format!("subreddit:{}", subreddit));
        }

        // Check for u/ prefix (username)
        if let Some(username) = keyword.strip_prefix("u/") {
            return KeywordType::UserId(username.to_string());
        }

        // Check for r/ prefix (subreddit)
        if let Some(subreddit) = keyword.strip_prefix("r/") {
            return KeywordType::Hashtag(format!("subreddit:{}", subreddit));
        }

        // Check if it looks like a Reddit post ID (6-7 alphanumeric chars)
        if (6..=8).contains(&keyword.len()) && keyword.chars().all(|c| c.is_alphanumeric()) {
            return KeywordType::ContentId(keyword.to_string());
        }

        // Default: regular search
        KeywordType::Search(keyword.to_string())
    }

    fn build_search_options(&self, config: &TaskConfig, keyword: &KeywordType) -> SearchOptions {
        let query = keyword.value().to_string();

        let mut options = SearchOptions::new(query).with_platform(self.name());

        // Store sort type in region field for now
        options = options.with_region(self.default_sort.clone());

        // Set count from config
        let count = config.max_videos.map(|v| v.min(100) as u32).unwrap_or(25);
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

        // Post context
        prompt.push_str("## Reddit Post Information\n\n");
        prompt.push_str(&format!("**Post ID**: {}\n", content.content_id));
        prompt.push_str(&format!("**Author**: u/{}\n", content.author));

        // Parse title and body from description (format: "title\n\nbody")
        let parts: Vec<&str> = content.description.splitn(2, "\n\n").collect();
        let title = parts.first().unwrap_or(&"");
        let body = parts.get(1).unwrap_or(&"");

        prompt.push_str(&format!("**Title**: {}\n", title));
        if !body.is_empty() {
            prompt.push_str(&format!("**Body**: {}\n", body));
        }
        prompt.push_str(&format!(
            "**Engagement**: {} upvotes, {} comments\n",
            content.engagement.likes, content.engagement.comments
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
            prompt.push_str(&format!("- **User**: u/{}\n", comment.author));
            prompt.push_str(&format!("- **Text**: {}\n", comment.text));
            prompt.push_str(&format!("- **Score**: {} points\n", comment.likes));
            if comment.is_reply {
                prompt.push_str("- **Type**: Reply to another comment\n");
            }
            prompt.push('\n');
        }

        // Analysis instructions
        prompt.push_str("## Analysis Instructions\n\n");
        prompt.push_str("For each Reddit comment, please provide:\n");
        prompt.push_str("1. **Intent**: What is the commenter trying to convey? (question, discussion, feedback, support, criticism, etc.)\n");
        prompt.push_str("2. **Sentiment**: Is the comment positive, negative, or neutral?\n");
        prompt.push_str("3. **Suggested Reply**: A thoughtful reply that fits Reddit's discussion-oriented culture. Be informative and genuine.\n");
        prompt.push_str("4. **Reason**: Brief explanation of why this reply is appropriate.\n\n");
        prompt.push_str("Note: Reddit values authenticity and dislikes obvious marketing. Keep replies helpful and genuine.\n");
        prompt.push_str("Please respond in JSON format with an array of analysis objects.\n");

        prompt
    }

    fn default_region(&self) -> &str {
        &self.default_sort
    }

    fn max_videos_per_search(&self) -> u32 {
        100 // Reddit API limit
    }

    fn max_comments_per_video(&self) -> u32 {
        50 // Default limit per request
    }
}

/// Reddit-specific sort options
pub mod sort_type {
    /// Sort by relevance (default)
    pub const RELEVANCE: &str = "relevance";
    /// Sort by hot
    pub const HOT: &str = "hot";
    /// Sort by top
    pub const TOP: &str = "top";
    /// Sort by newest
    pub const NEW: &str = "new";
    /// Sort by most comments
    pub const COMMENTS: &str = "comments";
}

/// Reddit-specific time filters
pub mod time_filter {
    /// Last hour
    pub const HOUR: &str = "hour";
    /// Last 24 hours
    pub const DAY: &str = "day";
    /// Last week
    pub const WEEK: &str = "week";
    /// Last month
    pub const MONTH: &str = "month";
    /// Last year
    pub const YEAR: &str = "year";
    /// All time
    pub const ALL: &str = "all";
}

#[cfg(test)]
mod tests {
    use super::*;

    // M4-T1: total-count tests (RED batch: tests 1+2 RED until cap removed; tests 3+4 allowed-green)
    // DR-15: assertion text matches plan verbatim; cap line reddit.rs:100 untouched by this agent.

    /// T1-R-1: max_videos=150 → options.count==150
    /// Selects 150 > legacy cap 100 to expose truncation. Expected RED: left: 100, right: 150.
    #[test]
    fn count_carries_total_max_videos() {
        let strategy = RedditStrategy::new();
        let config = TaskConfig::new(1, "reddit").with_max_videos(150);
        let keyword = KeywordType::Search("rust".to_string());
        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.count, 150);
    }

    /// T1-R-2: max_videos=237 → options.count==237
    /// Expected RED: left: 100, right: 237.
    #[test]
    fn count_arbitrary_total_not_capped() {
        let strategy = RedditStrategy::new();
        let config = TaskConfig::new(1, "reddit").with_max_videos(237);
        let keyword = KeywordType::Search("rust".to_string());
        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.count, 237);
    }

    /// T1-R-3: max_videos=50 → options.count==50
    /// Regression; allowed-green (AG-006, AG-012 覆盖, cap 行在 diff 内).
    #[test]
    fn count_below_legacy_cap_unchanged() {
        let strategy = RedditStrategy::new();
        let config = TaskConfig::new(1, "reddit").with_max_videos(50);
        let keyword = KeywordType::Search("rust".to_string());
        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.count, 50);
    }

    /// T1-R-4: max_videos=None → options.count==25 (reddit default)
    /// Allowed-green (AG-006).
    #[test]
    fn missing_max_videos_default_unchanged() {
        let strategy = RedditStrategy::new();
        let config = TaskConfig::new(1, "reddit");
        let keyword = KeywordType::Search("rust".to_string());
        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.count, 25);
    }

    #[test]
    fn test_parse_regular_search() {
        let strategy = RedditStrategy::new();
        let keyword = strategy.parse_keyword("rust programming");
        assert!(matches!(keyword, KeywordType::Search(_)));
        assert_eq!(keyword.value(), "rust programming");
    }

    #[test]
    fn test_parse_username() {
        let strategy = RedditStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("reddit_user:u/testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");

        // With u/ prefix only
        let keyword = strategy.parse_keyword("u/testuser");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "testuser");
    }

    #[test]
    fn test_parse_subreddit() {
        let strategy = RedditStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("reddit_subreddit:r/rust");
        assert!(matches!(keyword, KeywordType::Hashtag(_)));
        assert_eq!(keyword.value(), "subreddit:rust");

        // With r/ prefix only
        let keyword = strategy.parse_keyword("r/rust");
        assert!(matches!(keyword, KeywordType::Hashtag(_)));
        assert_eq!(keyword.value(), "subreddit:rust");
    }

    #[test]
    fn test_parse_post_id() {
        let strategy = RedditStrategy::new();

        // With prefix
        let keyword = strategy.parse_keyword("reddit_post:t3_abc123");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
        assert_eq!(keyword.value(), "abc123");

        // Short alphanumeric string
        let keyword = strategy.parse_keyword("abc123d");
        assert!(matches!(keyword, KeywordType::ContentId(_)));
    }

    #[test]
    fn test_build_search_options() {
        let strategy = RedditStrategy::new();
        let config = TaskConfig::new(1, "reddit").with_max_videos(50);
        let keyword = KeywordType::Search("rust".to_string());

        let options = strategy.build_search_options(&config, &keyword);
        assert_eq!(options.query, "rust");
        assert_eq!(options.count, 50);
    }

    #[test]
    fn test_format_analysis_prompt() {
        let strategy = RedditStrategy::new();

        let content = Content::new("reddit", "abc123")
            .with_author("testuser")
            .with_description("Check out my Rust project\n\nI built a cool CLI tool");

        let comments = vec![Comment::new("reddit", "c1", "abc123")
            .with_author("commenter1")
            .with_text("Nice work! How did you handle error handling?")];

        let prompt =
            strategy.format_analysis_prompt(&content, &comments, "We're building developer tools");

        assert!(prompt.contains("Reddit Post Information"));
        assert!(prompt.contains("u/testuser"));
        assert!(prompt.contains("error handling"));
        assert!(prompt.contains("developer tools"));
        assert!(prompt.contains("authenticity"));
    }

    #[test]
    fn test_is_user_keyword() {
        let strategy = RedditStrategy::new();

        assert!(strategy.is_user_keyword("u/username"));
        assert!(strategy.is_user_keyword("reddit_user:user"));
        assert!(!strategy.is_user_keyword("rust"));
        assert!(!strategy.is_user_keyword("r/rust"));
    }

    #[test]
    fn test_is_content_keyword() {
        let strategy = RedditStrategy::new();

        assert!(strategy.is_content_keyword("reddit_post:abc123"));
        assert!(strategy.is_content_keyword("abc123d"));
        assert!(!strategy.is_content_keyword("u/username"));
        assert!(!strategy.is_content_keyword("rust programming"));
    }
}
