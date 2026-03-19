//! Domain entities - Platform-agnostic business models
//!
//! These entities represent the core business concepts that are independent
//! of any specific platform (TikTok, Instagram, etc.) or infrastructure.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::warn;

// ============================================================
// Content Entity
// ============================================================

/// Platform-agnostic content representation (video, post, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    /// Platform identifier (e.g., "tiktok", "instagram")
    pub platform: String,

    /// Platform-specific content ID
    pub content_id: String,

    /// Content author's username
    pub author: String,

    /// Author's display name
    pub author_name: Option<String>,

    /// Content description/caption
    pub description: String,

    /// URL to the content
    pub url: Option<String>,

    /// Engagement metrics
    pub engagement: Engagement,

    /// Creation timestamp (Unix epoch)
    pub created_at: Option<i64>,

    /// Raw platform-specific data for reference
    pub raw_data: Option<serde_json::Value>,
}

/// Engagement metrics for content
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Engagement {
    pub likes: i64,
    pub comments: i64,
    pub shares: i64,
    pub views: i64,
}

impl Content {
    /// Create a new Content instance
    #[must_use]
    pub fn new(platform: impl Into<String>, content_id: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            content_id: content_id.into(),
            author: String::new(),
            author_name: None,
            description: String::new(),
            url: None,
            engagement: Engagement::default(),
            created_at: None,
            raw_data: None,
        }
    }

    /// Builder method: set author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    /// Builder method: set author name
    pub fn with_author_name(mut self, name: impl Into<String>) -> Self {
        self.author_name = Some(name.into());
        self
    }

    /// Builder method: set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder method: set URL
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Builder method: set engagement
    pub fn with_engagement(mut self, engagement: Engagement) -> Self {
        self.engagement = engagement;
        self
    }

    /// Builder method: set created_at
    pub fn with_created_at(mut self, timestamp: i64) -> Self {
        self.created_at = Some(timestamp);
        self
    }

    /// Builder method: set raw_data
    pub fn with_raw_data(mut self, data: serde_json::Value) -> Self {
        self.raw_data = Some(data);
        self
    }
}

// ============================================================
// Comment Entity
// ============================================================

/// Platform-agnostic comment representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// Platform identifier
    pub platform: String,

    /// Platform-specific comment ID
    pub comment_id: String,

    /// ID of the content this comment belongs to
    pub content_id: String,

    /// Parent comment ID (if this is a reply)
    pub parent_id: Option<String>,

    /// Comment author's username
    pub author: String,

    /// Author's display name
    pub author_name: Option<String>,

    /// Author's user ID
    pub author_uid: Option<String>,

    /// Comment text content
    pub text: String,

    /// Number of likes on this comment
    pub likes: i64,

    /// Number of replies to this comment
    pub reply_count: i32,

    /// Creation timestamp (Unix epoch)
    pub created_at: Option<i64>,

    /// Detected language of the comment
    pub language: Option<String>,

    /// Whether this is a reply to another comment
    pub is_reply: bool,

    /// Raw platform-specific data
    pub raw_data: Option<serde_json::Value>,
}

impl Comment {
    /// Create a new Comment instance
    #[must_use]
    pub fn new(
        platform: impl Into<String>,
        comment_id: impl Into<String>,
        content_id: impl Into<String>,
    ) -> Self {
        Self {
            platform: platform.into(),
            comment_id: comment_id.into(),
            content_id: content_id.into(),
            parent_id: None,
            author: String::new(),
            author_name: None,
            author_uid: None,
            text: String::new(),
            likes: 0,
            reply_count: 0,
            created_at: None,
            language: None,
            is_reply: false,
            raw_data: None,
        }
    }

    /// Builder method: set author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    /// Builder method: set author name
    pub fn with_author_name(mut self, name: impl Into<String>) -> Self {
        self.author_name = Some(name.into());
        self
    }

    /// Builder method: set author uid
    pub fn with_author_uid(mut self, uid: impl Into<String>) -> Self {
        self.author_uid = Some(uid.into());
        self
    }

    /// Builder method: set text
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Builder method: set likes
    pub fn with_likes(mut self, likes: i64) -> Self {
        self.likes = likes;
        self
    }

