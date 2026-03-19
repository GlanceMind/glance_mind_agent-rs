//! Facebook Platform Strategy
//!
//! Handles Facebook-specific keyword parsing, search options, and prompt formatting.

use serde_json::{json, Value};

use crate::config::platform::get_platform_id;
use crate::domain::{Comment, Content, KeywordType, SearchOptions, TaskConfig};
use crate::strategies::PlatformStrategy;

pub mod extra_keys {
    pub const MODE: &str = "mode";
    pub const SEARCH_TYPE: &str = "search_type";
    pub const RECENT_POSTS: &str = "recent_posts";
    pub const LOCATION: &str = "location";
    pub const START_DATE: &str = "start_date";
    pub const END_DATE: &str = "end_date";
    pub const POST_LOOKUP_ID: &str = "post_lookup_id";
    pub const POST_URL: &str = "post_url";
}

pub mod mode {
    pub const KEYWORD: &str = "keyword";
    pub const PAGE: &str = "page";
    pub const POST_URL: &str = "post_url";
}

/// Facebook platform strategy implementation
pub struct FacebookStrategy {
    default_region: String,
    default_search_type: String,
}

impl FacebookStrategy {
    /// Create a new Facebook strategy with default settings.
    pub fn new() -> Self {
        Self {
            default_region: "US".to_string(),
            default_search_type: "posts".to_string(),
        }
    }

    fn apply_extra_option(
        options: SearchOptions,
        config: &TaskConfig,
        key: &str,
        default: Option<Value>,
    ) -> SearchOptions {
        if let Some(value) = config.extra.get(key) {
            options.with_extra_value(key, value.clone())
        } else if let Some(value) = default {
            options.with_extra_value(key, value)
        } else {
            options
        }
    }

    fn extract_query_param(url: &str, key: &str) -> Option<String> {
        let query = url.split_once('?')?.1;
        for pair in query.split('&') {
            let (name, value) = pair.split_once('=')?;
            if name == key && !value.is_empty() {
                return Some(value.to_string());
            }
        }
        None
    }

    fn extract_path_id(url: &str, marker: &str) -> Option<String> {
        let (_, rest) = url.split_once(marker)?;
        let id = rest.split('/').find(|segment| !segment.is_empty())?;
        Some(id.to_string())
    }

    pub(crate) fn extract_post_lookup_id(url_or_id: &str) -> Option<String> {
        let candidate = url_or_id.trim();
        if candidate.is_empty() {
            return None;
        }
        if !candidate.contains("facebook.com") {
            return Some(candidate.to_string());
        }

        Self::extract_query_param(candidate, "story_fbid")
            .or_else(|| Self::extract_query_param(candidate, "fbid"))
            .or_else(|| Self::extract_path_id(candidate, "/posts/"))
            .or_else(|| Self::extract_path_id(candidate, "/permalink/"))
            .or_else(|| Self::extract_path_id(candidate, "/reel/"))
            .or_else(|| Self::extract_path_id(candidate, "/videos/"))
    }
}

impl Default for FacebookStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformStrategy for FacebookStrategy {
    fn name(&self) -> &str {
        "facebook"
    }

    fn platform_id(&self) -> i32 {
        get_platform_id(self.name())
    }

    fn parse_keyword(&self, keyword: &str) -> KeywordType {
        let keyword = keyword.trim();

        if let Some(page) = keyword.strip_prefix("facebook_page:") {
            return KeywordType::UserId(page.to_string());
        }

        if let Some(url) = keyword.strip_prefix("facebook_post_url:") {
            return KeywordType::ContentId(url.to_string());
        }

        if keyword.starts_with("http://") || keyword.starts_with("https://") {
            return KeywordType::ContentId(keyword.to_string());
        }

        KeywordType::Search(keyword.to_string())
    }

