//! Instagram TikHub API Response Types
//!
//! These types are derived from TikHub Instagram API responses.
//! Based on endpoints:
//! - `/api/v1/instagram/v3/general_search` - Search posts by hashtag/query
//! - `/api/v1/instagram/v2/fetch_hashtag_posts` - Search posts by hashtag
//! - `/api/v1/instagram/v2/search_reels` - Search Reels
//! - `/api/v1/instagram/v2/fetch_user_posts` - Fetch user's posts
//! - `/api/v1/instagram/v2/fetch_user_reels` - Fetch user's Reels
//! - `/api/v1/instagram/v2/fetch_post_comments` - Fetch post comments
//! - `/api/v1/instagram/v2/fetch_comment_replies` - Fetch comment replies

use serde::{Deserialize, Serialize};

// ============================================================
// Common Response Types
// ============================================================

/// Instagram API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramResponse<T> {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    pub data: Option<T>,
}

/// Instagram pagination response data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramPaginatedData<T> {
    /// Nested data containing items
    pub data: Option<InstagramItemsData<T>>,
    /// Pagination token for next page
    pub pagination_token: Option<String>,
}

/// Instagram items data wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramItemsData<T> {
    pub items: Option<Vec<T>>,
}

// ============================================================
// Hashtag Search API Types
// Endpoint: /api/v1/instagram/v2/fetch_hashtag_posts
// ============================================================

/// Hashtag search response type alias
pub type HashtagSearchResponse = InstagramResponse<InstagramPaginatedData<InstagramPost>>;

// ============================================================
// General Search API Types
// Endpoint: /api/v1/instagram/v3/general_search
// ============================================================

/// V3 general search response type alias.
pub type GeneralSearchResponse = InstagramResponse<InstagramGeneralSearchData>;

/// V2 general search response type alias.
///
/// Endpoint: /api/v1/instagram/v2/general_search
/// Observed shape: { code, message, data: { data: { items: [...] }, pagination_token } }
pub type GeneralSearchV2Response = InstagramResponse<InstagramPaginatedData<InstagramPost>>;

/// V3 general search response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramGeneralSearchData {
    #[serde(default)]
    pub rank_token: Option<String>,
    #[serde(default)]
    pub clear_client_cache: Option<bool>,
    #[serde(default)]
    pub media_grid: Option<InstagramMediaGrid>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// Media grid returned by Instagram V3 general search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramMediaGrid {
    #[serde(default)]
    pub sections: Option<Vec<InstagramMediaGridSection>>,
    #[serde(default)]
    pub rank_token: Option<String>,
    #[serde(default)]
    pub next_max_id: Option<String>,
    #[serde(default)]
    pub has_more: Option<bool>,
    #[serde(default)]
    pub reels_max_id: Option<String>,
    #[serde(default)]
    pub has_more_reels: Option<bool>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// V3 grid section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramMediaGridSection {
    #[serde(default)]
    pub layout_type: Option<String>,
    #[serde(default)]
    pub feed_type: Option<String>,
    #[serde(default)]
    pub layout_content: Option<InstagramLayoutContent>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// V3 grid section content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramLayoutContent {
    #[serde(default)]
    pub medias: Option<Vec<InstagramMediaWrapper>>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// V3 media wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramMediaWrapper {
    #[serde(default)]
    pub media: Option<InstagramPost>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

// ============================================================
// Reels Search API Types
// Endpoint: /api/v1/instagram/v2/search_reels
// ============================================================

/// Reels search response type alias
pub type ReelsSearchResponse = InstagramResponse<InstagramPaginatedData<InstagramPost>>;

// ============================================================
// User Posts API Types
// Endpoint: /api/v1/instagram/v2/fetch_user_posts
// ============================================================

/// User posts response type alias
pub type UserPostsResponse = InstagramResponse<InstagramPaginatedData<InstagramPost>>;

// ============================================================
// User Reels API Types
// Endpoint: /api/v1/instagram/v2/fetch_user_reels
// ============================================================

/// User reels response type alias
pub type UserReelsResponse = InstagramResponse<InstagramPaginatedData<InstagramPost>>;

// ============================================================
// Post Comments API Types
// Endpoint: /api/v1/instagram/v2/fetch_post_comments
// ============================================================

/// Post comments response type alias
pub type PostCommentsResponse = InstagramResponse<InstagramPaginatedData<InstagramComment>>;

// ============================================================
// Comment Replies API Types
// Endpoint: /api/v1/instagram/v2/fetch_comment_replies
// ============================================================

