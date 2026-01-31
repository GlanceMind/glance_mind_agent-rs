//! TikHub API Response Types
//!
//! These types are derived from actual TikHub API responses.
//! Use the fixture generator to capture real API responses for testing.

use serde::{Deserialize, Serialize};

// ============================================================
// Video Search API Types
// Endpoint: /api/v1/tiktok/app/v3/fetch_video_search_result
// ============================================================

/// Video search API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    pub data: Option<SearchData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchData {
    pub search_item_list: Option<Vec<SearchItem>>,
    pub has_more: Option<i32>,
    pub cursor: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchItem {
    pub aweme_info: Option<AwemeInfo>,
}

/// TikTok video information (aweme_info)
/// This is the core video data structure used across multiple endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwemeInfo {
    /// Unique video ID
    pub aweme_id: String,

    /// Video description/caption
    #[serde(default)]
    pub desc: Option<String>,

    /// Unix timestamp of creation
    pub create_time: Option<i64>,

    /// Shareable URL
    pub share_url: Option<String>,

    /// Video author information
    pub author: Option<Author>,

    /// Engagement statistics
    pub statistics: Option<Statistics>,

    /// Video details (duration, cover, etc.)
    pub video: Option<VideoDetail>,

    /// Music information
    pub music: Option<MusicInfo>,
}

/// Video author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    /// User ID
    pub uid: Option<String>,

    /// Username (unique_id, e.g., @username)
    pub unique_id: Option<String>,

    /// Display name
    pub nickname: Option<String>,

    /// Secure user ID
    pub sec_uid: Option<String>,

    /// Avatar URL
    pub avatar_thumb: Option<AvatarInfo>,

    /// Verification status
    pub custom_verify: Option<String>,

    /// Follower count
    pub follower_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarInfo {
    pub url_list: Option<Vec<String>>,
}

/// Video engagement statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    /// Like count
    pub digg_count: Option<i64>,

    /// Comment count
    pub comment_count: Option<i64>,

    /// Share count
    pub share_count: Option<i64>,

    /// View/play count
    pub play_count: Option<i64>,

    /// Collect/favorite count
    pub collect_count: Option<i64>,

    /// Download count
    pub download_count: Option<i64>,
}

/// Video details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDetail {
    /// Video duration in seconds
    pub duration: Option<i64>,

    /// Video width
    pub width: Option<i32>,

    /// Video height
    pub height: Option<i32>,

    /// Cover image
    pub cover: Option<CoverInfo>,

    /// Dynamic cover (animated)
    pub dynamic_cover: Option<CoverInfo>,

    /// Play address
    pub play_addr: Option<PlayAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverInfo {
    pub url_list: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayAddr {
    pub url_list: Option<Vec<String>>,
    pub data_size: Option<i64>,
}

/// Music/audio information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicInfo {
    pub id: Option<i64>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub album: Option<String>,
    pub duration: Option<i64>,
}

// ============================================================
// Comments API Types
// Endpoint: /api/v1/tiktok/web/fetch_post_comment
// ============================================================

/// Comments API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentsResponse {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    pub data: Option<CommentsData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentsData {
    pub comments: Option<Vec<TikTokComment>>,
    pub has_more: Option<i32>,
    pub cursor: Option<i64>,
    pub total: Option<i64>,
}

/// TikTok comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokComment {
    /// Comment ID
    pub cid: String,

    /// Comment text content
    pub text: Option<String>,

    /// Unix timestamp of creation
    pub create_time: Option<i64>,

    /// Like count on this comment
    pub digg_count: Option<i64>,

    /// Parent comment ID (if this is a reply)
    /// "0" means it's a top-level comment
    pub reply_id: Option<String>,

    /// Number of replies to this comment
    pub reply_comment_total: Option<i32>,

    /// Video ID this comment belongs to
    pub aweme_id: Option<String>,

    /// Comment author
    pub user: Option<CommentUser>,

    /// Whether this comment is pinned by author
    pub is_author_digged: Option<bool>,

    /// Comment language
    pub comment_language: Option<String>,
}

/// Comment author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentUser {
    /// User ID
    pub uid: Option<String>,

    /// Username
    pub unique_id: Option<String>,

    /// Display name
    pub nickname: Option<String>,

    /// Avatar
    pub avatar_thumb: Option<AvatarInfo>,

    /// Secure user ID
    pub sec_uid: Option<String>,
}

// ============================================================
// User Videos API Types
// Endpoint: /api/v1/tiktok/app/v3/fetch_user_post_videos
// ============================================================

