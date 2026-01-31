//! GlanceMind Protocol - Auto-generated from Protocol Buffers
//!
//! This module provides protocol definitions for:
//! - Queue messages (Scheduler <-> Agent)
//! - REST API (API <-> Executor)
//!
//! IMPORTANT: This file is auto-synced from glance_mind_protocol project.
//! Do NOT edit manually. Run `make sync` in glance_mind_protocol to update.

#![allow(clippy::derive_partial_eq_without_eq)]

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ============================================================
// Common Types (from common.proto)
// ============================================================

/// Supported social media platforms
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum Platform {
    Unspecified = 0,
    Reddit = 1,
    Tiktok = 2,
    Facebook = 3,
    Instagram = 4,
    Twitter = 5,
    Youtube = 6,
}

/// Type of data to crawl
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum DataType {
    Unspecified = 0,
    VideoContent = 1,
    VideoMetadata = 2,
    VideoComments = 3,
    KeywordSearch = 4,
}

/// Time range filter for search
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum TimeRange {
    Unspecified = 0,
    AllTime = 1,
    Last24h = 2,
    Last7d = 3,
    Last30d = 4,
    Last180d = 5,
}

impl Platform {
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "PLATFORM_UNSPECIFIED",
            Self::Reddit => "PLATFORM_REDDIT",
            Self::Tiktok => "PLATFORM_TIKTOK",
            Self::Facebook => "PLATFORM_FACEBOOK",
            Self::Instagram => "PLATFORM_INSTAGRAM",
            Self::Twitter => "PLATFORM_TWITTER",
            Self::Youtube => "PLATFORM_YOUTUBE",
        }
    }

    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "PLATFORM_UNSPECIFIED" => Some(Self::Unspecified),
            "PLATFORM_REDDIT" => Some(Self::Reddit),
            "PLATFORM_TIKTOK" => Some(Self::Tiktok),
            "PLATFORM_FACEBOOK" => Some(Self::Facebook),
            "PLATFORM_INSTAGRAM" => Some(Self::Instagram),
            "PLATFORM_TWITTER" => Some(Self::Twitter),
            "PLATFORM_YOUTUBE" => Some(Self::Youtube),
            _ => None,
        }
    }
}

/// Comment processing status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum CommentStatus {
    Unspecified = 0,
    Pending = 1,
    Processing = 2,
    Completed = 3,
}

impl CommentStatus {
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "COMMENT_STATUS_UNSPECIFIED",
            Self::Pending => "COMMENT_STATUS_PENDING",
            Self::Processing => "COMMENT_STATUS_PROCESSING",
            Self::Completed => "COMMENT_STATUS_COMPLETED",
        }
    }

    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "COMMENT_STATUS_UNSPECIFIED" => Some(Self::Unspecified),
            "COMMENT_STATUS_PENDING" => Some(Self::Pending),
            "COMMENT_STATUS_PROCESSING" => Some(Self::Processing),
            "COMMENT_STATUS_COMPLETED" => Some(Self::Completed),
            _ => None,
        }
    }
}

// ============================================================
// CrawlerTask Types (from crawler_task.proto)
// Scheduler -> Agent via Redis Queue
// ============================================================

/// Task message sent from Scheduler to Agent via Redis Queue
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CrawlerTask {
    pub meta: Option<CrawlerTaskMeta>,
    pub spec: Option<CrawlerTaskSpec>,
    pub config: Option<TaskConfig>,
}

/// Task metadata
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CrawlerTaskMeta {
    #[serde(default)]
    pub task_id: i64,
    #[serde(default)]
    pub campaign_id: i32,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub timestamp: f64,
}

/// Task specification
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CrawlerTaskSpec {
    #[serde(default)]
    pub platform: i32,
    #[serde(default)]
    pub data_type: i32,
}

/// Task configuration
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TaskConfig {
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub max_count: i32,
    #[serde(default)]
    pub search_offset: i32,
    #[serde(default)]
    pub search_limit: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<TaskFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_options: Option<String>,
}

/// Task filters
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TaskFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

// ============================================================
// DeviceComments Types (from device_comments.proto)
// API -> Executor via REST
// ============================================================

/// Request parameters for device-based comment query
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct DeviceCommentsQuery {
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_per_page")]
    pub per_page: i32,
    /// Platform name: "tiktok", "facebook", "instagram", "reddit", "twitter"
    #[serde(default = "default_platform")]
    pub platform: String,
}

