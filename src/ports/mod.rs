//! Ports - Trait definitions for external dependencies
//!
//! Ports define the interfaces that the application core uses to interact
//! with external systems. Adapters implement these ports.
//!
//! ## Outbound Ports (driven)
//! - `ContentGateway`: Fetch content (videos, posts) from platforms
//! - `CommentGateway`: Fetch comments from platforms
//! - `AiAnalyzer`: Generate AI-powered reply suggestions
//! - `ContentRepository`: Persist and query content data
//! - `PromptRepository`: Query prompt templates and campaign config
//! - `ProgressTracker`: Track task progress and status
//!
//! ## Inbound Ports (driving)
//! - Task consumption is handled by adapters directly (Redis consumer)

pub mod ai_analyzer;
pub mod comment_gateway;
pub mod content_gateway;
pub mod content_repository;
pub mod progress_tracker;
pub mod prompt_repository;

pub use ai_analyzer::AiAnalyzer;
pub use comment_gateway::CommentGateway;
pub use content_gateway::ContentGateway;
pub use content_repository::ContentRepository;
pub use progress_tracker::ProgressTracker;
pub use prompt_repository::PromptRepository;
