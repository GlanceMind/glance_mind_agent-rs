//! Twitter TikHub API Response Types
//!
//! These types are derived from TikHub Twitter Web API responses.
//! Based on endpoints:
//! - `/api/v1/twitter/web/fetch_search_timeline` - Search tweets
//! - `/api/v1/twitter/web/fetch_user_post_tweet` - Get user tweets
//! - `/api/v1/twitter/web/fetch_post_comments` - Get tweet comments/replies
//! - `/api/v1/twitter/web/fetch_tweet_detail` - Get single tweet detail

use serde::{de, Deserialize, Serialize};
use serde_json::Value;

// ============================================================
// Common Response Types
// ============================================================

/// Twitter API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterResponse<T> {
    pub code: i32,
    #[serde(default)]
    pub message: String,
    pub data: Option<T>,
}

// ============================================================
// Search Timeline API Types
// Endpoint: /api/v1/twitter/web/fetch_search_timeline
// ============================================================

/// Search timeline response
pub type TwitterSearchResponse = TwitterResponse<TwitterTimelineData>;

/// Timeline data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterTimelineData {
    /// Status indicator
    pub status: Option<String>,

    /// List of tweets in the timeline
    pub timeline: Option<Vec<TwitterTweet>>,

    /// Cursor for next page
    pub next_cursor: Option<String>,

    /// Cursor for previous page
    pub prev_cursor: Option<String>,
}

// ============================================================
// User Post Tweet API Types
// Endpoint: /api/v1/twitter/web/fetch_user_post_tweet
// ============================================================

/// User tweets response
pub type TwitterUserTweetsResponse = TwitterResponse<TwitterUserTimelineData>;

/// User timeline data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUserTimelineData {
    /// Pinned tweet (optional)
    pub pinned: Option<TwitterTweet>,

    /// List of timeline tweets
    pub timeline: Option<Vec<TwitterTweet>>,

    /// Cursor for next page
    pub next_cursor: Option<String>,
}

// ============================================================
// Post Comments API Types
// Endpoint: /api/v1/twitter/web/fetch_post_comments
// ============================================================

/// Tweet comments response
pub type TwitterCommentsResponse = TwitterResponse<TwitterCommentsData>;

/// Comments data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterCommentsData {
    /// Original tweet's like count
    pub likes: Option<i64>,

    /// Original tweet's text
    pub text: Option<String>,

    /// Thread of replies/comments
    pub thread: Option<Vec<TwitterTweet>>,

    /// Cursor for next page
    pub next_cursor: Option<String>,
}

/// Tweet detail response.
///
/// TikHub's OpenAPI spec currently models `data` as a generic value, so we
/// keep the raw payload and normalize it into `TwitterTweet` via
/// `extract_tweet_from_detail_response`.
pub type TwitterTweetDetailResponse = TwitterResponse<Value>;

// ============================================================
// Twitter Tweet Data Structure
// ============================================================

/// Twitter tweet/comment information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TwitterTweet {
    /// Tweet ID
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub tweet_id: Option<String>,

    /// Tweet ID (alternative field name for comments)
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub id: Option<String>,

    /// REST ID (seen in some TikHub Twitter payloads)
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub rest_id: Option<String>,

    /// Type indicator (usually "tweet")
    #[serde(rename = "type")]
    pub tweet_type: Option<String>,

    /// Tweet text content
    pub text: Option<String>,

    /// Screen name (handle)
    pub screen_name: Option<String>,

    /// Created at timestamp string
    pub created_at: Option<String>,

    /// Conversation ID
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub conversation_id: Option<String>,

    /// Language
    pub lang: Option<String>,

    /// Bookmark count
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub bookmarks: Option<i64>,

    /// Favorite/like count
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub favorites: Option<i64>,

    /// Like count (alternative field name)
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub likes: Option<i64>,

    /// Quote count
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub quotes: Option<i64>,

    /// Reply count
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub replies: Option<i64>,

    /// Retweet count
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub retweets: Option<i64>,

    /// View count (may be string or number)
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub views: Option<i64>,

    /// User information
    pub user_info: Option<TwitterUser>,

    /// Author information (alternative for comments)
    pub author: Option<TwitterUser>,

    /// Media attachments
    pub media: Option<TwitterMedia>,

    /// Entities (hashtags, urls, mentions)
    pub entities: Option<TwitterEntities>,

    /// In reply to status ID
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub in_reply_to_status_id_str: Option<String>,

    /// In reply to user ID
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub in_reply_to_user_id_str: Option<String>,
}