    /// Builder method: set reply count
    pub fn with_reply_count(mut self, count: i32) -> Self {
        self.reply_count = count;
        self
    }

    /// Builder method: set as reply
    pub fn as_reply_to(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self.is_reply = true;
        self
    }

    /// Builder method: set created_at
    pub fn with_created_at(mut self, timestamp: i64) -> Self {
        self.created_at = Some(timestamp);
        self
    }

    /// Builder method: set language
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Builder method: set raw_data
    pub fn with_raw_data(mut self, data: serde_json::Value) -> Self {
        self.raw_data = Some(data);
        self
    }
}

// ============================================================
// Reply Suggestion Entity
// ============================================================

/// AI-generated reply suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplySuggestion {
    /// The comment this reply is for
    pub comment_id: String,

    /// Suggested reply text
    pub reply_text: Option<String>,

    /// Suggested DM text
    pub dm_text: Option<String>,

    /// Suggested post reply text
    pub post_reply_text: Option<String>,

    /// Reason/explanation for the suggestion
    pub reason: Option<String>,

    /// Confidence score (0.0 - 1.0)
    pub confidence: Option<f64>,

    /// Intent classification
    pub intent: Option<CommentIntent>,

    /// Sentiment analysis result
    pub sentiment: Option<Sentiment>,

    /// Number of tokens used for generation
    pub tokens_used: Option<i32>,

    /// Model used for generation
    pub model: Option<String>,
}

impl ReplySuggestion {
    /// Create a new ReplySuggestion
    #[must_use]
    pub fn new(comment_id: impl Into<String>) -> Self {
        Self {
            comment_id: comment_id.into(),
            reply_text: None,
            dm_text: None,
            post_reply_text: None,
            reason: None,
            confidence: None,
            intent: None,
            sentiment: None,
            tokens_used: None,
            model: None,
        }
    }

    /// Builder method: set reply text
    pub fn with_reply(mut self, text: impl Into<String>) -> Self {
        self.reply_text = Some(text.into());
        self
    }

    /// Builder method: set DM text
    pub fn with_dm(mut self, text: impl Into<String>) -> Self {
        self.dm_text = Some(text.into());
        self
    }

    /// Builder method: set post reply text
    pub fn with_post_reply(mut self, text: impl Into<String>) -> Self {
        self.post_reply_text = Some(text.into());
        self
    }

    /// Builder method: set reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Builder method: set intent
    pub fn with_intent(mut self, intent: CommentIntent) -> Self {
        self.intent = Some(intent);
        self
    }

    /// Builder method: set sentiment
    pub fn with_sentiment(mut self, sentiment: Sentiment) -> Self {
        self.sentiment = Some(sentiment);
        self
    }

    /// Builder method: set model info
    pub fn with_model_info(mut self, model: impl Into<String>, tokens: i32) -> Self {
        self.model = Some(model.into());
        self.tokens_used = Some(tokens);
        self
    }
}

// ============================================================
// Supporting Types
// ============================================================

/// Comment intent classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentIntent {
    /// Asking a question about the product/service
    Question,
    /// Expressing interest in purchasing
    PurchaseIntent,
    /// Requesting more information
    InformationRequest,
    /// Providing feedback or review
    Feedback,
    /// Expressing complaint or dissatisfaction
    Complaint,
    /// General praise or positive comment
    Praise,
    /// Casual conversation or chitchat
    Chitchat,
    /// Spam or irrelevant content
    Spam,
    /// Unknown or unclassified intent
    Unknown,
}

impl Default for CommentIntent {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Sentiment analysis result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentiment {
    Positive,
    Negative,
    Neutral,
}

impl Default for Sentiment {
    fn default() -> Self {
        Self::Neutral
    }
}

// ============================================================
// Concurrency Configuration
// ============================================================

/// Concurrency configuration for task processing
///
/// Can be customized per campaign/template or use global defaults.
/// Uses Tokio's lightweight async tasks (similar to Go goroutines).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Maximum concurrent tasks to process simultaneously (default: 5)
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,

    /// Maximum concurrent videos to process per task (default: 5)
    #[serde(default = "default_max_concurrent_videos")]
    pub max_concurrent_videos: usize,

    /// AI API call concurrency limit (default: 20)
    #[serde(default = "default_ai_concurrency")]
    pub ai_concurrency: usize,

