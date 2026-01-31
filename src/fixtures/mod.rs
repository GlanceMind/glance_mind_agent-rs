//! Fixture Generation and Loading
//!
//! Tools for generating test fixtures from real TikHub API responses
//! and loading them for testing.
//!
//! Supports multiple platforms:
//! - TikTok
//! - Instagram
//! - Reddit
//! - Twitter

mod generator;
mod loader;

pub use generator::FixtureGenerator;
pub use loader::FixtureLoader;

use crate::tikhub::{
    AwemeInfo, InstagramComment, InstagramPost, RedditComment, RedditPost, TikTokComment,
    TwitterTweet,
};
use serde::{Deserialize, Serialize};

// ============================================================
// TikTok Fixture Data Structures
// ============================================================

/// TikTok video search fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokSearchFixture {
    /// Search keyword used
    pub keyword: String,

    /// Region code
    pub region: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total videos in this fixture
    pub total_count: usize,

    /// Raw API response (for reference)
    pub raw_response: serde_json::Value,

    /// Extracted video list
    pub videos: Vec<serde_json::Value>,
}

/// TikTok comments fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokCommentsFixture {
    /// Video ID
    pub aweme_id: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total comments in this fixture
    pub total_count: usize,

    /// Comment list (raw JSON for flexibility)
    pub comments: Vec<serde_json::Value>,
}

/// TikTok user videos fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokUserVideosFixture {
    /// Username (unique_id)
    pub unique_id: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total videos in this fixture
    pub total_count: usize,

    /// Raw API response
    pub raw_response: serde_json::Value,

    /// Video list
    pub videos: Vec<serde_json::Value>,
}

impl TikTokSearchFixture {
    /// Parse videos into typed AwemeInfo structs
    pub fn parse_videos(&self) -> Vec<AwemeInfo> {
        self.videos
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect()
    }
}

impl TikTokCommentsFixture {
    /// Parse comments into typed TikTokComment structs
    pub fn parse_comments(&self) -> Vec<TikTokComment> {
        self.comments
            .iter()
            .filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect()
    }
}

impl TikTokUserVideosFixture {
    /// Parse videos into typed AwemeInfo structs
    pub fn parse_videos(&self) -> Vec<AwemeInfo> {
        self.videos
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect()
    }
}

// ============================================================
// Instagram Fixture Data Structures
// ============================================================

/// Instagram hashtag search fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramSearchFixture {
    /// Search keyword (hashtag)
    pub keyword: String,

    /// Feed type (top, recent, reels)
    pub feed_type: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total posts in this fixture
    pub total_count: usize,

    /// Raw API response (for reference)
    pub raw_response: serde_json::Value,

    /// Extracted post list
    pub posts: Vec<serde_json::Value>,
}

/// Instagram comments fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramCommentsFixture {
    /// Post code or URL
    pub code_or_url: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total comments in this fixture
    pub total_count: usize,

    /// Comment list (raw JSON for flexibility)
    pub comments: Vec<serde_json::Value>,
}

/// Instagram user posts fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramUserPostsFixture {
    /// Username
    pub username: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total posts in this fixture
    pub total_count: usize,

    /// Raw API response
    pub raw_response: serde_json::Value,

    /// Post list
    pub posts: Vec<serde_json::Value>,
}

impl InstagramSearchFixture {
    /// Parse posts into typed InstagramPost structs
    pub fn parse_posts(&self) -> Vec<InstagramPost> {
        self.posts
            .iter()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect()
    }
}

impl InstagramCommentsFixture {
    /// Parse comments into typed InstagramComment structs
    pub fn parse_comments(&self) -> Vec<InstagramComment> {
        self.comments
            .iter()
            .filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect()
    }
}

impl InstagramUserPostsFixture {
    /// Parse posts into typed InstagramPost structs
    pub fn parse_posts(&self) -> Vec<InstagramPost> {
        self.posts
            .iter()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect()
    }
}

// ============================================================
// Reddit Fixture Data Structures
// ============================================================

/// Reddit search fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSearchFixture {
    /// Search query
    pub query: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total posts in this fixture
    pub total_count: usize,

    /// Raw API response (for reference)
    pub raw_response: serde_json::Value,

    /// Extracted post list
    pub posts: Vec<serde_json::Value>,
}

/// Reddit comments fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditCommentsFixture {
    /// Post ID
    pub post_id: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total comments in this fixture
    pub total_count: usize,

    /// Comment list (raw JSON for flexibility)
    pub comments: Vec<serde_json::Value>,
}

/// Reddit user posts fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditUserPostsFixture {
    /// Username
    pub username: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total posts in this fixture
    pub total_count: usize,

    /// Raw API response
    pub raw_response: serde_json::Value,

    /// Post list
    pub posts: Vec<serde_json::Value>,
}

impl RedditSearchFixture {
    /// Parse posts into typed RedditPost structs
    pub fn parse_posts(&self) -> Vec<RedditPost> {
        self.posts
            .iter()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect()
    }
}

impl RedditCommentsFixture {
    /// Parse comments into typed RedditComment structs
    pub fn parse_comments(&self) -> Vec<RedditComment> {
        self.comments
            .iter()
            .filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect()
    }
}

impl RedditUserPostsFixture {
    /// Parse posts into typed RedditPost structs
    pub fn parse_posts(&self) -> Vec<RedditPost> {
        self.posts
            .iter()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect()
    }
}

// ============================================================
// Twitter Fixture Data Structures
// ============================================================

/// Twitter search fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterSearchFixture {
    /// Search keyword
    pub keyword: String,

    /// Search type (Latest, Top, Media, etc.)
    pub search_type: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total tweets in this fixture
    pub total_count: usize,

    /// Raw API response (for reference)
    pub raw_response: serde_json::Value,

    /// Extracted tweet list
    pub tweets: Vec<serde_json::Value>,
}

/// Twitter comments/replies fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterCommentsFixture {
    /// Tweet ID
    pub tweet_id: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total comments in this fixture
    pub total_count: usize,

    /// Comment/reply list (raw JSON for flexibility)
    pub comments: Vec<serde_json::Value>,
}

/// Twitter user tweets fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUserTweetsFixture {
    /// Screen name (handle)
    pub screen_name: String,

    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,

    /// Total tweets in this fixture
    pub total_count: usize,

    /// Raw API response
    pub raw_response: serde_json::Value,

    /// Tweet list
    pub tweets: Vec<serde_json::Value>,
}

impl TwitterSearchFixture {
    /// Parse tweets into typed TwitterTweet structs
    pub fn parse_tweets(&self) -> Vec<TwitterTweet> {
        self.tweets
            .iter()
            .filter_map(|t| serde_json::from_value(t.clone()).ok())
            .collect()
    }
}

impl TwitterCommentsFixture {
    /// Parse comments into typed TwitterTweet structs (replies are tweets)
    pub fn parse_comments(&self) -> Vec<TwitterTweet> {
        self.comments
            .iter()
            .filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect()
    }
}

impl TwitterUserTweetsFixture {
    /// Parse tweets into typed TwitterTweet structs
    pub fn parse_tweets(&self) -> Vec<TwitterTweet> {
        self.tweets
            .iter()
            .filter_map(|t| serde_json::from_value(t.clone()).ok())
            .collect()
    }
}