fn default_page() -> i32 {
    1
}

fn default_per_page() -> i32 {
    20
}

fn default_platform() -> String {
    "tiktok".to_string()
}

/// Optimized response structure: campaign config extracted, comments as array
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct DeviceCommentsResponse {
    /// Campaign configuration (returned once)
    pub campaign: CampaignConfig,
    /// Comment data array
    pub comments: Vec<CommentData>,
    /// Pagination info
    pub pagination: Pagination,
}

/// Campaign auto-interaction configuration
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct CampaignConfig {
    pub campaign_id: i32,
    pub auto_like: bool,
    pub auto_follow: bool,
    pub auto_dm: bool,
    pub auto_reply_comments: bool,
    pub auto_reply_post: bool,
    /// Randomly selected profile_name from campaign's group
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
}

/// Individual comment data
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CommentData {
    /// Database ID
    pub id: i32,
    /// Platform-specific comment ID
    pub comment_id: String,
    /// Content ID (video_id / post_id / tweet_id)
    pub content_id: String,
    /// Platform enum
    pub platform: Platform,
    /// Comment text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Processing status
    pub status: CommentStatus,
    /// User information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_unique_id: Option<String>,
    /// AI-generated suggestions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_dm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_reply_post: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Timestamps (ISO 8601 format string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    pub created_at: String,
    /// Platform-specific fields for URL construction
    /// Direct URL to the content (post/video/tweet)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    /// Content type: POST/VIDEO/REEL (mainly for Facebook)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Author's unique ID (for TikTok URL construction: @{author_unique_id}/video/{content_id})
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_unique_id: Option<String>,
    /// Direct URL to the comment (for platforms that support it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_url: Option<String>,
    /// Profile name for task execution
    /// Randomly selected from campaign's associated social group
    /// Used by executor to determine which browser profile to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
}

/// Pagination information
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Pagination {
    pub total: i64,
    pub page: i32,
    pub per_page: i32,
    pub total_pages: i32,
}

/// Request to update comment status
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct UpdateCommentStatusRequest {
    pub comment_id: String,
    pub status: CommentStatus,
}

/// Response for status update
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct UpdateCommentStatusResponse {
    pub success: bool,
    pub message: String,
}

// ============================================================
// JSON Serialization for Enums
// ============================================================

impl Serialize for Platform {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_json_str())
    }
}

impl<'de> Deserialize<'de> for Platform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Platform::from_json_str(&s).ok_or_else(|| {
            serde::de::Error::unknown_variant(
                &s,
                &[
                    "reddit",
                    "tiktok",
                    "facebook",
                    "instagram",
                    "twitter",
                    "youtube",
                ],
            )
        })
    }
}

impl Platform {
    /// Convert to lowercase string for JSON serialization
    pub fn to_json_str(&self) -> &'static str {
        match self {
            Platform::Unspecified => "unspecified",
            Platform::Reddit => "reddit",
            Platform::Tiktok => "tiktok",
            Platform::Facebook => "facebook",
            Platform::Instagram => "instagram",
            Platform::Twitter => "twitter",
            Platform::Youtube => "youtube",
        }
    }

    /// Parse from string (case-insensitive)
    pub fn from_json_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "reddit" => Some(Platform::Reddit),
            "tiktok" => Some(Platform::Tiktok),
            "facebook" => Some(Platform::Facebook),
            "instagram" => Some(Platform::Instagram),
            "twitter" => Some(Platform::Twitter),
            "youtube" => Some(Platform::Youtube),
            "unspecified" => Some(Platform::Unspecified),
            _ => None,
        }
    }

    /// Convert from database platform_id
    pub fn from_platform_id(id: i32) -> Self {
        match id {
            1 => Platform::Reddit,
            2 => Platform::Tiktok,
            3 => Platform::Facebook,
            4 => Platform::Instagram,
            5 => Platform::Twitter,
            6 => Platform::Youtube,
            _ => Platform::Unspecified,
        }
    }
}

impl From<Platform> for i32 {
    fn from(p: Platform) -> i32 {
        p as i32
    }
}

// DataType JSON serialization
impl Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_json_str())
    }
}

impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DataType::from_json_str(&s).ok_or_else(|| {
            serde::de::Error::unknown_variant(
                &s,
                &[
                    "video_content",
                    "video_metadata",
                    "video_comments",
                    "keyword_search",
                ],
            )
        })
    }
}