    /// TikHub API call concurrency limit (default: 3)
    #[serde(default = "default_tikhub_concurrency")]
    pub tikhub_concurrency: usize,

    /// Minimum interval between AI API calls in milliseconds (default: 50)
    #[serde(default = "default_ai_min_interval_ms")]
    pub ai_min_interval_ms: u64,
}

fn default_max_concurrent_tasks() -> usize {
    5
}
fn default_max_concurrent_videos() -> usize {
    5
}
fn default_ai_concurrency() -> usize {
    20
}
fn default_tikhub_concurrency() -> usize {
    3
}
fn default_ai_min_interval_ms() -> u64 {
    50
}

/// Parse an environment variable with warning on invalid values
///
/// If the environment variable is set but cannot be parsed, logs a warning
/// and returns the default value.
fn parse_env_with_warning<T: FromStr>(env_var: &str, default: T) -> T {
    match std::env::var(env_var) {
        Ok(value) => match value.parse() {
            Ok(parsed) => parsed,
            Err(_) => {
                warn!(
                    env_var = %env_var,
                    value = %value,
                    default = ?std::any::type_name::<T>(),
                    "Invalid environment variable value, using default"
                );
                default
            }
        },
        Err(_) => default,
    }
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: default_max_concurrent_tasks(),
            max_concurrent_videos: default_max_concurrent_videos(),
            ai_concurrency: default_ai_concurrency(),
            tikhub_concurrency: default_tikhub_concurrency(),
            ai_min_interval_ms: default_ai_min_interval_ms(),
        }
    }
}

impl ConcurrencyConfig {
    /// Create a new ConcurrencyConfig with custom values
    pub fn new(
        max_concurrent_tasks: usize,
        max_concurrent_videos: usize,
        ai_concurrency: usize,
    ) -> Self {
        Self {
            max_concurrent_tasks,
            max_concurrent_videos,
            ai_concurrency,
            tikhub_concurrency: default_tikhub_concurrency(),
            ai_min_interval_ms: default_ai_min_interval_ms(),
        }
    }

    /// Create from environment variables
    ///
    /// If an environment variable is set but cannot be parsed, a warning is logged
    /// and the default value is used.
    pub fn from_env() -> Self {
        Self {
            max_concurrent_tasks: parse_env_with_warning(
                "AGENT_MAX_CONCURRENT_TASKS",
                default_max_concurrent_tasks(),
            ),
            max_concurrent_videos: parse_env_with_warning(
                "AGENT_MAX_CONCURRENT_VIDEOS",
                default_max_concurrent_videos(),
            ),
            ai_concurrency: parse_env_with_warning(
                "AGENT_AI_CONCURRENCY",
                default_ai_concurrency(),
            ),
            tikhub_concurrency: parse_env_with_warning(
                "AGENT_TIKHUB_CONCURRENCY",
                default_tikhub_concurrency(),
            ),
            ai_min_interval_ms: parse_env_with_warning(
                "AGENT_AI_MIN_INTERVAL_MS",
                default_ai_min_interval_ms(),
            ),
        }
    }

    /// Builder method: set max concurrent tasks
    pub fn with_max_concurrent_tasks(mut self, n: usize) -> Self {
        self.max_concurrent_tasks = n;
        self
    }

    /// Builder method: set max concurrent videos
    pub fn with_max_concurrent_videos(mut self, n: usize) -> Self {
        self.max_concurrent_videos = n;
        self
    }

    /// Builder method: set AI concurrency
    pub fn with_ai_concurrency(mut self, n: usize) -> Self {
        self.ai_concurrency = n;
        self
    }

    /// Builder method: set TikHub concurrency
    pub fn with_tikhub_concurrency(mut self, n: usize) -> Self {
        self.tikhub_concurrency = n;
        self
    }

    /// Builder method: set AI min interval
    pub fn with_ai_min_interval_ms(mut self, ms: u64) -> Self {
        self.ai_min_interval_ms = ms;
        self
    }
}

// ============================================================
// Task Configuration
// ============================================================

