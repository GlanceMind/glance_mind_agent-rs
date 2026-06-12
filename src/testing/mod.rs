//! Testing utilities - Mock clients and test helpers
//!
//! This module provides mock implementations for testing:
//! - MockContentGateway: Mock content/video fetching
//! - MockCommentGateway: Mock comment fetching
//! - MockAiAnalyzer: Mock AI analysis
//! - MockRepository: In-memory database mock
//! - TestFixtures: Pre-built test data

pub mod fixtures;
pub mod mock_ai;
pub mod mock_gateway;
pub mod mock_repository;

/// Shared mock HTTP server for adapter unit tests (RT-2). Test-only: the five
/// adapter test modules used to each carry a private copy of this helper.
#[cfg(test)]
pub(crate) mod mock_http;

pub use fixtures::TestFixtures;
pub use mock_ai::MockAiAnalyzer;
pub use mock_gateway::{MockCommentGateway, MockContentGateway};
pub use mock_repository::MockRepository;

/// Create a complete mock environment for testing
pub struct MockEnvironment {
    pub content_gateway: std::sync::Arc<MockContentGateway>,
    pub comment_gateway: std::sync::Arc<MockCommentGateway>,
    pub ai_analyzer: std::sync::Arc<MockAiAnalyzer>,
    pub repository: std::sync::Arc<MockRepository>,
}

impl MockEnvironment {
    /// Create a new mock environment with default test data
    pub fn new() -> Self {
        let fixtures = TestFixtures::default();

        let content_gateway = std::sync::Arc::new(MockContentGateway::new());
        let comment_gateway = std::sync::Arc::new(MockCommentGateway::new());
        let ai_analyzer = std::sync::Arc::new(MockAiAnalyzer::new());
        let repository = std::sync::Arc::new(MockRepository::new());

        // Pre-populate with test data
        for content in fixtures.contents() {
            content_gateway.add_content(content);
        }
        for (content_id, comments) in fixtures.comments() {
            for comment in comments {
                comment_gateway.add_comment(&content_id, comment);
            }
        }

        Self {
            content_gateway,
            comment_gateway,
            ai_analyzer,
            repository,
        }
    }

    /// Create with custom fixtures
    pub fn with_fixtures(fixtures: &TestFixtures) -> Self {
        let content_gateway = std::sync::Arc::new(MockContentGateway::new());
        let comment_gateway = std::sync::Arc::new(MockCommentGateway::new());
        let ai_analyzer = std::sync::Arc::new(MockAiAnalyzer::new());
        let repository = std::sync::Arc::new(MockRepository::new());

        for content in fixtures.contents() {
            content_gateway.add_content(content);
        }
        for (content_id, comments) in fixtures.comments() {
            for comment in comments {
                comment_gateway.add_comment(&content_id, comment);
            }
        }

        Self {
            content_gateway,
            comment_gateway,
            ai_analyzer,
            repository,
        }
    }
}

impl Default for MockEnvironment {
    fn default() -> Self {
        Self::new()
    }
}
