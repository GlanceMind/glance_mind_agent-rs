//! Database models
//!
//! Models match the production database schema.

use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use super::schema::*;

// ============================================================
// Platform Models
// ============================================================

/// Platform record
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_platforms)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Platform {
    pub id: i32,
    pub name: String,
    pub display_name: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub base_url: String,
    pub page_size: i32,
    pub content_table_name: Option<String>,
    pub comment_table_name: Option<String>,
}

// ============================================================
// Campaign Models
// ============================================================

/// Campaign record for querying
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_campaigns)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Campaign {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
    pub status: String,
    pub platform_id: i32,
    pub region_id: i32,
    pub ai_model_id: i32,
    pub target_audience: Option<String>,
    pub product_prompt: String,
    pub schedule_config: Option<serde_json::Value>,
    pub enable_ai_refactor: Option<bool>,
    pub persona_id: Option<i32>,
    pub max_scan_count: Option<i32>,
    pub budget_cap: Option<BigDecimal>,
    pub end_date: Option<DateTime<Utc>>,
    pub schedule_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub keyword: Option<String>,
    pub social_group_id: Option<i32>,
    pub call_to_action: Option<String>,
    pub tone_of_voice: Option<String>,
    pub additional_info: Option<String>,
    pub total_scanned: i32,
    pub auto_like: bool,
    pub auto_follow: bool,
    pub auto_dm: bool,
    pub pending_consumption: BigDecimal,
    pub actual_consumption: BigDecimal,
    pub is_frozen: bool,
    pub search_options: Option<serde_json::Value>,
    pub auto_reply_comments: bool,
    pub auto_reply_post: bool,
    pub completed_reason: Option<String>,
    pub reply_template_ids: Vec<i32>,
}

// ============================================================
// Campaign Template Models
// ============================================================

/// Campaign template record
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_campaign_templates)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct CampaignTemplate {
    pub id: i32,
    pub campaign_id: i32,
    pub library_template_id: Option<i32>,
    pub weight: i32,
    pub reply_prompt: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub dm_prompt: Option<String>,
    pub reply_post_prompt: Option<String>,
    pub name: Option<String>,
}

/// User-level reusable reply template library record.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_reply_template_library)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ReusableReplyTemplate {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub weight: i32,
    pub dm_prompt: Option<String>,
    pub reply_prompt: Option<String>,
    pub reply_post_prompt: Option<String>,
    pub usage_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ============================================================
// Crawler Task Models
// ============================================================

/// Crawler task record for querying
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_crawler_tasks)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct CrawlerTask {
    pub id: i32,
    pub campaign_id: i32,
    pub keywords: Option<Vec<String>>,
    pub max_count: i32,
    pub process_count: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub search_offset: i32,
    pub search_limit: i32,
    pub reserved_amount: Option<BigDecimal>,
    pub actual_consumption: Option<BigDecimal>,
    pub settled_at: Option<DateTime<Utc>>,
    pub terminal_reason: Option<String>,
}

// ============================================================
// Agent Video Models (TikTok)
// ============================================================