impl DataType {
    pub fn to_json_str(&self) -> &'static str {
        match self {
            DataType::Unspecified => "unspecified",
            DataType::VideoContent => "video_content",
            DataType::VideoMetadata => "video_metadata",
            DataType::VideoComments => "video_comments",
            DataType::KeywordSearch => "keyword_search",
        }
    }

    pub fn from_json_str(s: &str) -> Option<Self> {
        match s {
            "video_content" => Some(DataType::VideoContent),
            "video_metadata" => Some(DataType::VideoMetadata),
            "video_comments" => Some(DataType::VideoComments),
            "keyword_search" => Some(DataType::KeywordSearch),
            "unspecified" => Some(DataType::Unspecified),
            _ => None,
        }
    }
}

impl From<DataType> for i32 {
    fn from(d: DataType) -> i32 {
        d as i32
    }
}

// TimeRange JSON serialization
impl Serialize for TimeRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_json_str())
    }
}

impl<'de> Deserialize<'de> for TimeRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        TimeRange::from_json_str(&s).ok_or_else(|| {
            serde::de::Error::unknown_variant(
                &s,
                &["all_time", "last_24h", "last_7d", "last_30d", "last_180d"],
            )
        })
    }
}

impl TimeRange {
    pub fn to_json_str(&self) -> &'static str {
        match self {
            TimeRange::Unspecified => "unspecified",
            TimeRange::AllTime => "all_time",
            TimeRange::Last24h => "last_24h",
            TimeRange::Last7d => "last_7d",
            TimeRange::Last30d => "last_30d",
            TimeRange::Last180d => "last_180d",
        }
    }

    pub fn from_json_str(s: &str) -> Option<Self> {
        match s {
            "all_time" => Some(TimeRange::AllTime),
            "last_24h" => Some(TimeRange::Last24h),
            "last_7d" => Some(TimeRange::Last7d),
            "last_30d" => Some(TimeRange::Last30d),
            "last_180d" => Some(TimeRange::Last180d),
            "unspecified" => Some(TimeRange::Unspecified),
            _ => None,
        }
    }
}

impl From<TimeRange> for i32 {
    fn from(t: TimeRange) -> i32 {
        t as i32
    }
}

impl Serialize for CommentStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_json_str())
    }
}

impl<'de> Deserialize<'de> for CommentStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        CommentStatus::from_json_str(&s).ok_or_else(|| {
            serde::de::Error::unknown_variant(&s, &["pending", "processing", "completed"])
        })
    }
}

impl CommentStatus {
    pub fn to_json_str(&self) -> &'static str {
        match self {
            CommentStatus::Unspecified => "unspecified",
            CommentStatus::Pending => "pending",
            CommentStatus::Processing => "processing",
            CommentStatus::Completed => "completed",
        }
    }

    pub fn from_json_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(CommentStatus::Pending),
            "processing" => Some(CommentStatus::Processing),
            "completed" => Some(CommentStatus::Completed),
            "unspecified" => Some(CommentStatus::Unspecified),
            _ => None,
        }
    }

    /// Convert from i16 status code (database format)
    pub fn from_i16(status: i16) -> Self {
        match status {
            0 => CommentStatus::Pending,
            1 => CommentStatus::Processing,
            2 => CommentStatus::Completed,
            _ => CommentStatus::Unspecified,
        }
    }

    /// Convert to i16 status code (database format)
    pub fn to_i16(&self) -> i16 {
        match self {
            CommentStatus::Pending => 0,
            CommentStatus::Processing => 1,
            CommentStatus::Completed => 2,
            CommentStatus::Unspecified => -1,
        }
    }

    /// Convert from string status (for database string format)
    /// Supports both lowercase and uppercase: "pending"/"PENDING", etc.
    pub fn from_str_status(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => CommentStatus::Pending,
            "processing" => CommentStatus::Processing,
            "completed" => CommentStatus::Completed,
            _ => CommentStatus::Unspecified,
        }
    }
}

// ============================================================
// AIPub Types (from aipub.proto)
// API <-> Scheduler for AI Publish feature
//
// Version History:
// - v1 (2026-01-26): Initial protocol with AiPubInput, AiPubTaskContent,
//                    AiTaskInput, AiTaskResult
// ============================================================

/// Current protocol version
pub const AIPUB_PROTOCOL_VERSION: i32 = 1;