/// Comment replies response type alias
pub type CommentRepliesResponse = InstagramResponse<InstagramPaginatedData<InstagramComment>>;

// ============================================================
// Instagram V1 API Types
// Endpoint: /api/v1/instagram/v1/fetch_hashtag_posts
// ============================================================

/// V1 Hashtag search response
pub type HashtagSearchV1Response = InstagramResponse<InstagramHashtagV1DataWrapper>;

/// V1 outer data wrapper (response has data.data.hashtag structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramHashtagV1DataWrapper {
    pub data: Option<InstagramHashtagV1Data>,
}

/// V1 Hashtag data wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramHashtagV1Data {
    pub hashtag: Option<InstagramHashtagV1>,
}

/// V1 Hashtag information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramHashtagV1 {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "edge_hashtag_to_media")]
    pub edge_hashtag_to_media: Option<InstagramEdgesWrapper>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// V1 Edges wrapper (for both media and comments)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramEdgesWrapper {
    pub count: Option<i64>,
    pub page_info: Option<InstagramV1PageInfo>,
    pub edges: Option<Vec<InstagramV1Edge>>,
}

/// V1 Page info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1PageInfo {
    pub has_next_page: Option<bool>,
    pub end_cursor: Option<String>,
}

/// V1 Edge wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1Edge {
    pub node: Option<InstagramV1Node>,
}

/// V1 Node (post data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1Node {
    pub id: Option<String>,
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
    pub shortcode: Option<String>,
    pub display_url: Option<String>,
    #[serde(rename = "taken_at_timestamp")]
    pub taken_at_timestamp: Option<i64>,
    pub dimensions: Option<InstagramV1Dimensions>,
    #[serde(rename = "edge_media_to_caption")]
    pub edge_media_to_caption: Option<InstagramEdgesWrapper>,
    #[serde(rename = "edge_media_to_comment")]
    pub edge_media_to_comment: Option<InstagramV1Count>,
    #[serde(rename = "edge_liked_by")]
    pub edge_liked_by: Option<InstagramV1Count>,
    pub owner: Option<InstagramV1Owner>,
    #[serde(rename = "is_video")]
    pub is_video: Option<bool>,
    #[serde(rename = "video_view_count")]
    pub video_view_count: Option<i64>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// V1 Dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1Dimensions {
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// V1 Count wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1Count {
    pub count: Option<i64>,
}

/// V1 Owner information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1Owner {
    pub id: Option<String>,
    pub username: Option<String>,
    #[serde(rename = "profile_pic_url")]
    pub profile_pic_url: Option<String>,
}

/// V1 Caption edge (for nested captions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1CaptionEdge {
    pub node: Option<InstagramV1CaptionNode>,
}

/// V1 Caption node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramV1CaptionNode {
    pub text: Option<String>,
}

// ============================================================
// Instagram Post/Reel Data Structure
// ============================================================

/// Instagram post/reel information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramPost {
    /// Post shortcode
    #[serde(default)]
    pub code: Option<String>,

    /// Post ID
    #[serde(default)]
    pub id: Option<String>,

    /// Numeric media PK returned by Instagram V3.
    #[serde(default)]
    pub pk: Option<String>,

    /// Product type (clips=reel, feed=post, igtv=igtv)
    #[serde(default)]
    pub product_type: Option<String>,

    /// Media type (1=photo, 2=video)
    #[serde(default)]
    pub media_type: Option<i32>,

    /// Caption text
    #[serde(default)]
    pub caption_text: Option<String>,

    /// Caption object (alternative structure)
    #[serde(default)]
    pub caption: Option<InstagramCaption>,

    /// Post author information
    #[serde(default)]
    pub user: Option<InstagramUser>,

    /// Like count
    #[serde(default)]
    pub like_count: Option<i64>,

    /// Comment count
    #[serde(default)]
    pub comment_count: Option<i64>,

    /// Play count (for videos/reels)
    #[serde(default)]
    pub play_count: Option<i64>,

    /// Thumbnail URL
    #[serde(default)]
    pub thumbnail_url: Option<String>,

    /// Image versions (multiple resolutions)
    #[serde(default, deserialize_with = "deserialize_image_versions")]
    pub image_versions: Option<Vec<InstagramImage>>,

    /// V3 image versions wrapper.
    #[serde(default)]
    pub image_versions2: Option<InstagramImageVersions2>,

    /// Video versions (multiple resolutions)
    #[serde(default)]
    pub video_versions: Option<Vec<InstagramVideo>>,

    /// Is video
    #[serde(default)]
    pub is_video: Option<bool>,

    /// Taken at timestamp (Unix epoch)
    #[serde(default)]
    pub taken_at_ts: Option<i64>,

    /// Taken at. Older responses may use an ISO string; V3 uses a Unix timestamp.
    #[serde(default)]
    pub taken_at: Option<serde_json::Value>,
}