/// Agent video record for querying
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_videos)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct AgentVideo {
    pub id: i32,
    pub video_id: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub task_id: i32,
    pub campaign_id: Option<i32>,
    pub like_count: Option<i32>,
    pub comment_count: Option<i32>,
    pub share_count: Option<i32>,
    pub play_count: Option<i32>,
    pub publish_time: Option<i64>,
    pub author_unique_id: Option<String>,
    pub url: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// New agent video for insertion
#[derive(Debug, Clone, Insertable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_videos)]
pub struct NewAgentVideo {
    pub video_id: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub task_id: i32,
    pub campaign_id: Option<i32>,
    pub like_count: Option<i32>,
    pub comment_count: Option<i32>,
    pub share_count: Option<i32>,
    pub play_count: Option<i32>,
    pub publish_time: Option<i64>,
    pub author_unique_id: Option<String>,
    pub url: Option<String>,
}

// ============================================================
// Agent Comment Models (TikTok)
// ============================================================

/// Agent comment record for querying
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_comments)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct AgentComment {
    pub id: i32,
    pub video_db_id: i32,
    pub comment_id: String,
    pub user_nickname: Option<String>,
    pub user_unique_id: Option<String>,
    pub content: Option<String>,
    pub reason: Option<String>,
    pub suggested_reply: Option<String>,
    pub create_time: Option<NaiveDateTime>,
    pub created_at: DateTime<Utc>,
    pub campaign_id: Option<i32>,
    pub status: i16,
    pub suggested_dm: Option<String>,
    pub suggested_reply_post: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// New agent comment for insertion
#[derive(Debug, Clone, Insertable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_comments)]
pub struct NewAgentComment {
    pub video_db_id: i32,
    pub comment_id: String,
    pub user_nickname: Option<String>,
    pub user_unique_id: Option<String>,
    pub content: Option<String>,
    pub campaign_id: Option<i32>,
    pub status: i16,
}

/// Update agent comment (for AI analysis results)
#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = gm_agent_comments)]
pub struct UpdateAgentComment {
    pub reason: Option<String>,
    pub suggested_reply: Option<String>,
    pub suggested_dm: Option<String>,
    pub suggested_reply_post: Option<String>,
    pub status: Option<i16>,
    // Matching Python agent: updated_at = NOW() on update
    pub updated_at: Option<DateTime<Utc>>,
}

// ============================================================
// Facebook Models
// ============================================================

/// Facebook post record for querying.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_facebook_posts)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct FacebookPost {
    pub id: i32,
    pub task_id: i32,
    pub campaign_id: Option<i32>,
    pub facebook_post_id: String,
    pub post_type: Option<String>,
    pub url: Option<String>,
    pub message: Option<String>,
    pub message_rich: Option<String>,
    pub timestamp: Option<i64>,
    pub posted_at: Option<DateTime<Utc>>,
    pub reactions_count: Option<i32>,
    pub comments_count: Option<i32>,
    pub reshare_count: Option<i32>,
    pub reactions_like: Option<i32>,
    pub reactions_love: Option<i32>,
    pub reactions_haha: Option<i32>,
    pub reactions_wow: Option<i32>,
    pub reactions_sad: Option<i32>,
    pub reactions_angry: Option<i32>,
    pub reactions_care: Option<i32>,
    pub author_id: Option<String>,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
    pub author_profile_picture_url: Option<String>,
    pub author_title: Option<String>,
    pub has_image: Option<bool>,
    pub image_url: Option<String>,
    pub image_width: Option<i32>,
    pub image_height: Option<i32>,
    pub image_id: Option<String>,
    pub has_video: Option<bool>,
    pub video_thumbnail: Option<String>,
    pub external_url: Option<String>,
    pub attached_post_url: Option<String>,
    pub comments_id: Option<String>,
    pub shares_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Facebook comment record for querying.
#[derive(Debug, Clone, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_facebook_comments)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct FacebookComment {
    pub id: i32,
    pub post_db_id: i32,
    pub campaign_id: Option<i32>,
    pub facebook_comment_id: String,
    pub parent_comment_id: Option<String>,
    pub comment_url: Option<String>,
    pub comment_text: String,
    pub reason: Option<String>,
    pub suggested_reply: Option<String>,
    pub suggested_dm: Option<String>,
    pub suggested_reply_post: Option<String>,
    pub comment_user_id: Option<String>,
    pub comment_username: Option<String>,
    pub comment_user_url: Option<String>,
    pub comment_user_profile_picture: Option<String>,
    pub like_count: Option<i32>,
    pub reply_count: Option<i32>,
    pub threading_depth: Option<i32>,
    pub created_at_ts: Option<i64>,
    pub comment_created_at: Option<DateTime<Utc>>,
    pub facebook_post_id: Option<String>,
    pub post_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub status: Option<i16>,
}

