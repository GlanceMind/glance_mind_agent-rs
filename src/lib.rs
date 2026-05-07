//! GlanceMind Agent - Rust Implementation
//!
//! AI-powered social media comment analysis agent supporting multiple platforms.
//!
//! ## Architecture
//!
//! This crate follows a hexagonal architecture with:
//! - **Config**: Application configuration (platform registry, settings)
//! - **Domain**: Core business entities and logic
//! - **Ports**: Trait definitions for external dependencies
//! - **Adapters**: Concrete implementations (TikHub, PostgreSQL, OpenAI)
//! - **Platform**: Platform management (TikTok, etc.)
//! - **Strategies**: Platform-specific behavior
//! - **Orchestrator**: Workflow coordination using ports
//! - **Testing**: Mock implementations and test utilities
//! - **Fixtures**: Test data generation from real APIs

// Configuration (loaded at startup)
pub mod config;

// Core layers
pub mod adapters;
pub mod domain;
pub mod orchestrator;
pub mod platform;
pub mod ports;
pub mod strategies;
pub mod worker;

// Testing utilities
pub mod testing;

// Infrastructure
pub mod concurrency;
pub mod db;
pub mod error;
pub mod fixtures;
pub mod protocol_gen;
pub mod tikhub;

// Re-exports for convenience
pub use error::{Error, Result};
pub use fixtures::{FixtureGenerator, FixtureLoader};
pub use tikhub::TikHubClient;

// Config re-exports (platform registry loaded from database)
pub use config::platform::{global_registry, init_global_registry};
pub use config::{PlatformInfo, PlatformLookup, PlatformRegistry};

// Domain re-exports
pub use domain::{
    Comment, CommentIntent, ConcurrencyConfig, Content, Engagement, KeywordType, ReplySuggestion,
    SearchOptions, Sentiment, TaskConfig, TaskResult,
};

// Concurrency re-exports
pub use concurrency::{
    AiRateLimiter, GlobalRateLimiters, RateLimitError, RateLimitResult, RateLimiter,
    TikHubRateLimiter,
};
pub use domain::errors::{
    AiError, AiResult, DbError, DbResult, GatewayError, GatewayResult, QueueError, QueueResult,
    WorkflowError, WorkflowResult,
};

// Ports re-exports
pub use ports::{
    AiAnalyzer, CommentGateway, ContentGateway, ContentRepository, ProgressTracker,
    PromptRepository,
};

// Strategy re-exports
pub use strategies::{
    FacebookStrategy, InstagramStrategy, PlatformStrategy, RedditStrategy, StrategyRegistry,
    TikTokStrategy, TwitterStrategy,
};

// Orchestrator re-exports
pub use orchestrator::{OrchestratorBuilder, OrchestratorConfig, WorkflowOrchestrator};

// Adapter re-exports
pub use adapters::redis::TaskResult as RedisTaskResult;
pub use adapters::{CrawlerTaskBuilder, CrawlerTaskExt, RedisTaskConsumer};
pub use adapters::{
    FacebookAdapter, FixtureMockAdapter, OpenAiAdapter, PostgresAdapter, TikHubAdapter,
};
pub use adapters::{InstagramAdapter, RedditAdapter, TwitterAdapter};

// Protocol re-exports (task types from glance_mind_protocol)
pub use protocol_gen::{
    CrawlerTask, CrawlerTaskMeta, CrawlerTaskSpec, Platform as ProtocolPlatform,
    TaskConfig as ProtocolTaskConfig, TaskFilters,
};

// Worker re-exports
pub use worker::{MultiPlatformWorker, WorkerBuilder, WorkerConfig};

// Platform re-exports (Platform trait, not to be confused with PlatformRegistry from config)
pub use platform::registry::PlatformRegistry as PlatformInstanceRegistry;
pub use platform::{Platform as PlatformTrait, TikTokConfig, TikTokPlatform};

// Testing re-exports (for integration tests)
pub use testing::{
    MockAiAnalyzer, MockCommentGateway, MockContentGateway, MockEnvironment, MockRepository,
    TestFixtures,
};
