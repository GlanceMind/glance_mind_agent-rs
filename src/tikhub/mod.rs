//! TikHub API Client
//!
//! Provides typed access to TikHub API for TikTok, Instagram, Reddit, and Twitter
//! data retrieval with robust error handling and automatic retry.
//!
//! ## Supported Endpoints
//!
//! ### TikTok
//! - `/api/v1/tiktok/app/v3/fetch_video_search_result` - Search videos by keyword
//! - `/api/v1/tiktok/web/fetch_post_comment` - Fetch video comments
//! - `/api/v1/tiktok/app/v3/fetch_user_post_videos` - Fetch user's videos
//!
//! ### Instagram
//! - `/api/v1/instagram/v2/fetch_hashtag_posts` - Search posts by hashtag
//! - `/api/v1/instagram/v2/search_reels` - Search Reels
//! - `/api/v1/instagram/v2/fetch_user_posts` - Fetch user's posts
//! - `/api/v1/instagram/v2/fetch_post_comments` - Fetch post comments
//!
//! ### Reddit
//! - `/api/v1/reddit/app/fetch_dynamic_search` - Search posts
//! - `/api/v1/reddit/app/fetch_post_comments` - Fetch post comments
//! - `/api/v1/reddit/app/fetch_user_posts` - Fetch user's posts
//!
//! ### Twitter
//! - `/api/v1/twitter/web/fetch_search_timeline` - Search tweets
//! - `/api/v1/twitter/web/fetch_user_post_tweet` - Get user tweets
//! - `/api/v1/twitter/web/fetch_post_comments` - Get tweet comments/replies
//!
//! ## Error Handling
//!
//! The client handles TikHub HTTP status codes appropriately:
//! - 400: Bad Request - retried once, then skipped
//! - 401: Unauthorized - fatal, task terminated
//! - 402: Payment Required - fatal, task terminated
//! - 403: Forbidden - fatal, task terminated
//! - 404: Not Found - skipped, continue with next item
//! - 429: Rate Limited - retry with exponential backoff
//! - 500+: Server Error - retry with exponential backoff
//!
//! ## Example
//!
//! ```no_run
//! use glance_mind_agent_rs::tikhub::{TikHubClient, SearchParams, HashtagSearchParams};
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = TikHubClient::from_env().unwrap();
//!     
//!     // TikTok: Search with automatic retry
//!     let response = client
//!         .search_videos_with_retry(&SearchParams::new("travel"))
//!         .await
//!         .unwrap();
//!     
//!     // Instagram: Search hashtag posts
//!     let instagram_response = client
//!         .search_hashtag_posts_with_retry(&HashtagSearchParams::new("fitness"))
//!         .await
//!         .unwrap();
//! }
//! ```

mod client;
mod error;
mod instagram_types;
mod reddit_types;
mod twitter_types;
mod types;

pub use client::TikHubClient;
pub use error::{PartialFetchResult, TikHubError, TikHubRetryConfig};
pub use instagram_types::*;
pub use reddit_types::*;
pub use twitter_types::*;
pub use types::*;
