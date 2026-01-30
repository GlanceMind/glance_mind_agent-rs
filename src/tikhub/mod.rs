//! TikHub API Client
//!
//! Provides typed access to TikHub API for TikTok data retrieval with
//! robust error handling and automatic retry.
//!
//! ## Supported Endpoints
//!
//! - `/api/v1/tiktok/app/v3/fetch_video_search_result` - Search videos by keyword
//! - `/api/v1/tiktok/web/fetch_post_comment` - Fetch video comments
//! - `/api/v1/tiktok/app/v3/fetch_user_post_videos` - Fetch user's videos
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
//! use glance_mind_agent_rs::tikhub::{TikHubClient, SearchParams};
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = TikHubClient::from_env().unwrap();
//!     
//!     // Search with automatic retry
//!     let response = client
//!         .search_videos_with_retry(&SearchParams::new("travel"))
//!         .await
//!         .unwrap();
//!     
//!     // Fetch comments with partial data recovery
//!     let result = client
//!         .fetch_all_comments_safe("7327061675382260482", 300)
//!         .await;
//!     
//!     if result.is_partial {
//!         println!("Warning: Only got {} comments due to error", result.data.len());
//!     }
//! }
//! ```

mod types;
mod client;
mod error;

pub use types::*;
pub use client::TikHubClient;
pub use error::{TikHubError, TikHubRetryConfig, PartialFetchResult};