/// Image configuration for FL video models
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AiPubImageConfig {
    /// URL of the start frame image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_frame_url: Option<String>,
    /// URL of the end frame image (for FL models that support transitions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_frame_url: Option<String>,
}

// ============================================================
// AI Task Input/Result Protocol
// Structure for gm_aipub_ai_tasks.input and result fields
// Used by: Scheduler (creates/reads)
// ============================================================

/// AI Task Input - stored in gm_aipub_ai_tasks.input
/// Supports multiple task types: content_gen, video_gen, combined
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AiTaskInput {
    /// Protocol version for forward compatibility
    #[serde(default)]
    pub version: i32,

    // === Content Generation Input ===
    /// Video generation base prompt (from AiPubInput.video_prompt)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_prompt: Option<String>,

    /// Content generation prompt (from AiPubInput.content_prompt)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_prompt: Option<String>,

    // === Video Generation Input ===
    /// AI model name (e.g., "veo-3.1", "sora-1.0")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Final assembled prompt for video generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Associated aipub_task ID (for video_gen tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aipub_task_id: Option<i32>,

    /// Start frame image URL (for image-to-video)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_image_url: Option<String>,

    /// End frame image URL (for FL models)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_image_url: Option<String>,
}

impl AiTaskInput {
    /// Create input for content generation task
    pub fn for_content_gen(video_prompt: Option<String>, content_prompt: Option<String>) -> Self {
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            video_prompt,
            content_prompt,
            ..Default::default()
        }
    }

    /// Create input for video generation task
    pub fn for_video_gen(model: String, prompt: String, aipub_task_id: i32) -> Self {
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            model: Some(model),
            prompt: Some(prompt),
            aipub_task_id: Some(aipub_task_id),
            ..Default::default()
        }
    }

    /// Create input for video generation with images (FL models)
    pub fn for_video_gen_with_images(
        model: String,
        prompt: String,
        aipub_task_id: i32,
        start_image_url: Option<String>,
        end_image_url: Option<String>,
    ) -> Self {
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            model: Some(model),
            prompt: Some(prompt),
            aipub_task_id: Some(aipub_task_id),
            start_image_url,
            end_image_url,
            ..Default::default()
        }
    }
}