/// Instagram caption object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramCaption {
    #[serde(default)]
    pub text: Option<String>,
}

/// Instagram user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramUser {
    /// User ID.
    pub id: Option<String>,

    /// Numeric user PK returned alongside id by Instagram V3.
    #[serde(default)]
    pub pk: Option<String>,

    /// Username
    #[serde(default)]
    pub username: Option<String>,

    /// Full display name
    #[serde(default)]
    pub full_name: Option<String>,

    /// Profile picture URL
    #[serde(default)]
    pub profile_pic_url: Option<String>,

    /// Is verified
    #[serde(default)]
    pub is_verified: Option<bool>,
}

/// Instagram image version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramImage {
    pub url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

fn deserialize_image_versions<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<InstagramImage>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(value) = Option::<serde_json::Value>::deserialize(deserializer)? else {
        return Ok(None);
    };

    match value {
        serde_json::Value::Array(items) => {
            Ok(serde_json::from_value(serde_json::Value::Array(items)).ok())
        }
        serde_json::Value::Object(mut object) => match object.remove("items") {
            Some(serde_json::Value::Array(items)) => {
                Ok(serde_json::from_value(serde_json::Value::Array(items)).ok())
            }
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

/// Instagram V3 image versions wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramImageVersions2 {
    #[serde(default)]
    pub candidates: Option<Vec<InstagramImage>>,
}

/// Instagram video version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramVideo {
    pub url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

// ============================================================
// Instagram Comment Data Structure
// ============================================================

/// Instagram comment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramComment {
    /// Comment ID
    #[serde(default)]
    pub id: Option<String>,

    /// Numeric comment PK returned by some Instagram payloads.
    #[serde(default)]
    pub pk: Option<String>,

    /// Comment text
    #[serde(default)]
    pub text: Option<String>,

    /// Comment author
    #[serde(default)]
    pub user: Option<InstagramUser>,

    /// Like count
    /// Note: TikHub returns both like_count and comment_like_count with same value,
    /// so we only map to like_count to avoid "duplicate field" serde error
    #[serde(default)]
    pub like_count: Option<i64>,

    /// Child comment count (replies)
    #[serde(default)]
    pub child_comment_count: Option<i32>,

    /// Created at timestamp (Unix epoch)
    /// Note: TikHub returns both created_at and created_at_utc with same value,
    /// so we only map to created_at to avoid "duplicate field" serde error
    #[serde(default)]
    pub created_at: Option<i64>,

    /// Parent comment ID (for replies)
    #[serde(default)]
    pub parent_comment_id: Option<String>,
}

// ============================================================
// Request Parameters
// ============================================================

/// Hashtag search parameters
#[derive(Debug, Clone, Default)]
pub struct HashtagSearchParams {
    pub keyword: String,
    pub feed_type: String, // "top", "recent", or "reels"
    pub pagination_token: Option<String>,
}

impl HashtagSearchParams {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            feed_type: "top".to_string(),
            pagination_token: None,
        }
    }

    pub fn with_feed_type(mut self, feed_type: impl Into<String>) -> Self {
        self.feed_type = feed_type.into();
        self
    }

    pub fn with_pagination_token(mut self, token: impl Into<String>) -> Self {
        self.pagination_token = Some(token.into());
        self
    }
}

/// Reels search parameters
#[derive(Debug, Clone, Default)]
pub struct ReelsSearchParams {
    pub keyword: String,
    pub pagination_token: Option<String>,
}

impl ReelsSearchParams {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            pagination_token: None,
        }
    }

    pub fn with_pagination_token(mut self, token: impl Into<String>) -> Self {
        self.pagination_token = Some(token.into());
        self
    }
}

/// User posts/reels fetch parameters
#[derive(Debug, Clone, Default)]
pub struct UserPostsParams {
    pub username: Option<String>,
    pub user_id: Option<String>,
    pub pagination_token: Option<String>,
}

impl UserPostsParams {
    pub fn by_username(username: impl Into<String>) -> Self {
        Self {
            username: Some(username.into()),
            user_id: None,
            pagination_token: None,
        }
    }