/// Task configuration for content crawling and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Campaign ID
    pub campaign_id: i32,

    /// Platform to crawl
    pub platform: String,

    /// Search keywords
    pub keywords: Vec<String>,

    /// Target region code
    pub region: Option<String>,

    /// Maximum number of videos to process
    pub max_videos: Option<i32>,

    /// Maximum comments per video
    pub max_comments_per_video: Option<i32>,

    /// Sort type for search results (TikTok: 0=relevance, 1=most_liked)
    pub sort_type: Option<u8>,

    /// Publish time filter (TikTok: 0=all, 1=day, 7=week, 30=month, 90=3months, 180=6months)
    pub publish_time: Option<u8>,

    /// Product/service description for AI context
    pub product_prompt: Option<String>,

    /// Target audience description
    pub target_audience: Option<String>,

    /// Reply strategy instructions
    pub reply_strategy: Option<String>,

    /// DM strategy instructions
    pub dm_strategy: Option<String>,

    /// Concurrency configuration (optional, uses global defaults if not set)
    #[serde(default)]
    pub concurrency: Option<ConcurrencyConfig>,

    /// Additional configuration
    pub extra: HashMap<String, serde_json::Value>,
}

impl TaskConfig {
    /// Create a new TaskConfig
    #[must_use]
    pub fn new(campaign_id: i32, platform: impl Into<String>) -> Self {
        Self {
            campaign_id,
            platform: platform.into(),
            keywords: Vec::new(),
            region: None,
            max_videos: None,
            max_comments_per_video: None,
            sort_type: None,
            publish_time: None,
            product_prompt: None,
            target_audience: None,
            reply_strategy: None,
            dm_strategy: None,
            concurrency: None,
            extra: HashMap::new(),
        }
    }

    /// Get effective concurrency config (task-level or default)
    pub fn effective_concurrency(&self) -> ConcurrencyConfig {
        self.concurrency.clone().unwrap_or_default()
    }

    /// Set concurrency configuration
    pub fn with_concurrency(mut self, config: ConcurrencyConfig) -> Self {
        self.concurrency = Some(config);
        self
    }

    /// Add a keyword
    pub fn add_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keywords.push(keyword.into());
        self
    }

    /// Set keywords
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Set region
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Set max videos
    pub fn with_max_videos(mut self, max: i32) -> Self {
        self.max_videos = Some(max);
        self
    }

    /// Set max comments per video
    pub fn with_max_comments_per_video(mut self, max: i32) -> Self {
        self.max_comments_per_video = Some(max);
        self
    }

    /// Set sort type (TikTok: 0=relevance, 1=most_liked)
    pub fn with_sort_type(mut self, sort_type: u8) -> Self {
        self.sort_type = Some(sort_type);
        self
    }

    /// Set publish time filter (TikTok: 0=all, 1=day, 7=week, 30=month, 90=3months, 180=6months)
    pub fn with_publish_time(mut self, publish_time: u8) -> Self {
        self.publish_time = Some(publish_time);
        self
    }
}

// ============================================================
// Task Result
// ============================================================

/// Result of processing a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID
    pub task_id: i64,

    /// Whether the task succeeded
    pub success: bool,

    /// Number of contents processed
    pub contents_processed: i32,

    /// Number of comments processed
    pub comments_processed: i32,

    /// Number of AI analyses generated
    pub analyses_generated: i32,

    /// Error message if failed
    pub error: Option<String>,

    /// Processing duration in milliseconds
    pub duration_ms: Option<u64>,
}

impl TaskResult {
    /// Create a success result
    pub fn success(task_id: i64) -> Self {
        Self {
            task_id,
            success: true,
            contents_processed: 0,
            comments_processed: 0,
            analyses_generated: 0,
            error: None,
            duration_ms: None,
        }
    }

    /// Create a failure result
    pub fn failure(task_id: i64, error: impl Into<String>) -> Self {
        Self {
            task_id,
            success: false,
            contents_processed: 0,
            comments_processed: 0,
            analyses_generated: 0,
            error: Some(error.into()),
            duration_ms: None,
        }
    }

    /// Update counts
    pub fn with_counts(mut self, contents: i32, comments: i32, analyses: i32) -> Self {
        self.contents_processed = contents;
        self.comments_processed = comments;
        self.analyses_generated = analyses;
        self
    }

    /// Set duration
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }
}

// ============================================================
// Search Options
// ============================================================