/// User videos API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserVideosResponse {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    pub data: Option<UserVideosData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserVideosData {
    /// List of videos
    pub aweme_list: Option<Vec<AwemeInfo>>,

    /// Whether there are more videos
    pub has_more: Option<i32>,

    /// Cursor for pagination
    pub max_cursor: Option<i64>,

    /// Minimum cursor (for backward pagination)
    pub min_cursor: Option<i64>,
}

// ============================================================
// Request Parameters
// ============================================================

/// Search parameters
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub keyword: String,
    pub offset: u32,
    pub count: u32,
    pub region: String,
    pub sort_type: u8,
    pub publish_time: u8,
}

impl SearchParams {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            offset: 0,
            count: 10,
            region: "US".to_string(),
            sort_type: 0,
            publish_time: 0,
        }
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = region.into();
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count.min(20); // TikHub max is 20
        self
    }

    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = offset;
        self
    }

    /// Set sort type (0=relevance, 1=most_liked)
    pub fn with_sort_type(mut self, sort_type: u8) -> Self {
        self.sort_type = sort_type;
        self
    }

    /// Set publish time filter (0=all, 1=day, 7=week, 30=month, 90=3months, 180=6months)
    pub fn with_publish_time(mut self, publish_time: u8) -> Self {
        self.publish_time = publish_time;
        self
    }
}

/// Comment fetch parameters
#[derive(Debug, Clone)]
pub struct CommentParams {
    pub cursor: String,
    pub count: u32,
}

impl Default for CommentParams {
    fn default() -> Self {
        Self::new()
    }
}

impl CommentParams {
    pub fn new() -> Self {
        Self {
            cursor: "0".to_string(),
            count: 100,
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = cursor.into();
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count.min(100); // TikHub max is 100
        self
    }
}

/// User video fetch parameters
#[derive(Debug, Clone, Default)]
pub struct UserVideoParams {
    pub sec_user_id: Option<String>,
    pub unique_id: Option<String>,
    pub max_cursor: i64,
    pub count: u32,
    pub sort_type: u8,
}

impl UserVideoParams {
    pub fn by_unique_id(unique_id: impl Into<String>) -> Self {
        Self {
            unique_id: Some(unique_id.into()),
            sec_user_id: None,
            max_cursor: 0,
            count: 20,
            sort_type: 0,
        }
    }

    pub fn by_sec_user_id(sec_user_id: impl Into<String>) -> Self {
        Self {
            sec_user_id: Some(sec_user_id.into()),
            unique_id: None,
            max_cursor: 0,
            count: 20,
            sort_type: 0,
        }
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count.min(20);
        self
    }
}

// ============================================================
// Conversion utilities
// ============================================================

impl AwemeInfo {
    /// Get the video description, falling back to empty string
    pub fn description(&self) -> &str {
        self.desc.as_deref().unwrap_or("")
    }

    /// Get author username
    pub fn author_username(&self) -> Option<&str> {
        self.author.as_ref()?.unique_id.as_deref()
    }

    /// Get author display name
    pub fn author_name(&self) -> Option<&str> {
        self.author.as_ref()?.nickname.as_deref()
    }

    /// Get like count
    pub fn likes(&self) -> i64 {
        self.statistics
            .as_ref()
            .and_then(|s| s.digg_count)
            .unwrap_or(0)
    }

    /// Get comment count
    pub fn comments(&self) -> i64 {
        self.statistics
            .as_ref()
            .and_then(|s| s.comment_count)
            .unwrap_or(0)
    }

    /// Get share count
    pub fn shares(&self) -> i64 {
        self.statistics
            .as_ref()
            .and_then(|s| s.share_count)
            .unwrap_or(0)
    }

    /// Get play/view count
    pub fn views(&self) -> i64 {
        self.statistics
            .as_ref()
            .and_then(|s| s.play_count)
            .unwrap_or(0)
    }
}

impl TikTokComment {
    /// Get comment text, falling back to empty string
    pub fn content(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }

    /// Check if this is a reply to another comment
    pub fn is_reply(&self) -> bool {
        self.reply_id
            .as_deref()
            .map(|id| id != "0")
            .unwrap_or(false)
    }

    /// Get commenter username
    pub fn username(&self) -> Option<&str> {
        self.user.as_ref()?.unique_id.as_deref()
    }

    /// Get commenter display name
    pub fn nickname(&self) -> Option<&str> {
        self.user.as_ref()?.nickname.as_deref()
    }

    /// Get like count
    pub fn likes(&self) -> i64 {
        self.digg_count.unwrap_or(0)
    }
}