/// Twitter tweet record for querying.
#[derive(Debug, Clone, Queryable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_twitter_tweets)]
pub struct TwitterTweet {
    pub id: i32,
    pub task_id: i32,
    pub campaign_id: Option<i32>,
    pub twitter_tweet_id: String,
    pub conversation_id: Option<String>,
    pub full_text: String,
    pub lang: Option<String>,
    pub screen_name: Option<String>,
    pub user_name: Option<String>,
    pub user_id: Option<String>,
    pub user_description: Option<String>,
    pub user_followers_count: Option<i32>,
    pub user_avatar: Option<String>,
    pub user_verified: Option<bool>,
    pub media_urls: Option<Vec<Option<String>>>,
    pub has_media: Option<bool>,
    pub favorite_count: Option<i32>,
    pub retweet_count: Option<i32>,
    pub reply_count: Option<i32>,
    pub quote_count: Option<i32>,
    pub bookmark_count: Option<i32>,
    pub view_count: Option<i32>,
    pub is_reply: Option<bool>,
    pub in_reply_to_status_id: Option<String>,
    pub in_reply_to_user_id: Option<String>,
    pub created_at_str: Option<String>,
    pub created_at_ts: Option<i64>,
    pub tweet_created_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Twitter comment record for querying.
#[derive(Debug, Clone, Queryable, Serialize, Deserialize)]
#[diesel(table_name = gm_agent_twitter_comments)]
pub struct TwitterComment {
    pub id: i32,
    pub tweet_db_id: i32,
    pub campaign_id: Option<i32>,
    pub twitter_comment_id: String,
    pub conversation_id: Option<String>,
    pub comment_screen_name: Option<String>,
    pub comment_user_name: Option<String>,
    pub comment_user_id: Option<String>,
    pub comment_user_followers: Option<i32>,
    pub comment_text: String,
    pub reason: Option<String>,
    pub suggested_reply: Option<String>,
    pub favorite_count: Option<i32>,
    pub retweet_count: Option<i32>,
    pub reply_count: Option<i32>,
    pub in_reply_to_status_id: Option<String>,
    pub is_reply: Option<bool>,
    pub media_urls: Option<Vec<Option<String>>>,
    pub has_media: Option<bool>,
    pub created_at_str: Option<String>,
    pub created_at_ts: Option<i64>,
    pub comment_created_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub suggested_dm: Option<String>,
    pub suggested_reply_post: Option<String>,
    pub status: Option<i16>,
}

// ============================================================
// Status Constants
// ============================================================

/// Comment processing status
pub mod comment_status {
    pub const PENDING: i16 = 0;
    pub const PROCESSING: i16 = 1;
    pub const COMPLETED: i16 = 2;
    pub const FAILED: i16 = 3;
}

/// Campaign status (string-based, matching production)
pub mod campaign_status {
    pub const DRAFT: &str = "DRAFT";
    pub const ACTIVE: &str = "ACTIVE";
    pub const PAUSED: &str = "PAUSED";
    pub const COMPLETED: &str = "COMPLETED";
    pub const STOPPING: &str = "STOPPING";
}

/// Crawler task status (string-based, matching production)
/// Note: "processing" must match fn_update_task_progress stored procedure check
pub mod task_status {
    pub const INIT: &str = "init";
    pub const PENDING: &str = "pending";
    pub const PROCESSING: &str = "processing"; // Was "running", changed to match stored procedure
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
}

/// Platform IDs (must match database)
pub mod platform_id {
    pub const REDDIT: i32 = 1;
    pub const TIKTOK: i32 = 2;
    pub const FACEBOOK: i32 = 3;
    pub const INSTAGRAM: i32 = 4;
    pub const TWITTER: i32 = 5;
    pub const YOUTUBE: i32 = 6;
}
