//! Mock Gateway implementations for testing

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, KeywordType, SearchOptions};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    content_gateway::FetchOutcome,
    CommentGateway, ContentGateway,
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

    /// M1-T3 分页注入(FR-003):为 keyword 注入多页搜索结果(页间隐式 cursor 由 mock 内部生成)。
    ///
    /// 契约(m1-pagination-core.md §3 M1-T3 测试 2):
    /// - mock 的 `fetch_by_keyword_with_outcome` override 内部用 `PaginationLoop` 驱动
    ///   (共享构造 dogfooding;AG-005 合规:mock 的是上游数据,不是状态机);
    /// - **legacy 桥接(冻结)**:`add_search_pages` 同时使 legacy `fetch_by_keyword`
    ///   返回各页 flatten 后截断到 `options.count` 的结果。
    pub fn add_search_pages(&self, _keyword: &str, _pages: Vec<Vec<Content>>) {
        todo!("M1-T3 实现载荷:分页注入存储 + legacy flatten 桥接")
    }

    /// M1-T3:在第 `page_idx` 页(0-based)注入错误,翻页驱动行进至该页时触发。
    ///
    /// 语义(DR-10 冻结):PartialFailure 触发集 = 仅 `GatewayError::RateLimited` 且已有进展;
    /// 其余错误即使有进展也整体 `Err`;零进展(第 1 页即错)一律整体 `Err`(F-001)。
    pub fn set_page_error_at(&self, _page_idx: usize, _error: MockError) {
        todo!("M1-T3 实现载荷:页级错误注入存储")
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
            Some(MockError::RateLimit) => Err(GatewayError::RateLimited {
                retry_after_secs: Some(60),
            }),
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
            KeywordType::ContentId(cid) => match self.fetch_by_id(cid).await? {
                Some(c) => Ok(vec![c]),
                None => Ok(vec![]),
            },
        }
    }

    /// M1-T3 override 骨架:按注入页驱动 `PaginationLoop`,产出 `FetchOutcome`/`Err`
    /// (语义 = m1-pagination-core.md §3 M1-T3 测试 2/3/4/6;DR-10 触发集冻结)。
    async fn fetch_by_keyword_with_outcome(
        &self,
        _keyword: &KeywordType,
        _options: &SearchOptions,
    ) -> GatewayResult<FetchOutcome> {
        todo!("M1-T3 实现载荷:分页注入驱动 + shortfall 产出")
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
            Some(MockError::RateLimit) => Err(GatewayError::RateLimited {
                retry_after_secs: Some(60),
            }),
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

        let fetched: Vec<Comment> = all.into_iter().take(options.count as usize).collect();

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
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::RateLimited { .. }
        ));
    }

    #[tokio::test]
    async fn test_provider_harness_content_gateway_error_boundaries() {
        let cases = vec![
            (MockError::Network, "network"),
            (MockError::RateLimit, "rate_limit"),
            (MockError::NotFound, "not_found"),
            (MockError::Auth, "auth"),
        ];

        for (mode, boundary) in cases {
            let gateway = MockContentGateway::new();
            gateway.set_error_mode(Some(mode));

            let result = gateway.search(&SearchOptions::new("harness")).await;
            let error = result.expect_err(boundary);

            match boundary {
                "network" => assert!(matches!(error, GatewayError::Network(_))),
                "rate_limit" => assert!(matches!(error, GatewayError::RateLimited { .. })),
                "not_found" => assert!(matches!(error, GatewayError::NotFound(_))),
                "auth" => assert!(matches!(error, GatewayError::AuthFailed(_))),
                _ => unreachable!(),
            }
        }
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

    #[tokio::test]
    async fn test_provider_harness_comment_gateway_tracks_success_boundary() {
        let gateway = MockCommentGateway::new();
        gateway.add_comment(
            "v123",
            Comment::new("mock", "c1", "v123")
                .with_author("user1")
                .with_text("Harness comment"),
        );

        let comments = gateway.fetch_all_comments("v123", 5).await.unwrap();

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "Harness comment");
        assert!(matches!(
            gateway.get_calls().as_slice(),
            [GatewayCall::FetchAllComments {
                content_id,
                max: 5
            }] if content_id == "v123"
        ));
    }

    // ===== M1-T3 测试载荷(m1-pagination-core.md §3 M1-T3;断言 = 计划原文契约) =====

    use crate::ports::content_gateway::FetchShortfall;

    /// 生成 `n` 条 content_id 互异的测试 Content(沿仓内 `Content::new` 样板)。
    fn page(prefix: &str, start: usize, n: usize) -> Vec<Content> {
        (start..start + n)
            .map(|i| Content::new("mock", format!("{prefix}{i}")))
            .collect()
    }

    fn search_kw() -> KeywordType {
        KeywordType::Search("kw".to_string())
    }

    /// M1-T3 测试 2:注入 2 页(20+10)、count=50 → 30 条 + Exhausted。
    #[tokio::test]
    async fn mock_gateway_paged_injection_exhausted() {
        let gateway = MockContentGateway::new();
        gateway.add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);

        let outcome = gateway
            .fetch_by_keyword_with_outcome(&search_kw(), &SearchOptions::new("kw").with_count(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 30);
        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    }

    /// M1-T3 测试 3:注入 3 页(20+20+20)、count=50 → 50 条、shortfall=None。
    #[tokio::test]
    async fn mock_gateway_paged_injection_reaches_count() {
        let gateway = MockContentGateway::new();
        gateway.add_search_pages(
            "kw",
            vec![page("a", 0, 20), page("a", 20, 20), page("a", 40, 20)],
        );

        let outcome = gateway
            .fetch_by_keyword_with_outcome(&search_kw(), &SearchOptions::new("kw").with_count(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 50);
        assert!(outcome.shortfall.is_none());
    }

    /// M1-T3 测试 4:第 2 页 RateLimit + 第 1 页已有 20 条 → PartialFailure 含 "rate"
    /// (大小写不敏感);第 1 页即错(零进展)→ 整体 Err(GatewayError::RateLimited)(F-001)。
    #[tokio::test]
    async fn mock_gateway_page_failure_with_progress() {
        // 有进展分支
        let gateway = MockContentGateway::new();
        gateway.add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        gateway.set_page_error_at(1, MockError::RateLimit);

        let outcome = gateway
            .fetch_by_keyword_with_outcome(&search_kw(), &SearchOptions::new("kw").with_count(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 20);
        match outcome.shortfall {
            Some(FetchShortfall::PartialFailure { ref message }) => {
                assert!(
                    message.to_lowercase().contains("rate"),
                    "PartialFailure message must contain \"rate\" (case-insensitive), got {message:?}"
                );
            }
            ref other => panic!("expected Some(PartialFailure), got {other:?}"),
        }

        // 零进展分支(第 1 页即错)
        let gateway = MockContentGateway::new();
        gateway.add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        gateway.set_page_error_at(0, MockError::RateLimit);

        let result = gateway
            .fetch_by_keyword_with_outcome(&search_kw(), &SearchOptions::new("kw").with_count(50))
            .await;

        assert!(
            matches!(result, Err(GatewayError::RateLimited { .. })),
            "zero-progress failure must be overall Err(GatewayError::RateLimited), got {result:?}"
        );
    }

    /// M1-T3 测试 6(DR-10 触发集冻结):第 2 页注入非 429(network 类)错误、
    /// 第 1 页已有 20 条 → 整体 Err;不得收敛为 PartialFailure
    /// (触发集 = 仅 `GatewayError::RateLimited`,扩大须经 root)。
    #[tokio::test]
    async fn non_rate_limited_error_with_progress_is_err() {
        let gateway = MockContentGateway::new();
        gateway.add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        gateway.set_page_error_at(1, MockError::Network);

        let result = gateway
            .fetch_by_keyword_with_outcome(&search_kw(), &SearchOptions::new("kw").with_count(50))
            .await;

        assert!(
            matches!(result, Err(_)),
            "non-rate-limited error with progress must be overall Err (DR-10), got {result:?}"
        );
    }

    /// M1-T3 测试 8(legacy 桥接冻结):注入 2 页(20+10)→ legacy `fetch_by_keyword`
    /// 返回各页 flatten 后截断到 `options.count` 的结果:count=50 → 30 条;count=25 → 25 条。
    #[tokio::test]
    async fn legacy_fetch_sees_flattened_pages() {
        let gateway = MockContentGateway::new();
        gateway.add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);

        let full = gateway
            .fetch_by_keyword(&search_kw(), &SearchOptions::new("kw").with_count(50))
            .await
            .unwrap();
        assert_eq!(full.len(), 30);

        let truncated = gateway
            .fetch_by_keyword(&search_kw(), &SearchOptions::new("kw").with_count(25))
            .await
            .unwrap();
        assert_eq!(truncated.len(), 25);
    }
}
