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
pub mod domain;
pub mod ports;
pub mod strategies;
pub mod platform;
pub mod orchestrator;
pub mod adapters;
pub mod worker;

// Testing utilities
pub mod testing;

// Infrastructure
pub mod tikhub;
pub mod fixtures;
pub mod protocol_gen;
pub mod db;
pub mod error;
pub mod concurrency;

// Re-exports for convenience
pub use error::{Error, Result};
pub use tikhub::TikHubClient;
pub use fixtures::{FixtureGenerator, FixtureLoader};

// Config re-exports (platform registry loaded from database)
pub use config::{PlatformInfo, PlatformLookup, PlatformRegistry};
pub use config::platform::{init_global_registry, global_registry};

// Domain re-exports
pub use domain::{
    Content, Comment, ReplySuggestion, Engagement,
    CommentIntent, Sentiment, TaskConfig, TaskResult,
    SearchOptions, KeywordType, ConcurrencyConfig,
};

// Concurrency re-exports
pub use concurrency::{RateLimiter, AiRateLimiter, TikHubRateLimiter, GlobalRateLimiters};
pub use domain::errors::{
    WorkflowError, GatewayError, AiError, DbError, QueueError,
    WorkflowResult, GatewayResult, AiResult, DbResult, QueueResult,
};

// Ports re-exports
pub use ports::{
    ContentGateway, CommentGateway, AiAnalyzer,
    ContentRepository, PromptRepository, ProgressTracker,
};

// Strategy re-exports
pub use strategies::{PlatformStrategy, StrategyRegistry, TikTokStrategy};

// Orchestrator re-exports
pub use orchestrator::{WorkflowOrchestrator, OrchestratorBuilder, OrchestratorConfig};

// Adapter re-exports
pub use adapters::{TikHubAdapter, OpenAiAdapter, PostgresAdapter, FixtureMockAdapter};
pub use adapters::{RedisTaskConsumer, CrawlerTaskExt, CrawlerTaskBuilder};
pub use adapters::redis::TaskResult as RedisTaskResult;

// Protocol re-exports (task types from glance_mind_protocol)
pub use protocol_gen::{
    CrawlerTask, CrawlerTaskMeta, CrawlerTaskSpec,
    TaskConfig as ProtocolTaskConfig, TaskFilters,
    Platform as ProtocolPlatform,
};

// Worker re-exports
pub use worker::{MultiPlatformWorker, WorkerConfig, WorkerBuilder};

// Platform re-exports (Platform trait, not to be confused with PlatformRegistry from config)
pub use platform::{Platform as PlatformTrait, TikTokPlatform, TikTokConfig};
pub use platform::registry::PlatformRegistry as PlatformInstanceRegistry;

// Testing re-exports (for integration tests)
pub use testing::{
    MockContentGateway, MockCommentGateway, MockAiAnalyzer, MockRepository,
    TestFixtures, MockEnvironment,
};