fn deserialize_optional_stringish<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(Value::Bool(value)) => Ok(Some(value.to_string())),
        Some(other) => Err(de::Error::custom(format!(
            "expected string/number/bool/null, got {other}"
        ))),
    }
}

fn deserialize_optional_i64ish<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_i64()
            .or_else(|| value.as_u64().map(|unsigned| unsigned as i64))
            .map(Some)
            .ok_or_else(|| de::Error::custom(format!("invalid numeric value: {value}"))),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                trimmed
                    .parse::<i64>()
                    .map(Some)
                    .map_err(|err| de::Error::custom(format!("invalid integer string `{trimmed}`: {err}")))
            }
        }
        Some(other) => Err(de::Error::custom(format!(
            "expected number/string/null, got {other}"
        ))),
    }
}

/// Twitter user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUser {
    /// User ID (rest_id)
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub rest_id: Option<String>,

    /// Display name
    pub name: Option<String>,

    /// Screen name (handle)
    pub screen_name: Option<String>,

    /// User description/bio
    pub description: Option<String>,

    /// Follower count
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub followers_count: Option<i64>,

    /// Avatar URL
    pub avatar: Option<String>,

    /// Is verified (legacy checkmark)
    pub verified: Option<bool>,

    /// Is blue verified (Twitter Blue)
    pub blue_verified: Option<bool>,
}

/// Twitter media attachments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TwitterMedia {
    /// Object format with video/photo keys
    Object {
        video: Option<Vec<TwitterVideoMedia>>,
        photo: Option<Vec<TwitterPhotoMedia>>,
    },
    /// List format (array of items)
    List(Vec<TwitterMediaItem>),
    /// Catch-all for other formats
    Other(serde_json::Value),
}

/// Twitter photo media object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterPhotoMedia {
    pub media_url_https: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub extra: Option<serde_json::Value>,
}

/// Twitter media item (when in list format)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TwitterMediaItem {
    Object {
        url: Option<String>,
        media_url_https: Option<String>,
    },
    Url(String),
    Other(serde_json::Value),
}

/// Twitter video media
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterVideoMedia {
    pub variants: Option<Vec<TwitterVideoVariant>>,
}

/// Twitter video variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterVideoVariant {
    pub content_type: Option<String>,
    pub url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i64ish")]
    pub bitrate: Option<i64>,
}