    pub fn by_user_id(user_id: impl Into<String>) -> Self {
        Self {
            username: None,
            user_id: Some(user_id.into()),
            pagination_token: None,
        }
    }

    pub fn with_pagination_token(mut self, token: impl Into<String>) -> Self {
        self.pagination_token = Some(token.into());
        self
    }
}

/// Comment fetch parameters
#[derive(Debug, Clone, Default)]
pub struct InstagramCommentParams {
    pub code_or_url: String,
    pub sort_by: String, // "recent" or "popular"
    pub pagination_token: Option<String>,
}

impl InstagramCommentParams {
    pub fn new(code_or_url: impl Into<String>) -> Self {
        Self {
            code_or_url: code_or_url.into(),
            sort_by: "recent".to_string(),
            pagination_token: None,
        }
    }

    pub fn with_sort_by(mut self, sort_by: impl Into<String>) -> Self {
        self.sort_by = sort_by.into();
        self
    }

    pub fn with_pagination_token(mut self, token: impl Into<String>) -> Self {
        self.pagination_token = Some(token.into());
        self
    }
}

/// Comment replies fetch parameters
#[derive(Debug, Clone, Default)]
pub struct CommentRepliesParams {
    pub code_or_url: String,
    pub comment_id: String,
    pub pagination_token: Option<String>,
}

impl CommentRepliesParams {
    pub fn new(code_or_url: impl Into<String>, comment_id: impl Into<String>) -> Self {
        Self {
            code_or_url: code_or_url.into(),
            comment_id: comment_id.into(),
            pagination_token: None,
        }
    }

    pub fn with_pagination_token(mut self, token: impl Into<String>) -> Self {
        self.pagination_token = Some(token.into());
        self
    }
}

// ============================================================
// Conversion Utilities
// ============================================================

impl InstagramPost {
    /// Get the post caption text
    pub fn caption_text_str(&self) -> &str {
        if let Some(ref text) = self.caption_text {
            if !text.is_empty() {
                return text;
            }
        }
        if let Some(ref caption) = self.caption {
            if let Some(ref text) = caption.text {
                return text;
            }
        }
        ""
    }

    /// Get author username
    pub fn author_username(&self) -> Option<&str> {
        self.user.as_ref()?.username.as_deref()
    }

    /// Get author display name
    pub fn author_name(&self) -> Option<&str> {
        self.user.as_ref()?.full_name.as_deref()
    }

    /// Get post ID
    pub fn post_id(&self) -> Option<&str> {
        self.pk
            .as_deref()
            .or(self.id.as_deref())
            .or(self.code.as_deref())
    }

    /// Check if this is a reel
    pub fn is_reel(&self) -> bool {
        self.product_type.as_deref() == Some("clips")
    }

    /// Check if this is a video
    pub fn is_video_content(&self) -> bool {
        self.is_video.unwrap_or(false) || self.media_type == Some(2) || self.is_reel()
    }

    /// Get like count
    pub fn likes(&self) -> i64 {
        self.like_count.unwrap_or(0)
    }

    /// Get comment count
    pub fn comments(&self) -> i64 {
        self.comment_count.unwrap_or(0)
    }

    /// Get play count
    pub fn views(&self) -> i64 {
        self.play_count.unwrap_or(0)
    }

    /// Get thumbnail URL
    pub fn thumbnail(&self) -> Option<&str> {
        if let Some(ref url) = self.thumbnail_url {
            if !url.is_empty() {
                return Some(url);
            }
        }
        if let Some(ref images) = self.image_versions {
            if let Some(first) = images.first() {
                return first.url.as_deref();
            }
        }
        if let Some(ref images) = self.image_versions2 {
            if let Some(first) = images.candidates.as_ref().and_then(|list| list.first()) {
                return first.url.as_deref();
            }
        }
        None
    }

    /// Get created timestamp
    pub fn created_at_timestamp(&self) -> Option<i64> {
        if self.taken_at_ts.is_some() {
            return self.taken_at_ts;
        }
        if let Some(ref taken_at) = self.taken_at {
            if let Some(ts) = taken_at.as_i64() {
                return Some(ts);
            }
            if let Some(text) = taken_at.as_str() {
                if let Ok(ts) = text.parse::<i64>() {
                    return Some(ts);
                }
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text) {
                    return Some(dt.timestamp());
                }
            }
        }
        None
    }
}

impl InstagramComment {
    /// Get comment text
    pub fn content(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }

    /// Get commenter username
    pub fn username(&self) -> Option<&str> {
        self.user.as_ref()?.username.as_deref()
    }

