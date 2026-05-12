//! Adapters - Implementations of ports for external systems
//!
//! ## Outbound Adapters
//! - `FacebookAdapter`: RapidAPI Facebook Scraper3 for Facebook posts and comments
//! - `TikHubAdapter`: TikHub API for TikTok content and comments
//! - `InstagramAdapter`: TikHub API for Instagram content and comments
//! - `RedditAdapter`: TikHub API for Reddit content and comments
//! - `TwitterAdapter`: TikHub API for Twitter content and comments
//! - `OpenAiAdapter`: DeepSeek OpenAI-compatible API for AI analysis
//! - `PostgresAdapter`: PostgreSQL for data persistence
//! - `FixtureMockAdapter`: Test fixtures for mocking
//!
//! ## Inbound Adapters
//! - `RedisTaskConsumer`: Redis queue for task consumption

pub mod facebook;
pub mod instagram;
pub mod mock;
pub mod openai;
pub mod postgres;
pub mod reddit;
pub mod redis;
pub mod tikhub;
pub mod twitter;

pub use facebook::FacebookAdapter;
pub use instagram::InstagramAdapter;
pub use mock::FixtureMockAdapter;
pub use openai::OpenAiAdapter;
pub use postgres::PostgresAdapter;
pub use reddit::RedditAdapter;
pub use redis::{CrawlerTaskBuilder, CrawlerTaskExt, RedisTaskConsumer, TaskResult};
pub use tikhub::TikHubAdapter;
pub use twitter::TwitterAdapter;