/// Options for content search
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Search query/keyword
    pub query: String,

    /// Platform name (e.g., "tiktok", "instagram", "reddit", "twitter")
    pub platform: Option<String>,

    /// Region code
    pub region: Option<String>,

    /// Number of results to fetch
    pub count: u32,

    /// Pagination offset
    pub offset: u32,

    /// Sort type (platform-specific)
    pub sort_type: Option<u8>,

    /// Time filter (platform-specific)
    pub publish_time: Option<u8>,

    /// Platform-specific extra options
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl SearchOptions {
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            platform: None,
            region: None,
            count: 10,
            offset: 0,
            sort_type: None,
            publish_time: None,
            extra: HashMap::new(),
        }
    }

    pub fn with_platform(mut self, platform: impl Into<String>) -> Self {
        self.platform = Some(platform.into());
        self
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = offset;
        self
    }

    /// Attach a platform-specific extra option
    pub fn with_extra_value(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.extra.insert(key.into(), value);
        self
    }

    /// Attach multiple platform-specific extra options
    pub fn with_extra(mut self, extra: HashMap<String, serde_json::Value>) -> Self {
        self.extra.extend(extra);
        self
    }

    /// Create a copy with a different query (efficient for modifying just the query)
    pub fn with_query(&self, query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            platform: self.platform.clone(),
            region: self.region.clone(),
            count: self.count,
            offset: self.offset,
            sort_type: self.sort_type,
            publish_time: self.publish_time,
            extra: self.extra.clone(),
        }
    }
}

// ============================================================
// Keyword Type
// ============================================================

/// Parsed keyword type for platform-specific handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordType {
    /// Regular search keyword
    Search(String),

    /// User's unique ID (e.g., @username)
    UserId(String),

    /// User's secure ID (platform-specific)
    SecUserId(String),

    /// Direct video/content ID
    ContentId(String),

    /// Hashtag search
    Hashtag(String),
}

impl KeywordType {
    /// Get the raw value
    pub fn value(&self) -> &str {
        match self {
            KeywordType::Search(v) => v,
            KeywordType::UserId(v) => v,
            KeywordType::SecUserId(v) => v,
            KeywordType::ContentId(v) => v,
            KeywordType::Hashtag(v) => v,
        }
    }

    /// Check if this is a user-based keyword
    pub fn is_user_based(&self) -> bool {
        matches!(self, KeywordType::UserId(_) | KeywordType::SecUserId(_))
    }

    /// Check if this is a content-based keyword
    pub fn is_content_based(&self) -> bool {
        matches!(self, KeywordType::ContentId(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_builder() {
        let content = Content::new("tiktok", "123456")
            .with_author("testuser")
            .with_description("Test video")
            .with_engagement(Engagement {
                likes: 100,
                comments: 10,
                shares: 5,
                views: 1000,
            });

        assert_eq!(content.platform, "tiktok");
        assert_eq!(content.content_id, "123456");
        assert_eq!(content.author, "testuser");
        assert_eq!(content.engagement.likes, 100);
    }

    #[test]
    fn test_comment_builder() {
        let comment = Comment::new("tiktok", "c123", "v456")
            .with_author("commenter")
            .with_text("Great video!")
            .with_likes(50)
            .as_reply_to("c100");

        assert_eq!(comment.comment_id, "c123");
        assert_eq!(comment.content_id, "v456");
        assert!(comment.is_reply);
        assert_eq!(comment.parent_id, Some("c100".to_string()));
    }

    #[test]
    fn test_reply_suggestion_builder() {
        let suggestion = ReplySuggestion::new("c123")
            .with_reply("Thank you!")
            .with_intent(CommentIntent::Praise)
            .with_sentiment(Sentiment::Positive);

        assert_eq!(suggestion.comment_id, "c123");
        assert_eq!(suggestion.reply_text, Some("Thank you!".to_string()));
        assert_eq!(suggestion.intent, Some(CommentIntent::Praise));
    }

    #[test]
    fn test_task_result() {
        let result = TaskResult::success(1)
            .with_counts(10, 100, 50)
            .with_duration(5000);

        assert!(result.success);
        assert_eq!(result.contents_processed, 10);
        assert_eq!(result.comments_processed, 100);
        assert_eq!(result.analyses_generated, 50);
    }
}