/// Twitter entities (hashtags, urls, mentions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterEntities {
    pub hashtags: Option<Vec<TwitterHashtag>>,
    pub urls: Option<Vec<TwitterUrl>>,
    pub user_mentions: Option<Vec<TwitterMention>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterHashtag {
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUrl {
    pub url: Option<String>,
    pub expanded_url: Option<String>,
    pub display_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterMention {
    pub screen_name: Option<String>,
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_stringish")]
    pub id_str: Option<String>,
}

// ============================================================
// Request Parameters
// ============================================================

/// Search parameters
#[derive(Debug, Clone, Default)]
pub struct TwitterSearchParams {
    pub keyword: String,
    pub search_type: String, // "Latest", "Top", "Media", "People", "Lists"
    pub cursor: Option<String>,
}

impl TwitterSearchParams {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            search_type: "Latest".to_string(),
            cursor: None,
        }
    }

    pub fn with_search_type(mut self, search_type: impl Into<String>) -> Self {
        self.search_type = search_type.into();
        self
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

/// User tweets fetch parameters
#[derive(Debug, Clone, Default)]
pub struct TwitterUserTweetsParams {
    pub rest_id: Option<String>,
    pub screen_name: Option<String>,
    pub cursor: Option<String>,
}

impl TwitterUserTweetsParams {
    pub fn by_rest_id(rest_id: impl Into<String>) -> Self {
        Self {
            rest_id: Some(rest_id.into()),
            screen_name: None,
            cursor: None,
        }
    }

    pub fn by_screen_name(screen_name: impl Into<String>) -> Self {
        Self {
            rest_id: None,
            screen_name: Some(screen_name.into()),
            cursor: None,
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

/// Tweet comments fetch parameters
#[derive(Debug, Clone, Default)]
pub struct TwitterCommentParams {
    pub tweet_id: String,
    pub cursor: Option<String>,
}

impl TwitterCommentParams {
    pub fn new(tweet_id: impl Into<String>) -> Self {
        Self {
            tweet_id: tweet_id.into(),
            cursor: None,
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

// ============================================================
// Conversion Utilities
// ============================================================

impl TwitterTweet {
    /// Get tweet ID
    pub fn get_tweet_id(&self) -> Option<&str> {
        self.tweet_id.as_deref().or(self.id.as_deref())
    }

    /// Get tweet text
    pub fn content(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }

    /// Get screen name (handle)
    pub fn author_handle(&self) -> Option<&str> {
        self.screen_name
            .as_deref()
            .or_else(|| self.user_info.as_ref()?.screen_name.as_deref())
            .or_else(|| self.author.as_ref()?.screen_name.as_deref())
    }

    /// Get display name
    pub fn author_name(&self) -> Option<&str> {
        self.user_info
            .as_ref()
            .and_then(|u| u.name.as_deref())
            .or_else(|| self.author.as_ref().and_then(|a| a.name.as_deref()))
    }

    /// Get user ID
    pub fn user_id(&self) -> Option<&str> {
        self.user_info
            .as_ref()
            .and_then(|u| u.rest_id.as_deref())
            .or_else(|| self.author.as_ref().and_then(|a| a.rest_id.as_deref()))
    }

    /// Get favorite/like count
    pub fn like_count(&self) -> i64 {
        self.favorites.or(self.likes).unwrap_or(0)
    }

    /// Get retweet count
    pub fn retweet_count(&self) -> i64 {
        self.retweets.unwrap_or(0)
    }

    /// Get reply count
    pub fn reply_count(&self) -> i64 {
        self.replies.unwrap_or(0)
    }

    /// Get quote count
    pub fn quote_count(&self) -> i64 {
        self.quotes.unwrap_or(0)
    }

    /// Get bookmark count
    pub fn bookmark_count(&self) -> i64 {
        self.bookmarks.unwrap_or(0)
    }

    /// Get view count
    pub fn view_count(&self) -> i64 {
        self.views.unwrap_or(0)
    }

    /// Check if this is a reply
    pub fn is_reply(&self) -> bool {
        self.in_reply_to_status_id_str.is_some()
    }

    /// Get created timestamp
    pub fn created_at_timestamp(&self) -> Option<i64> {
        let created_at = self.created_at.as_ref()?;

        // Parse Twitter's date format: "Fri Jan 09 22:17:51 +0000 2026"
        if let Ok(dt) = chrono::DateTime::parse_from_str(created_at, "%a %b %d %H:%M:%S %z %Y") {
            return Some(dt.timestamp());
        }

        None
    }

    /// Get media URLs
    pub fn media_urls(&self) -> Vec<String> {
        let mut urls = Vec::new();

        match &self.media {
            Some(TwitterMedia::List(items)) => {
                for item in items {
                    match item {
                        TwitterMediaItem::Url(url) => urls.push(url.clone()),
                        TwitterMediaItem::Object {
                            url,
                            media_url_https,
                        } => {
                            if let Some(u) = url {
                                urls.push(u.clone());
                            } else if let Some(u) = media_url_https {
                                urls.push(u.clone());
                            }
                        }
                        TwitterMediaItem::Other(_) => {}
                    }
                }
            }
            Some(TwitterMedia::Object { video, photo }) => {
                // Add videos (get highest quality mp4)
                if let Some(videos) = video {
                    for v in videos {
                        if let Some(variants) = &v.variants {
                            let mp4_variants: Vec<_> = variants
                                .iter()
                                .filter(|v| v.content_type.as_deref() == Some("video/mp4"))
                                .collect();
                            if let Some(best) =
                                mp4_variants.iter().max_by_key(|v| v.bitrate.unwrap_or(0))
                            {
                                if let Some(url) = &best.url {
                                    urls.push(url.clone());
                                }
                            }
                        }
                    }
                }
                // Add photos
                if let Some(photos) = photo {
                    for p in photos {
                        if let Some(u) = &p.media_url_https {
                            urls.push(u.clone());
                        }
                    }
                }
            }
            Some(TwitterMedia::Other(_)) => {}
            None => {}
        }

        urls
    }

    /// Check if tweet has media
    pub fn has_media(&self) -> bool {
        match &self.media {
            Some(TwitterMedia::List(items)) => !items.is_empty(),
            Some(TwitterMedia::Object { video, photo }) => {
                video.as_ref().map(|v| !v.is_empty()).unwrap_or(false)
                    || photo.as_ref().map(|p| !p.is_empty()).unwrap_or(false)
            }
            Some(TwitterMedia::Other(_)) => true, // Assume has media if Other
            None => false,
        }
    }
}

fn looks_like_tweet_payload(tweet: &TwitterTweet) -> bool {
    tweet.get_tweet_id().is_some()
        && (tweet.text.is_some() || tweet.created_at.is_some() || tweet.tweet_type.is_some())
}

fn normalize_tweet_from_value(mut tweet: TwitterTweet, value: &Value) -> TwitterTweet {
    if tweet.get_tweet_id().is_none() && tweet.text.is_some() {
        if let Some(rest_id) = value
            .get("rest_id")
            .and_then(|raw| match raw {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                _ => None,
            })
        {
            tweet.id = Some(rest_id);
        }
    }

    tweet
}

/// Extract a single tweet from the flexible TikHub tweet-detail response.
pub fn extract_tweet_from_detail_response(
    response: &TwitterTweetDetailResponse,
) -> Option<TwitterTweet> {
    response.data.as_ref().and_then(extract_tweet_from_value)
}

/// Extract a single tweet from a raw JSON value.
pub fn extract_tweet_from_value(value: &Value) -> Option<TwitterTweet> {
    match value {
        Value::Null => None,
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                serde_json::from_str::<Value>(trimmed)
                    .ok()
                    .and_then(|parsed| extract_tweet_from_value(&parsed))
            }
        }
        Value::Array(values) => values.iter().find_map(extract_tweet_from_value),
        Value::Object(object) => {
            if let Ok(tweet) = serde_json::from_value::<TwitterTweet>(value.clone()) {
                let tweet = normalize_tweet_from_value(tweet, value);
                if looks_like_tweet_payload(&tweet) {
                    return Some(tweet);
                }
            }

            for key in [
                "tweet",
                "data",
                "result",
                "tweet_result",
                "tweetResult",
                "status_result",
                "detail",
                "post",
            ] {
                if let Some(candidate) = object.get(key) {
                    if let Some(tweet) = extract_tweet_from_value(candidate) {
                        return Some(tweet);
                    }
                }
            }

            object.values().find_map(extract_tweet_from_value)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_twitter_search_params() {
        let params = TwitterSearchParams::new("rust programming")
            .with_search_type("Top")
            .with_cursor("cursor123");

        assert_eq!(params.keyword, "rust programming");
        assert_eq!(params.search_type, "Top");
        assert_eq!(params.cursor, Some("cursor123".to_string()));
    }

    #[test]
    fn test_twitter_tweet_helpers() {
        let tweet = TwitterTweet {
            tweet_id: Some("123456".to_string()),
            id: None,
            rest_id: None,
            tweet_type: Some("tweet".to_string()),
            text: Some("Test tweet content".to_string()),
            screen_name: Some("testuser".to_string()),
            created_at: Some("Fri Jan 09 22:17:51 +0000 2026".to_string()),
            conversation_id: None,
            lang: Some("en".to_string()),
            bookmarks: Some(5),
            favorites: Some(100),
            likes: None,
            quotes: Some(10),
            replies: Some(25),
            retweets: Some(50),
            views: Some(1000),
            user_info: Some(TwitterUser {
                rest_id: Some("u123".to_string()),
                name: Some("Test User".to_string()),
                screen_name: Some("testuser".to_string()),
                description: None,
                followers_count: Some(500),
                avatar: None,
                verified: Some(false),
                blue_verified: Some(false),
            }),
            author: None,
            media: None,
            entities: None,
            in_reply_to_status_id_str: None,
            in_reply_to_user_id_str: None,
        };

        assert_eq!(tweet.get_tweet_id(), Some("123456"));
        assert_eq!(tweet.content(), "Test tweet content");
        assert_eq!(tweet.author_handle(), Some("testuser"));
        assert_eq!(tweet.author_name(), Some("Test User"));
        assert_eq!(tweet.like_count(), 100);
        assert_eq!(tweet.view_count(), 1000);
        assert_eq!(tweet.quote_count(), 10);
        assert_eq!(tweet.bookmark_count(), 5);
        assert!(!tweet.is_reply());
    }

    #[test]
    fn test_twitter_user_params() {
        let params = TwitterUserTweetsParams::by_screen_name("elonmusk").with_cursor("cursor456");

        assert_eq!(params.screen_name, Some("elonmusk".to_string()));
        assert!(params.rest_id.is_none());
        assert_eq!(params.cursor, Some("cursor456".to_string()));
    }

    #[test]
    fn test_extract_tweet_detail_from_wrapped_payload() {
        let response: TwitterTweetDetailResponse = serde_json::from_value(serde_json::json!({
            "code": 200,
            "message": "success",
            "data": {
                "tweet": {
                    "tweet_id": "1808168603721650364",
                    "type": "tweet",
                    "text": "Detail payload tweet",
                    "created_at": "Fri Jan 09 22:17:51 +0000 2026",
                    "user_info": {
                        "rest_id": "42",
                        "screen_name": "jack",
                        "name": "Jack"
                    }
                }
            }
        }))
        .unwrap();

        let tweet =
            extract_tweet_from_detail_response(&response).expect("wrapped payload should yield a tweet");
        assert_eq!(tweet.get_tweet_id(), Some("1808168603721650364"));
        assert_eq!(tweet.author_handle(), Some("jack"));
    }

    #[test]
    fn test_extract_tweet_detail_from_stringified_payload() {
        let response: TwitterTweetDetailResponse = serde_json::from_value(serde_json::json!({
            "code": 200,
            "message": "success",
            "data": "{\"result\":{\"rest_id\":\"1808168603721650364\",\"text\":\"Detail payload tweet\",\"created_at\":\"Fri Jan 09 22:17:51 +0000 2026\",\"user_info\":{\"rest_id\":\"42\",\"screen_name\":\"jack\",\"name\":\"Jack\"}}}"
        }))
        .unwrap();

        let tweet = extract_tweet_from_detail_response(&response)
            .expect("stringified payload should be parsed into a tweet");
        assert_eq!(tweet.get_tweet_id(), Some("1808168603721650364"));
        assert_eq!(tweet.content(), "Detail payload tweet");
    }

    #[test]
    fn test_stringish_fields_deserialize_for_tweet_counts() {
        let tweet: TwitterTweet = serde_json::from_value(serde_json::json!({
            "tweet_id": 1808168603721650364u64,
            "text": "hello",
            "favorites": "12",
            "retweets": 3,
            "replies": "4",
            "quotes": 5,
            "bookmarks": "6",
            "views": "7"
        }))
        .unwrap();

        assert_eq!(tweet.get_tweet_id(), Some("1808168603721650364"));
        assert_eq!(tweet.like_count(), 12);
        assert_eq!(tweet.retweet_count(), 3);
        assert_eq!(tweet.reply_count(), 4);
        assert_eq!(tweet.quote_count(), 5);
        assert_eq!(tweet.bookmark_count(), 6);
        assert_eq!(tweet.view_count(), 7);
    }
}
