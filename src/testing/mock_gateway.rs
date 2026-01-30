//! Mock Gateway implementations for testing

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::domain::{Content, Comment, KeywordType, SearchOptions};
use crate::domain::errors::{GatewayError, GatewayResult};
use crate::ports::{
    ContentGateway, CommentGateway,
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
};

/// Mock content gateway for testing
pub struct MockContentGateway {
    /// Stored contents by ID
    contents: RwLock<HashMap<String, Content>>,
    /// Contents by search keyword
    search_results: RwLock<HashMap<String, Vec<Content>>>,
    /// Contents by user ID
    user_contents: RwLock<HashMap<String, Vec<Content>>>,
    /// Whether to simulate errors
    error_mode: RwLock<Option<MockError>>,
    /// Call tracking
    calls: RwLock<Vec<GatewayCall>>,
}

/// Mock comment gateway for testing
pub struct MockCommentGateway {
    /// Comments by content ID
    comments: RwLock<HashMap<String, Vec<Comment>>>,
    /// Whether to simulate errors
    error_mode: RwLock<Option<MockError>>,
    /// Call tracking
    calls: RwLock<Vec<GatewayCall>>,
}

/// Simulated errors for testing error handling
#[derive(Debug, Clone)]
pub enum MockError {
    Network,
    RateLimit,
    NotFound,
    Auth,
}

/// Tracked gateway call
#[derive(Debug, Clone)]
pub enum GatewayCall {
    Search { query: String, count: u32 },
    FetchUser { user_id: String, count: u32 },
    FetchById { content_id: String },
    FetchComments { content_id: String, count: u32 },
    FetchAllComments { content_id: String, max: u32 },
}

impl MockContentGateway {
    /// Create a new mock content gateway
    pub fn new() -> Self {
        Self {
            contents: RwLock::new(HashMap::new()),
            search_results: RwLock::new(HashMap::new()),
            user_contents: RwLock::new(HashMap::new()),
            error_mode: RwLock::new(None),
            calls: RwLock::new(Vec::new()),
        }
    }

    /// Add a content item
    pub fn add_content(&self, content: Content) {
        let mut contents = self.contents.write().unwrap();
        contents.insert(content.content_id.clone(), content);
    }

    /// Add search results for a keyword
    pub fn add_search_results(&self, keyword: &str, results: Vec<Content>) {
        let mut search = self.search_results.write().unwrap();
        search.insert(keyword.to_lowercase(), results);
    }

    /// Add user content
    pub fn add_user_content(&self, user_id: &str, contents: Vec<Content>) {
        let mut user = self.user_contents.write().unwrap();
        user.insert(user_id.to_string(), contents);
    }

    /// Set error mode for testing error handling
    pub fn set_error_mode(&self, error: Option<MockError>) {
        let mut mode = self.error_mode.write().unwrap();
        *mode = error;
    }

    /// Get all tracked calls
    pub fn get_calls(&self) -> Vec<GatewayCall> {
        self.calls.read().unwrap().clone()
    }

    /// Clear tracked calls
    pub fn clear_calls(&self) {
        self.calls.write().unwrap().clear();
    }

    fn track_call(&self, call: GatewayCall) {
        self.calls.write().unwrap().push(call);
    }

    fn check_error(&self) -> GatewayResult<()> {
        let mode = self.error_mode.read().unwrap();
        match mode.as_ref() {
            Some(MockError::Network) => Err(GatewayError::Network("Mock network error".into())),
            Some(MockError::RateLimit) => Err(GatewayError::RateLimited { retry_after_secs: Some(60) }),
            Some(MockError::NotFound) => Err(GatewayError::NotFound("Mock not found".into())),
            Some(MockError::Auth) => Err(GatewayError::AuthFailed("Mock auth error".into())),
            None => Ok(()),
        }
    }
}

impl Default for MockContentGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentGateway for MockContentGateway {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        self.track_call(GatewayCall::Search {
            query: options.query.clone(),
            count: options.count,
        });
        self.check_error()?;

        let search = self.search_results.read().unwrap();
        let key = options.query.to_lowercase();
        
