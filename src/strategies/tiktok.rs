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
        let count = config.max_videos.map(|v| v.max(0) as u32).unwrap_or(10);
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
    use crate::pagination::platform_page_cap;

    // ── M3-T1 测试集 ──────────────────────────────────────────────────────────
    //
    // 现状核对结论(2026-06-12):
    //   - tiktok.rs:98 已是 `v.max(0) as u32`(PR #5 删除 cap),count 为总量语义。
    //   - strategies/mod.rs 无 `extra_keys::PAGE_SIZE`(M3 新增),extra 写入不存在。
    //
    // 红/绿预期:
    //   测试 1/2 允许先绿(RED 前提被 PR #5 消解;AG-006 金丝雀:临时恢复 .min(20) 须红)。
    //   测试 3 真红(extra 无 "page_size" 键,PAGE_SIZE 写入逻辑未实现)。
    //   测试 4/5 允许先绿。

    /// M3-T1 测试 1: max_videos=50 → options.count == 50
    /// 允许先绿 + AG-006 金丝雀(临时恢复 .min(20) 须红);
    /// RED 前提已被 PR #5 消解(cap 行在 main 删除)。
    #[test]
    fn count_carries_total_max_videos() {
        let strategy = TikTokStrategy::new();
        let config = TaskConfig::new(1, "tiktok").with_max_videos(50);
        let keyword = KeywordType::Search("travel".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(
            options.count, 50,
            "count 应携带总量 50,不得被截断到 20(R-001/T-001 tiktok;PR #5 删 cap)"
        );
    }

    /// M3-T1 测试 2: max_videos=137 → options.count == 137(防换任何其它硬上限)
    /// 允许先绿 + AG-006 金丝雀;RED 前提同测试 1,已被 PR #5 消解。
    #[test]
    fn count_arbitrary_total_not_capped() {
        let strategy = TikTokStrategy::new();
        let config = TaskConfig::new(1, "tiktok").with_max_videos(137);
        let keyword = KeywordType::Search("travel".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(
            options.count, 137,
            "count 应携带总量 137,禁止任何总量 cap(R-001/T-001 tiktok;PR #5 消解)"
        );
    }

    /// M3-T1 测试 3: page_size_hint → extra["page_size"] 写入,clamp 到 platform_page_cap=20
    ///
    /// 真红:main 无 PAGE_SIZE extra 写入逻辑 → extra 无 "page_size" 键。
    /// GREEN:实现者在 build_search_options 写入 extra["page_size"](clamp 到 20)。
    #[test]
    fn page_size_extra_from_hint_clamped() {
        let strategy = TikTokStrategy::new();
        let cap = platform_page_cap("tiktok"); // D4 冻结:20

        // Case A: hint=7 → extra["page_size"] == 7(在 cap 之内)
        let mut config_a = TaskConfig::new(1, "tiktok").with_max_videos(50);
        config_a.page_size_hint = Some(7);
        let keyword = KeywordType::Search("travel".to_string());
        let options_a = strategy.build_search_options(&config_a, &keyword);

        assert!(
            options_a.extra.get("page_size").is_some(),
            "page_size_hint=Some(7) 时 extra[\"page_size\"] 必须存在(M3 §2.1/D4)"
        );
        assert_eq!(
            options_a.extra.get("page_size").and_then(|v| v.as_u64()),
            Some(7u64),
            "page_size_hint=Some(7) → extra[\"page_size\"]==7(未超 cap={cap})"
        );

        // Case B: hint=500 → extra["page_size"] == cap(20)
        let mut config_b = TaskConfig::new(1, "tiktok").with_max_videos(100);
        config_b.page_size_hint = Some(500);
        let options_b = strategy.build_search_options(&config_b, &keyword);

        assert!(
            options_b.extra.get("page_size").is_some(),
            "page_size_hint=Some(500) 时 extra[\"page_size\"] 必须存在"
        );
        assert_eq!(
            options_b.extra.get("page_size").and_then(|v| v.as_u64()),
            Some(cap as u64),
            "page_size_hint=Some(500) → extra[\"page_size\"]==cap={cap}(clamp)"
        );
    }

    /// M3-T1 测试 4: page_size_hint=None → extra 无 "page_size" 键
    /// 允许先绿(None 情形下不写入 extra;适配器默认使用 cap=20)。
    #[test]
    fn no_hint_no_page_size_extra() {
        let strategy = TikTokStrategy::new();
        let config = TaskConfig::new(1, "tiktok").with_max_videos(50);
        // page_size_hint 默认为 None
        let keyword = KeywordType::Search("travel".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        assert!(
            options.extra.get("page_size").is_none(),
            "page_size_hint=None 时 extra 不得含 \"page_size\" 键(适配器默认 cap=20)"
        );
    }

    /// M3-T1 测试 5: max_videos=None → options.count == 10(现状缺省)
    /// 允许先绿;AG-006 由 AG-012 覆盖。
    #[test]
    fn missing_max_videos_default_unchanged() {
        let strategy = TikTokStrategy::new();
        let config = TaskConfig::new(1, "tiktok"); // max_videos=None
        let keyword = KeywordType::Search("travel".to_string());

        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(
            options.count, 10,
            "max_videos=None 时 count 应为现状缺省 10(不改变现有默认行为)"
        );
    }

    // ── 原有测试 ─────────────────────────────────────────────────────────────

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
