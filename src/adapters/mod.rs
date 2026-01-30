//! Adapters - Implementations of ports for external systems
//!
//! ## Outbound Adapters
//! - `TikHubAdapter`: TikHub API for content and comments
//! - `OpenAiAdapter`: OpenAI/DeepSeek for AI analysis
//! - `PostgresAdapter`: PostgreSQL for data persistence
//! - `FixtureMockAdapter`: Test fixtures for mocking
//!
//! ## Inbound Adapters
//! - `RedisTaskConsumer`: Redis queue for task consumption

pub mod tikhub;
pub mod openai;
pub mod postgres;
pub mod mock;
pub mod redis;

pub use tikhub::TikHubAdapter;
pub use openai::OpenAiAdapter;
pub use postgres::PostgresAdapter;
pub use mock::FixtureMockAdapter;
pub use redis::{RedisTaskConsumer, TaskResult, CrawlerTaskExt, CrawlerTaskBuilder};