        Ok(search
            .get(&key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(options.count as usize)
            .collect())
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        self.check_error()?;

        match keyword {
            KeywordType::Search(q) | KeywordType::Hashtag(q) => {
                let mut opts = options.clone();
                opts.query = q.clone();
                self.search(&opts).await
            }
            KeywordType::UserId(uid) | KeywordType::SecUserId(uid) => {
                self.fetch_user_content(uid, options.count).await
            }
            KeywordType::ContentId(cid) => {
                match self.fetch_by_id(cid).await? {
                    Some(c) => Ok(vec![c]),
                    None => Ok(vec![]),
                }
            }
        }
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        self.track_call(GatewayCall::FetchUser {
            user_id: user_id.to_string(),
            count,
        });
        self.check_error()?;

        let user = self.user_contents.read().unwrap();
        Ok(user
            .get(user_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(count as usize)
            .collect())
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        self.track_call(GatewayCall::FetchById {
            content_id: content_id.to_string(),
        });
        self.check_error()?;

        let contents = self.contents.read().unwrap();
        Ok(contents.get(content_id).cloned())
    }

    fn platform(&self) -> &str {
        "mock"
    }
}

impl MockCommentGateway {
    /// Create a new mock comment gateway
    pub fn new() -> Self {
        Self {
            comments: RwLock::new(HashMap::new()),
            error_mode: RwLock::new(None),
            calls: RwLock::new(Vec::new()),
        }
    }

    /// Add comments for a content ID
    pub fn add_comment(&self, content_id: &str, comment: Comment) {
        let mut comments = self.comments.write().unwrap();
        comments
            .entry(content_id.to_string())
            .or_default()
            .push(comment);
    }

    /// Add multiple comments for a content ID
    pub fn add_comments(&self, content_id: &str, new_comments: Vec<Comment>) {
        let mut comments = self.comments.write().unwrap();
        comments
            .entry(content_id.to_string())
            .or_default()
            .extend(new_comments);
    }

    /// Set error mode
    pub fn set_error_mode(&self, error: Option<MockError>) {
        let mut mode = self.error_mode.write().unwrap();
        *mode = error;
    }

    /// Get all tracked calls
    pub fn get_calls(&self) -> Vec<GatewayCall> {
        self.calls.read().unwrap().clone()
    }

    fn track_call(&self, call: GatewayCall) {
        self.calls.write().unwrap().push(call);
    }

    fn check_error(&self) -> GatewayResult<()> {
        let mode = self.error_mode.read().unwrap();
        match mode.as_ref() {
            Some(MockError::Network) => Err(GatewayError::Network("Mock network error".into())),
            Some(MockError::RateLimit) => Err(GatewayError::RateLimited { retry_after_secs: Some(60) }),
            Some(MockError::NotFound) => Err(GatewayError::NotFound("Mock not found".into())),
            Some(MockError::Auth) => Err(GatewayError::AuthFailed("Mock auth error".into())),
            None => Ok(()),
        }
    }
}

impl Default for MockCommentGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommentGateway for MockCommentGateway {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        self.track_call(GatewayCall::FetchComments {
            content_id: content_id.to_string(),
            count: options.count,
        });
        self.check_error()?;

        let comments = self.comments.read().unwrap();
        let all = comments.get(content_id).cloned().unwrap_or_default();
        
        let fetched: Vec<Comment> = all
            .into_iter()
            .take(options.count as usize)
            .collect();

        Ok(FetchCommentsResult {
            comments: fetched,
            has_more: false,
            next_cursor: None,
            total: None,
        })
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        self.track_call(GatewayCall::FetchAllComments {
            content_id: content_id.to_string(),
            max: max_count,
        });
        self.check_error()?;

        let comments = self.comments.read().unwrap();
        Ok(comments
            .get(content_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(max_count as usize)
            .collect())
    }

    async fn fetch_replies(
        &self,
        _content_id: &str,
        _comment_id: &str,
        _options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        self.check_error()?;
        Ok(vec![])
    }

    fn platform(&self) -> &str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_content_gateway_search() {
        let gateway = MockContentGateway::new();
        
        // Add test content
        let content = Content::new("mock", "v123")
            .with_author("testuser")
            .with_description("Test video about fitness");
        
        gateway.add_search_results("fitness", vec![content.clone()]);

        // Search
        let opts = SearchOptions::new("fitness");
        let results = gateway.search(&opts).await.unwrap();
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content_id, "v123");
        
        // Check call tracking
        let calls = gateway.get_calls();
        assert_eq!(calls.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_content_gateway_error() {
        let gateway = MockContentGateway::new();
        gateway.set_error_mode(Some(MockError::RateLimit));

        let opts = SearchOptions::new("test");
        let result = gateway.search(&opts).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GatewayError::RateLimited { .. }));
    }

    #[tokio::test]
    async fn test_mock_comment_gateway() {
        let gateway = MockCommentGateway::new();
        
        let comment = Comment::new("mock", "c1", "v123")
            .with_author("user1")
            .with_text("Great video!");
        
        gateway.add_comment("v123", comment);

        let comments = gateway.fetch_all_comments("v123", 10).await.unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "Great video!");
    }
}