/// AI Task Result - stored in gm_aipub_ai_tasks.result
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AiTaskResult {
    /// Protocol version for forward compatibility
    #[serde(default)]
    pub version: i32,

    // === Content Generation Result ===
    /// Number of content variations generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_count: Option<i32>,

    /// Number of video tasks pending
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_pending_count: Option<i32>,

    /// Generated content variations (for batch_text plans)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_variations: Vec<ContentVariation>,

    // === Video Generation Result ===
    /// Generated video URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,

    /// Video duration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_duration: Option<f32>,

    // === Common Fields ===
    /// Timestamp when task completed (ISO 8601 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,

    /// Error message if task failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Content variation - single generated content item
/// Used in AiTaskResult.content_variations for batch content generation
/// Matches proto message ContentVariation
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct ContentVariation {
    /// Post title (for TikTok, YouTube, etc.)
    #[serde(default)]
    pub title: String,

    /// Main text content (description/caption)
    /// Note: Accepts both "text_content" (proto) and "description" (legacy) for deserialization
    #[serde(default, alias = "description")]
    pub text_content: String,

    /// Hashtags for the post
    #[serde(default)]
    pub hashtags: Vec<String>,

    /// Location tag (for TikTok/Instagram geo-tagging, e.g., "New York", "Tokyo")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,

    /// Video scene description - unique visual elements for this variation
    /// Used to differentiate videos when same base video_prompt is shared
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_scene: Option<String>,
}

impl AiTaskResult {
    /// Create result for content generation task
    pub fn for_content_gen(content_count: i32, video_pending_count: i32) -> Self {
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            content_count: Some(content_count),
            video_pending_count: Some(video_pending_count),
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    /// Create result for content generation task with variations
    pub fn for_content_gen_with_variations(
        content_variations: Vec<ContentVariation>,
        video_pending_count: i32,
    ) -> Self {
        let content_count = content_variations.len() as i32;
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            content_count: Some(content_count),
            video_pending_count: Some(video_pending_count),
            content_variations,
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    /// Create result for video generation task
    pub fn for_video_gen(video_url: String) -> Self {
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            video_url: Some(video_url),
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    /// Create error result
    pub fn error(error_msg: String) -> Self {
        Self {
            version: AIPUB_PROTOCOL_VERSION,
            error: Some(error_msg),
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }
}

/// AI Publish input configuration.
/// Used by API to create plans and by Scheduler to generate AI tasks.
///
/// Contains prompts for different content types and optional image configurations for FL models.
///
/// ## Prompt Fields
/// - `video_prompt`: Used for video generation (e.g., scene description, transitions, visual style)
/// - `content_prompt`: Used for text content generation (captions, titles, descriptions, hashtags)
/// - `prompt`: Legacy field, kept for backward compatibility. If set, used as fallback when specific prompts are empty.
///
/// ## Usage Examples
/// - Video content: Set both `video_prompt` (for video AI) and `content_prompt` (for text/captions)
/// - Text-only content: Only set `content_prompt`
/// - Legacy API calls: Only set `prompt` (both tasks will use this)
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AiPubInput {
    /// Video generation prompt - describes the visual content, scenes, transitions, and style
    /// Used by video AI models (e.g., Sora, Veo) to generate video content
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub video_prompt: String,

    /// Content/text generation prompt - for titles, captions, descriptions, and hashtags
    /// Used by chat AI models (e.g., GPT, DeepSeek) to generate accompanying text
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_prompt: String,

    /// Legacy prompt field - kept for backward compatibility
    /// If video_prompt or content_prompt is empty, this value is used as fallback
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,

    /// Default images for FL video models (used for all accounts if no per-account images)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_images: Option<AiPubImageConfig>,

    /// Per-account images for FL video models (key: account_id as string)
    /// Overrides default_images for specific accounts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_images: Option<std::collections::HashMap<String, AiPubImageConfig>>,
}

impl AiPubInput {
    /// Create a new AiPubInput with just a legacy prompt (for backward compatibility)
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            video_prompt: String::new(),
            content_prompt: String::new(),
            default_images: None,
            account_images: None,
        }
    }

    /// Create a new AiPubInput with separate video and content prompts
    pub fn with_prompts(
        video_prompt: impl Into<String>,
        content_prompt: impl Into<String>,
    ) -> Self {
        Self {
            video_prompt: video_prompt.into(),
            content_prompt: content_prompt.into(),
            prompt: String::new(),
            default_images: None,
            account_images: None,
        }
    }

    /// Get the effective video prompt (falls back to legacy prompt if empty)
    pub fn get_video_prompt(&self) -> &str {
        if !self.video_prompt.is_empty() {
            &self.video_prompt
        } else {
            &self.prompt
        }
    }

    /// Get the effective content prompt (falls back to legacy prompt if empty)
    pub fn get_content_prompt(&self) -> &str {
        if !self.content_prompt.is_empty() {
            &self.content_prompt
        } else {
            &self.prompt
        }
    }

    /// Get the image configuration for a specific account.
    /// Returns per-account images if available, otherwise default images.
    pub fn get_images_for_account(&self, account_id: &str) -> Option<&AiPubImageConfig> {
        if let Some(ref account_images) = self.account_images {
            if let Some(config) = account_images.get(account_id) {
                return Some(config);
            }
        }
        self.default_images.as_ref()
    }
}

// ============================================================
// AiPub Task Content
// Content structure for gm_aipub_tasks.content field
// Used by: Scheduler (creates), API (reads), Executor (reads for publishing)
// ============================================================

/// Content structure for gm_aipub_tasks.content field.
/// Used by: Scheduler (creates), API (reads), Executor (reads for publishing)
///
/// This is the standardized structure for task content stored in the database.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct AiPubTaskContent {
    // === Publishing Content ===
    /// Main text content for the post (description/caption)
    #[serde(default)]
    pub text_content: String,

    /// Post title (for TikTok, YouTube, etc.)
    #[serde(default)]
    pub title: String,

    /// Hashtags for the post
    #[serde(default)]
    pub hashtags: Vec<String>,

    /// Location tag (for TikTok/Instagram geo-tagging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,

    /// Video scene description - unique visual elements for this variation
    /// Used to differentiate videos when same base video_prompt is shared
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_scene: Option<String>,

    // === Video Generation ===
    /// Final video generation prompt (base_prompt + video_scene + title)
    /// Sent to video AI for generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_prompt: Option<String>,

    /// Start frame image URL (for image-to-video generation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_frame_url: Option<String>,

    /// End frame image URL (for FL models with transitions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_frame_url: Option<String>,

    /// Generated video URL (filled after video generation completes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,

    // === Task State Flags ===
    /// Whether this task requires video generation
    #[serde(default)]
    pub video_generation_needed: bool,

    /// Whether video has been submitted to generation service
    #[serde(default)]
    pub video_submitted: bool,

    /// Associated AI task ID (references gm_aipub_ai_tasks.id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_task_id: Option<i32>,
}

impl AiPubTaskContent {
    /// Create a new task content with basic publishing content
    pub fn new(title: impl Into<String>, text_content: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text_content: text_content.into(),
            ..Default::default()
        }
    }

    /// Check if this task has a video (either generated or to be generated)
    pub fn has_video(&self) -> bool {
        self.video_url.is_some() || self.video_generation_needed
    }

    /// Check if this task is ready for publishing
    pub fn is_ready_for_publish(&self) -> bool {
        // If video needed, must have video_url
        if self.video_generation_needed && self.video_url.is_none() {
            return false;
        }
        // Must have content
        !self.title.is_empty() || !self.text_content.is_empty()
    }
}

// ============================================================
// Default implementations
// ============================================================

impl Default for Pagination {
    fn default() -> Self {
        Self {
            total: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
        }
    }
}

// ============================================================
// Builder methods for DeviceCommentsResponse
// ============================================================

impl DeviceCommentsResponse {
    /// Create a new response with the given data
    pub fn new(
        campaign: CampaignConfig,
        comments: Vec<CommentData>,
        total: i64,
        page: i32,
        per_page: i32,
    ) -> Self {
        let total_pages = if per_page > 0 {
            ((total as f64) / (per_page as f64)).ceil() as i32
        } else {
            0
        };

        Self {
            campaign,
            comments,
            pagination: Pagination {
                total,
                page,
                per_page,
                total_pages,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_json_serialization() {
        let platform = Platform::Tiktok;
        let json = serde_json::to_string(&platform).unwrap();
        assert_eq!(json, r#""tiktok""#);

        let parsed: Platform = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Platform::Tiktok);
    }

    #[test]
    fn test_platform_to_i32() {
        let platform: i32 = Platform::Tiktok.into();
        assert_eq!(platform, 2);

        let reddit: i32 = Platform::Reddit.into();
        assert_eq!(reddit, 1);
    }

    #[test]
    fn test_comment_status_i16_conversion() {
        assert_eq!(CommentStatus::from_i16(0), CommentStatus::Pending);
        assert_eq!(CommentStatus::Completed.to_i16(), 2);
    }

    #[test]
    fn test_device_comments_query_deserialization() {
        let json = r#"{"device_id": "test_device"}"#;
        let query: DeviceCommentsQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.device_id, "test_device");
        assert_eq!(query.platform, "tiktok"); // default
        assert_eq!(query.page, 1); // default
        assert_eq!(query.per_page, 20); // default
    }

    #[test]
    fn test_device_comments_response_serialization() {
        let response = DeviceCommentsResponse::new(
            CampaignConfig {
                campaign_id: 123,
                auto_like: true,
                auto_follow: false,
                auto_dm: true,
                auto_reply_comments: true,
                auto_reply_post: false,
                profile_name: Some("test_profile".to_string()),
            },
            vec![CommentData {
                id: 1,
                comment_id: "c123".to_string(),
                content_id: "v456".to_string(),
                platform: Platform::Tiktok,
                content: Some("Test comment".to_string()),
                status: CommentStatus::Pending,
                user_nickname: Some("user1".to_string()),
                user_unique_id: Some("uid1".to_string()),
                suggested_reply: None,
                suggested_dm: None,
                suggested_reply_post: None,
                reason: None,
                create_time: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                content_url: None,
                content_type: None,
                author_unique_id: Some("tiktok_author".to_string()),
                comment_url: None,
                profile_name: Some("test_profile".to_string()),
            }],
            100,
            1,
            20,
        );

        let json = serde_json::to_string_pretty(&response).unwrap();
        println!("{}", json);

        let parsed: DeviceCommentsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.campaign.campaign_id, 123);
        assert_eq!(parsed.comments.len(), 1);
        assert_eq!(parsed.pagination.total, 100);
        assert_eq!(parsed.pagination.total_pages, 5);
    }

    #[test]
    fn test_crawler_task_serialization() {
        let task = CrawlerTask {
            meta: Some(CrawlerTaskMeta {
                task_id: 123,
                campaign_id: 456,
                source: "campaign-456".to_string(),
                timestamp: 1234567890.0,
            }),
            spec: Some(CrawlerTaskSpec {
                platform: Platform::Tiktok.into(),
                data_type: DataType::VideoComments.into(),
            }),
            config: Some(TaskConfig {
                keywords: vec!["test".to_string()],
                max_count: 50,
                search_offset: 0,
                search_limit: 20,
                filters: Some(TaskFilters {
                    time_range: Some(TimeRange::Last180d.into()),
                    region: Some("US".to_string()),
                }),
                search_options: None,
            }),
        };

        let json = serde_json::to_string_pretty(&task).unwrap();
        println!("{}", json);

        let parsed: CrawlerTask = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.meta.unwrap().task_id, 123);
        assert_eq!(parsed.spec.unwrap().platform, 2); // TikTok
    }

    #[test]
    fn test_data_type_json() {
        let dt = DataType::VideoComments;
        let json = serde_json::to_string(&dt).unwrap();
        assert_eq!(json, r#""video_comments""#);
    }

    #[test]
    fn test_time_range_json() {
        let tr = TimeRange::Last30d;
        let json = serde_json::to_string(&tr).unwrap();
        assert_eq!(json, r#""last_30d""#);
    }

    #[test]
    fn test_aipub_input_serialization() {
        let input = AiPubInput {
            video_prompt: "Create a dynamic video with smooth transitions".to_string(),
            content_prompt: "Summer fashion trends 2024 - must-have styles".to_string(),
            prompt: String::new(),
            default_images: Some(AiPubImageConfig {
                start_frame_url: Some("https://example.com/start.png".to_string()),
                end_frame_url: Some("https://example.com/end.png".to_string()),
            }),
            account_images: None,
        };

        let json = serde_json::to_string_pretty(&input).unwrap();
        println!("{}", json);

        let parsed: AiPubInput = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.video_prompt,
            "Create a dynamic video with smooth transitions"
        );
        assert_eq!(
            parsed.content_prompt,
            "Summer fashion trends 2024 - must-have styles"
        );
        assert!(parsed.default_images.is_some());
    }

    #[test]
    fn test_aipub_input_with_account_images() {
        let mut account_images = std::collections::HashMap::new();
        account_images.insert(
            "123".to_string(),
            AiPubImageConfig {
                start_frame_url: Some("https://example.com/acc123_start.png".to_string()),
                end_frame_url: None,
            },
        );

        let input = AiPubInput {
            video_prompt: String::new(),
            content_prompt: String::new(),
            prompt: "Test prompt".to_string(),
            default_images: Some(AiPubImageConfig {
                start_frame_url: Some("https://example.com/default_start.png".to_string()),
                end_frame_url: None,
            }),
            account_images: Some(account_images),
        };

        // Account 123 should get its own images
        let acc123_images = input.get_images_for_account("123");
        assert!(acc123_images.is_some());
        assert_eq!(
            acc123_images.unwrap().start_frame_url,
            Some("https://example.com/acc123_start.png".to_string())
        );

        // Account 456 should get default images
        let acc456_images = input.get_images_for_account("456");
        assert!(acc456_images.is_some());
        assert_eq!(
            acc456_images.unwrap().start_frame_url,
            Some("https://example.com/default_start.png".to_string())
        );
    }

    #[test]
    fn test_aipub_input_prompt_fallback() {
        // Test with separate prompts
        let input = AiPubInput::with_prompts("Video description here", "Caption and hashtags here");
        assert_eq!(input.get_video_prompt(), "Video description here");
        assert_eq!(input.get_content_prompt(), "Caption and hashtags here");

        // Test legacy prompt fallback
        let legacy_input = AiPubInput::new("Legacy combined prompt");
        assert_eq!(legacy_input.get_video_prompt(), "Legacy combined prompt");
        assert_eq!(legacy_input.get_content_prompt(), "Legacy combined prompt");

        // Test mixed (video_prompt set, content_prompt empty -> fallback to prompt)
        let mixed_input = AiPubInput {
            video_prompt: "Specific video prompt".to_string(),
            content_prompt: String::new(),
            prompt: "Fallback prompt".to_string(),
            default_images: None,
            account_images: None,
        };
        assert_eq!(mixed_input.get_video_prompt(), "Specific video prompt");
        assert_eq!(mixed_input.get_content_prompt(), "Fallback prompt");
    }
}