    /// Get commenter display name
    pub fn nickname(&self) -> Option<&str> {
        self.user.as_ref()?.full_name.as_deref()
    }

    /// Get commenter user ID
    pub fn user_id(&self) -> Option<&str> {
        self.user
            .as_ref()
            .and_then(|user| user.pk.as_deref().or(user.id.as_deref()))
    }

    /// Get like count
    pub fn likes(&self) -> i64 {
        self.like_count.unwrap_or(0)
    }

    /// Get reply count
    pub fn reply_count(&self) -> i32 {
        self.child_comment_count.unwrap_or(0)
    }

    /// Check if this is a reply
    pub fn is_reply(&self) -> bool {
        self.parent_comment_id.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashtag_search_params() {
        let params = HashtagSearchParams::new("fitness")
            .with_feed_type("recent")
            .with_pagination_token("abc123");

        assert_eq!(params.keyword, "fitness");
        assert_eq!(params.feed_type, "recent");
        assert_eq!(params.pagination_token, Some("abc123".to_string()));
    }

    #[test]
    fn test_instagram_post_helpers() {
        let post = InstagramPost {
            code: Some("ABC123".to_string()),
            id: Some("12345".to_string()),
            pk: None,
            product_type: Some("clips".to_string()),
            media_type: Some(2),
            caption_text: Some("Test caption".to_string()),
            caption: None,
            user: Some(InstagramUser {
                id: Some("u123".to_string()),
                pk: None,
                username: Some("testuser".to_string()),
                full_name: Some("Test User".to_string()),
                profile_pic_url: None,
                is_verified: Some(false),
            }),
            like_count: Some(100),
            comment_count: Some(10),
            play_count: Some(1000),
            thumbnail_url: None,
            image_versions: None,
            image_versions2: None,
            video_versions: None,
            is_video: Some(true),
            taken_at_ts: Some(1234567890),
            taken_at: None,
        };

        assert_eq!(post.caption_text_str(), "Test caption");
        assert_eq!(post.author_username(), Some("testuser"));
        assert_eq!(post.post_id(), Some("12345"));
        assert!(post.is_reel());
        assert!(post.is_video_content());
        assert_eq!(post.likes(), 100);
        assert_eq!(post.views(), 1000);
    }

    #[test]
    fn test_instagram_post_image_versions_legacy_array_deserializes() {
        let post: InstagramPost = serde_json::from_value(serde_json::json!({
            "code": "ABC123",
            "image_versions": [
                {
                    "url": "https://example.com/legacy.jpg",
                    "width": 640,
                    "height": 640
                }
            ]
        }))
        .expect("legacy image_versions array should deserialize");

        let images = post
            .image_versions
            .as_ref()
            .expect("legacy image_versions should be retained");
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].url.as_deref(),
            Some("https://example.com/legacy.jpg")
        );
        assert_eq!(post.thumbnail(), Some("https://example.com/legacy.jpg"));
    }

    #[test]
    fn test_instagram_post_image_versions_live_wrapper_deserializes() {
        let post: InstagramPost = serde_json::from_value(serde_json::json!({
            "code": "ABC123",
            "image_versions": {
                "items": [
                    {
                        "url": "https://example.com/live.jpg",
                        "width": 1080,
                        "height": 1080
                    }
                ],
                "additional_items": [],
                "animated_thumbnail_spritesheet_info_candidates": {},
                "scrubber_spritesheet_info_candidates": {}
            }
        }))
        .expect("live image_versions wrapper should deserialize");

        let images = post
            .image_versions
            .as_ref()
            .expect("live image_versions items should be retained");
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].url.as_deref(),
            Some("https://example.com/live.jpg")
        );
        assert_eq!(post.thumbnail(), Some("https://example.com/live.jpg"));
    }

    #[test]
    fn test_instagram_comment_helpers() {
        let comment = InstagramComment {
            id: Some("c123".to_string()),
            pk: None,
            text: Some("Great post!".to_string()),
            user: Some(InstagramUser {
                id: Some("u456".to_string()),
                pk: None,
                username: Some("commenter".to_string()),
                full_name: Some("Commenter Name".to_string()),
                profile_pic_url: None,
                is_verified: None,
            }),
            like_count: Some(50),
            child_comment_count: Some(5),
            created_at: Some(1234567890),
            parent_comment_id: None,
        };

        assert_eq!(comment.content(), "Great post!");
        assert_eq!(comment.username(), Some("commenter"));
        assert_eq!(comment.likes(), 50);
        assert!(!comment.is_reply());
    }
}