    fn build_search_options(&self, config: &TaskConfig, keyword: &KeywordType) -> SearchOptions {
        let region = config
            .region
            .clone()
            .unwrap_or_else(|| self.default_region.clone());
        let count = config.max_videos.map(|v| v.min(20) as u32).unwrap_or(10);

        let mut options = SearchOptions::new(keyword.value().to_string())
            .with_platform(self.name())
            .with_region(region)
            .with_count(count);

        options = Self::apply_extra_option(
            options,
            config,
            extra_keys::SEARCH_TYPE,
            Some(json!(self.default_search_type)),
        );
        options = Self::apply_extra_option(options, config, extra_keys::RECENT_POSTS, None);
        options = Self::apply_extra_option(options, config, extra_keys::LOCATION, None);
        options = Self::apply_extra_option(options, config, extra_keys::START_DATE, None);
        options = Self::apply_extra_option(options, config, extra_keys::END_DATE, None);

        match keyword {
            KeywordType::UserId(page_identifier) | KeywordType::SecUserId(page_identifier) => {
                options = options
                    .with_query(page_identifier.clone())
                    .with_extra_value(extra_keys::MODE, json!(mode::PAGE));
            }
            KeywordType::ContentId(url_or_id) => {
                let lookup_id = Self::extract_post_lookup_id(url_or_id)
                    .unwrap_or_else(|| url_or_id.to_string());
                options = options
                    .with_query(lookup_id.clone())
                    .with_extra_value(extra_keys::MODE, json!(mode::POST_URL))
                    .with_extra_value(extra_keys::POST_LOOKUP_ID, json!(lookup_id));
                if url_or_id.contains("facebook.com") {
                    options = options.with_extra_value(extra_keys::POST_URL, json!(url_or_id));
                }
            }
            KeywordType::Search(_) | KeywordType::Hashtag(_) => {
                options = options.with_extra_value(extra_keys::MODE, json!(mode::KEYWORD));
            }
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

        prompt.push_str("## Facebook Post Information\n\n");
        prompt.push_str(&format!("**Post ID**: {}\n", content.content_id));
        prompt.push_str(&format!("**Author**: {}\n", content.author));
        if let Some(ref name) = content.author_name {
            prompt.push_str(&format!("**Author Name**: {}\n", name));
        }
        prompt.push_str(&format!("**Message**: {}\n", content.description));
        prompt.push_str(&format!(
            "**Engagement**: {} reactions, {} comments, {} shares\n",
            content.engagement.likes, content.engagement.comments, content.engagement.shares
        ));
        if let Some(ref url) = content.url {
            prompt.push_str(&format!("**URL**: {}\n", url));
        }
        prompt.push('\n');

        if !context.is_empty() {
            prompt.push_str("## Business Context\n\n");
            prompt.push_str(context);
            prompt.push_str("\n\n");
        }

        prompt.push_str("## Comments To Analyze\n\n");
        for (index, comment) in comments.iter().enumerate() {
            prompt.push_str(&format!("### Comment {}\n", index + 1));
            prompt.push_str(&format!("- **ID**: {}\n", comment.comment_id));
            prompt.push_str(&format!("- **Author**: {}", comment.author));
            if let Some(ref name) = comment.author_name {
                prompt.push_str(&format!(" ({})", name));
            }
            prompt.push('\n');
            prompt.push_str(&format!("- **Text**: {}\n", comment.text));
            prompt.push_str(&format!("- **Reactions**: {}\n", comment.likes));
            if comment.is_reply {
                prompt.push_str("- **Type**: Reply\n");
            }
            prompt.push('\n');
        }

        prompt.push_str("## Analysis Instructions\n\n");
        prompt.push_str("For each Facebook comment, provide:\n");
        prompt.push_str("1. **Intent**: question, feedback, purchase_intent, complaint, compliment, or other.\n");
        prompt.push_str("2. **Sentiment**: positive, neutral, or negative.\n");
        prompt.push_str("3. **Suggested Reply**: a natural reply that fits Facebook discussion style.\n");
        prompt.push_str("4. **Reason**: a short explanation for the reply choice.\n\n");
        prompt.push_str("Return JSON with one analysis object per comment.\n");

        prompt
    }

    fn default_region(&self) -> &str {
        &self.default_region
    }

    fn max_videos_per_search(&self) -> u32 {
        20
    }

    fn max_comments_per_video(&self) -> u32 {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_regular_search() {
        let strategy = FacebookStrategy::new();
        let keyword = strategy.parse_keyword("travel tips");
        assert!(matches!(keyword, KeywordType::Search(_)));
        assert_eq!(keyword.value(), "travel tips");
    }

    #[test]
    fn test_parse_page_keyword() {
        let strategy = FacebookStrategy::new();
        let keyword = strategy.parse_keyword("facebook_page:NatGeoMuseum");
        assert!(matches!(keyword, KeywordType::UserId(_)));
        assert_eq!(keyword.value(), "NatGeoMuseum");
    }

    #[test]
    fn test_parse_post_url_keyword() {
        let strategy = FacebookStrategy::new();
        let keyword = strategy.parse_keyword(
            "facebook_post_url:https://www.facebook.com/NatGeoMuseum/posts/pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl",
        );
        assert!(matches!(keyword, KeywordType::ContentId(_)));
        assert!(keyword.value().contains("facebook.com"));
    }

    #[test]
    fn test_extract_numeric_post_lookup_id() {
        let lookup = FacebookStrategy::extract_post_lookup_id(
            "https://www.facebook.com/groups/191706144748433/posts/1923718391547191/",
        );
        assert_eq!(lookup.as_deref(), Some("1923718391547191"));
    }

    #[test]
    fn test_extract_pfbid_post_lookup_id() {
        let lookup = FacebookStrategy::extract_post_lookup_id(
            "https://www.facebook.com/NatGeoMuseum/posts/pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl",
        );
        assert_eq!(
            lookup.as_deref(),
            Some("pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl")
        );
    }

    #[test]
    fn test_build_page_search_options() {
        let strategy = FacebookStrategy::new();
        let mut config = TaskConfig::new(100, "facebook")
            .with_keywords(vec!["facebook_page:NatGeoMuseum".to_string()])
            .with_max_videos(3)
            .with_region("US");
        config
            .extra
            .insert(extra_keys::RECENT_POSTS.to_string(), json!(true));

        let keyword = strategy.parse_keyword("facebook_page:NatGeoMuseum");
        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(options.query, "NatGeoMuseum");
        assert_eq!(options.count, 3);
        assert_eq!(
            options.extra.get(extra_keys::MODE).and_then(|v| v.as_str()),
            Some(mode::PAGE)
        );
        assert_eq!(
            options
                .extra
                .get(extra_keys::SEARCH_TYPE)
                .and_then(|v| v.as_str()),
            Some("posts")
        );
        assert_eq!(
            options
                .extra
                .get(extra_keys::RECENT_POSTS)
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_build_post_url_search_options() {
        let strategy = FacebookStrategy::new();
        let config = TaskConfig::new(100, "facebook").with_region("US");
        let keyword = strategy.parse_keyword(
            "facebook_post_url:https://www.facebook.com/NatGeoMuseum/posts/pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl",
        );
        let options = strategy.build_search_options(&config, &keyword);

        assert_eq!(
            options.extra.get(extra_keys::MODE).and_then(|v| v.as_str()),
            Some(mode::POST_URL)
        );
        assert_eq!(
            options
                .extra
                .get(extra_keys::POST_LOOKUP_ID)
                .and_then(|v| v.as_str()),
            Some("pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl")
        );
    }
}
