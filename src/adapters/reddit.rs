//! Reddit Adapter - Implements ContentGateway and CommentGateway for Reddit
//!
//! This adapter wraps the TikHubClient to implement the port interfaces for Reddit.
//! It provides automatic retry and proper error mapping.

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::pagination::{PageDecision, PaginationLoop, StopReason};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    content_gateway::{FetchOutcome, FetchShortfall},
    CommentGateway, ContentGateway,
};
use crate::tikhub::{
    extract_comments_from_trees, extract_posts_from_search, RedditComment, RedditCommentParams,
    RedditPost, RedditSearchParams, RedditUserPostsParams, TikHubClient, TikHubError,
};

/// 翻页循环终止信息(M4-T2,适配器私有,镜像 facebook `FbFetchEnd`):
/// - `Stop(reason)`:循环以 M1 D2 的 `StopReason` 语义终止;
/// - `Partial(err)`:`RateLimited` 且原始进展非空的先例分支,保留原错误
///   (零交付时由 `fetch_by_keyword_with_outcome` 原样 `Err`,DR-01)。
#[derive(Debug)]
enum RedditFetchEnd {
    Stop(StopReason),
    Partial(GatewayError),
}

/// Reddit adapter implementing ContentGateway and CommentGateway
pub struct RedditAdapter {
    client: TikHubClient,
}

impl RedditAdapter {
    /// Create a new Reddit adapter with the given client
    pub fn new(client: TikHubClient) -> Self {
        Self { client }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, TikHubError> {
        let client = TikHubClient::from_env()?;
        Ok(Self { client })
    }

    /// Create with API key and optional base URL
    pub fn with_api_key(
        api_key: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<Self, TikHubError> {
        let base = base_url.unwrap_or_else(|| "https://api.tikhub.io".to_string());
        let client = TikHubClient::new(api_key, base)?;
        Ok(Self { client })
    }

    /// Convert TikHubError to GatewayError
    fn convert_error(err: TikHubError) -> GatewayError {
        match err {
            TikHubError::Unauthorized { message } => {
                GatewayError::AuthFailed(format!("Reddit auth failed: {}", message))
            }
            TikHubError::PaymentRequired { message } => {
                GatewayError::AuthFailed(format!("Reddit payment required: {}", message))
            }
            TikHubError::Forbidden { message } => {
                GatewayError::AuthFailed(format!("Reddit access forbidden: {}", message))
            }
            TikHubError::MissingApiKey => {
                GatewayError::AuthFailed("TikHub API key not configured".into())
            }
            TikHubError::RateLimited { retry_after_secs } => {
                GatewayError::RateLimited { retry_after_secs }
            }
            TikHubError::ServerError { status, message } => GatewayError::Api {
                code: status as i32,
                message: format!("Server error: {}", message),
            },
            TikHubError::NetworkError { message } => GatewayError::Network(message),
            TikHubError::BadRequest { message } => {
                GatewayError::InvalidParams(format!("Bad request: {}", message))
            }
            TikHubError::NotFound { message } => GatewayError::NotFound(message),
            TikHubError::EmptyData => GatewayError::EmptyResponse,
            TikHubError::ParseError(msg) => GatewayError::ParseError(msg),
            TikHubError::InvalidParam(msg) => GatewayError::InvalidParams(msg),
        }
    }

    /// Convert Reddit post to domain Content
    fn convert_content(post: &RedditPost) -> Content {
        let post_id = post.post_id().unwrap_or_default();
        let _subreddit = post.subreddit_name().unwrap_or("");

        Content {
            platform: "reddit".to_string(),
            content_id: post_id.clone(),
            author: post.author_name().unwrap_or("").to_string(),
            author_name: post.author_name().map(|s| s.to_string()),
            description: format!("{}\n\n{}", post.title_str(), post.content_str()),
            url: post
                .permalink
                .as_ref()
                .map(|p| format!("https://www.reddit.com{}", p))
                .or(post.url.clone()),
            engagement: Engagement {
                likes: post.votes(), // Reddit uses upvotes/score
                comments: post.comments(),
                shares: 0,
                views: 0, // Reddit doesn't expose view count
            },
            created_at: post.created_at_timestamp(),
            raw_data: serde_json::to_value(post).ok(),
        }
    }

    /// Convert Reddit comment to domain Comment
    fn convert_comment(comment: &RedditComment, content_id: &str) -> Comment {
        Comment {
            platform: "reddit".to_string(),
            comment_id: comment.comment_id().to_string(),
            content_id: content_id.to_string(),
            parent_id: comment.parent_id.clone(),
            author: comment.author.clone(),
            author_name: Some(comment.author.clone()),
            author_uid: None,
            text: comment.body.clone(),
            likes: comment.score,
            reply_count: 0, // Reddit comment structure is tree-based
            created_at: Some(comment.created_utc),
            language: None,
            is_reply: comment.is_reply,
            raw_data: serde_json::to_value(comment).ok(),
        }
    }

    /// Convert Reddit batch post detail to domain Content
    fn convert_batch_post(post: &crate::tikhub::RedditBatchPostDetail) -> Content {
        let post_id = post.id.clone().unwrap_or_default();

        Content {
            platform: "reddit".to_string(),
            content_id: post_id.clone(),
            author: post.author.clone().unwrap_or_default(),
            author_name: post.author.clone(),
            description: format!(
                "{}\n\n{}",
                post.title.as_deref().unwrap_or(""),
                post.selftext.as_deref().unwrap_or("")
            ),
            url: post
                .permalink
                .as_ref()
                .map(|p| format!("https://www.reddit.com{}", p))
                .or_else(|| post.url.clone()),
            engagement: Engagement {
                likes: post.score.unwrap_or(0),
                comments: post.num_comments.unwrap_or(0),
                shares: 0,
                views: 0,
            },
            created_at: post.created_utc.map(|t| t as i64),
            raw_data: serde_json::to_value(post).ok(),
        }
    }

    /// content 搜索翻页(M4-T2;M1 `PaginationLoop` + after-cursor 翻页,镜像评论侧
    /// `with_after`/`has_next_page`/`end_cursor` 既有 cursor 模式)。
    ///
    /// 每页:`RedditSearchParams.with_after(cursor?)` → 提取 posts(去前缀 content_id)
    /// → `accept_page(&ids, next)`;`next` = `has_next_page==true` 时 `Some(end_cursor)`
    /// 否则 `None`。`Continue { cursor }` 转发 after;`Stop(reason)` 终止。
    ///
    /// DR-10:仅 `RateLimited` 且已有进展 → `Partial`;硬错误/零进展 → 原样 `Err`。
    async fn search_content_paginated(
        &self,
        query: &str,
        count: u32,
    ) -> GatewayResult<(Vec<Content>, RedditFetchEnd)> {
        let mut loop_state = PaginationLoop::new(count as usize);
        let mut contents: Vec<Content> = Vec::new();
        let mut cursor: Option<String> = None;

        let end = loop {
            let mut params = RedditSearchParams::new(query);
            if let Some(ref after) = cursor {
                params = params.with_after(after);
            }

            let response = match self.client.search_reddit_posts_with_retry(&params).await {
                Ok(response) => response,
                Err(err) => {
                    let mapped = Self::convert_error(err);
                    // DR-10:仅 RateLimited 且已有进展 → Partial;其它(硬错误)/零进展 → Err。
                    if matches!(mapped, GatewayError::RateLimited { .. }) && !contents.is_empty() {
                        break RedditFetchEnd::Partial(mapped);
                    }
                    return Err(mapped);
                }
            };

            let main = response
                .data
                .as_ref()
                .and_then(|d| d.search.as_ref())
                .and_then(|s| s.dynamic.as_ref())
                .and_then(|d| d.components.as_ref())
                .and_then(|c| c.main.as_ref());

            let page_posts = response
                .data
                .as_ref()
                .map(extract_posts_from_search)
                .unwrap_or_default();

            let page_info = main.and_then(|m| m.page_info.as_ref());
            let has_next = page_info.and_then(|p| p.has_next_page).unwrap_or(false);
            let next_cursor = if has_next {
                page_info.and_then(|p| p.end_cursor.clone())
            } else {
                None
            };

            // posts 与其 content_id 一一对应(按接受顺序裁剪到 newly_accepted)。
            let page_contents: Vec<Content> = page_posts.iter().map(|p| Self::convert_content(p)).collect();
            let page_ids: Vec<String> = page_contents.iter().map(|c| c.content_id.clone()).collect();

            let outcome = loop_state.accept_page(&page_ids, next_cursor);

            // 仅追加本页「新接受」的 content(去重 + 页内截断由 PaginationLoop 保证)。
            for accepted_id in &outcome.newly_accepted {
                if let Some(content) = page_contents.iter().find(|c| &c.content_id == accepted_id) {
                    contents.push(content.clone());
                }
            }

            match outcome.decision {
                PageDecision::Continue { cursor: next } => {
                    cursor = Some(next);
                }
                PageDecision::Stop(reason) => break RedditFetchEnd::Stop(reason),
            }
        };

        // GREEN 规格补 F-02:循环终止时记结构化日志(platform、accepted_count、终止原因)。
        match &end {
            RedditFetchEnd::Stop(reason) => {
                tracing::info!(
                    platform = "reddit",
                    accepted_count = contents.len(),
                    stop_reason = ?reason,
                    "reddit content pagination loop terminated"
                );
            }
            RedditFetchEnd::Partial(err) => {
                tracing::warn!(
                    platform = "reddit",
                    accepted_count = contents.len(),
                    error = %err,
                    "reddit content pagination loop ended with partial failure"
                );
            }
        }

        Ok((contents, end))
    }
}

#[async_trait]
impl ContentGateway for RedditAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        // content 路径走新 PaginationLoop 循环;`search()` 丢弃 shortfall(签名行为不变)。
        let (contents, _end) = self
            .search_content_paginated(&options.query, options.count)
            .await?;
        Ok(contents)
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                // content 路径走新 PaginationLoop 循环;此签名丢弃 shortfall。
                let (contents, _end) =
                    self.search_content_paginated(query, options.count).await?;
                Ok(contents)
            }
            KeywordType::UserId(username) => self.fetch_user_content(username, options.count).await,
            KeywordType::ContentId(post_id) | KeywordType::SecUserId(post_id) => {
                // Reddit post IDs start with t3_
                tracing::warn!(
                    "Reddit direct post fetch not fully supported for ID: {}",
                    post_id
                );
                Ok(vec![])
            }
        }
    }

    /// M4-T2:override D1 默认方法。content 搜索路径走带 shortfall 的新翻页循环;
    /// 其它路径(UserId / ContentId 等)退回默认语义(`fetch_by_keyword` 包装,shortfall=None)。
    ///
    /// 映射(语义与 `PaginationLoop::shortfall_for` 一致;镜像 facebook 适配器 override):
    /// - `Stop(ReachedMaxCount)` 或 已达量 → `None`;
    /// - `Stop(UpstreamExhausted | CursorLoop | EmptyPageLimit)` → `Some(Exhausted)`;
    /// - `Partial(err)`(仅 RateLimited 且有进展)→ `Some(PartialFailure { .. })`;
    ///   零交付时原样 `Err`(DR-01 / DR-10)。
    async fn fetch_by_keyword_with_outcome(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<FetchOutcome> {
        let query = match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => query,
            // 非 content 搜索路径:退回默认语义(滚动兼容,shortfall=None)。
            _ => {
                return Ok(FetchOutcome {
                    contents: self.fetch_by_keyword(keyword, options).await?,
                    shortfall: None,
                });
            }
        };

        let (contents, end) = self.search_content_paginated(query, options.count).await?;
        let delivered = contents.len();
        let target = options.count as usize;

        let shortfall = match end {
            RedditFetchEnd::Stop(_) if delivered >= target => None,
            RedditFetchEnd::Stop(StopReason::ReachedMaxCount) => None,
            RedditFetchEnd::Stop(
                StopReason::UpstreamExhausted
                | StopReason::CursorLoop
                | StopReason::EmptyPageLimit,
            ) => Some(FetchShortfall::Exhausted),
            RedditFetchEnd::Partial(err) => {
                if contents.is_empty() {
                    // DR-01/DR-10:零可交付进展 → 原样 Err。
                    return Err(err);
                }
                Some(FetchShortfall::PartialFailure {
                    message: err.to_string(),
                })
            }
        };

        Ok(FetchOutcome {
            contents,
            shortfall,
        })
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        let params = RedditUserPostsParams::new(user_id);

        let response = self
            .client
            .fetch_reddit_user_posts_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let posts: Vec<Content> = response
            .data
            .as_ref()
            .and_then(|d| d.post_feed.as_ref())
            .and_then(|f| f.elements.as_ref())
            .and_then(|e| e.edges.as_ref())
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|e| e.node.as_ref())
                    .take(count as usize)
                    .map(Self::convert_content)
                    .collect()
            })
            .unwrap_or_default();

        Ok(posts)
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        // Use batch API to fetch single post
        let post_id = if content_id.starts_with("t3_") {
            content_id.to_string()
        } else {
            format!("t3_{}", content_id)
        };

        let response = self
            .client
            .fetch_reddit_post_details_batch_with_retry(&[post_id])
            .await
            .map_err(Self::convert_error)?;

        let post = response
            .data
            .as_ref()
            .and_then(|d| d.posts.as_ref())
            .and_then(|posts| posts.first());

        match post {
            Some(p) => Ok(Some(Self::convert_batch_post(p))),
            None => Ok(None),
        }
    }

    fn platform(&self) -> &str {
        "reddit"
    }
}

#[async_trait]
impl CommentGateway for RedditAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        // Ensure post_id has t3_ prefix
        let post_id = if content_id.starts_with("t3_") {
            content_id.to_string()
        } else {
            format!("t3_{}", content_id)
        };

        let mut params = RedditCommentParams::new(&post_id).with_limit(options.count);

        if let Some(ref cursor) = options.cursor {
            params = params.with_after(cursor);
        }

        let response = self
            .client
            .fetch_reddit_comments_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let comments: Vec<Comment> = response
            .data
            .as_ref()
            .and_then(|d| d.post_info_by_id.as_ref())
            .and_then(|p| p.comment_forest.as_ref())
            .and_then(|f| f.trees.as_ref())
            .map(|trees| {
                extract_comments_from_trees(trees, None)
                    .iter()
                    .map(|c| Self::convert_comment(c, content_id))
                    .collect()
            })
            .unwrap_or_default();

        let page_info = response
            .data
            .as_ref()
            .and_then(|d| d.post_info_by_id.as_ref())
            .and_then(|p| p.comment_forest.as_ref())
            .and_then(|f| f.page_info.as_ref());

        let has_more = page_info.and_then(|p| p.has_next_page).unwrap_or(false);
        let next_cursor = page_info.and_then(|p| p.end_cursor.clone());

        Ok(FetchCommentsResult {
            comments,
            has_more,
            next_cursor,
            total: None,
        })
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        let mut all_comments = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let remaining = max_count.saturating_sub(all_comments.len() as u32);
            if remaining == 0 {
                break;
            }

            let options = FetchCommentsOptions {
                cursor: cursor.clone(),
                count: remaining.min(50),
                sort: crate::ports::comment_gateway::CommentSort::default(),
                include_replies: false,
            };

            let result = self.fetch_comments(content_id, &options).await?;

            if result.comments.is_empty() {
                break;
            }

            all_comments.extend(result.comments);

            if !result.has_more {
                break;
            }

            cursor = result.next_cursor;

            // Small delay to avoid rate limiting
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        Ok(all_comments)
    }

    async fn fetch_replies(
        &self,
        _content_id: &str,
        _comment_id: &str,
        _options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        // Reddit comments are already in a tree structure, replies are included
        Ok(vec![])
    }

    fn platform(&self) -> &str {
        "reddit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // ================================================================
    // M4-T2 reddit content after-pagination mock-HTTP helpers (FR-003)
    //
    // mock-HTTP helper 按 FR-003 模块内复制(沿 facebook/twitter 适配器既有
    // spawn_mock_http_server_with_capture 样板,零新依赖)。
    //
    // DR-20: mock 响应形状取自 `src/tikhub/reddit_types.rs` serde 定义
    //   (`RedditSearchResponse = RedditResponse<RedditSearchData>`;
    //    `data.search.dynamic.components.main.{edges,pageInfo}`,
    //    edges[].node.children[].post = RedditPost;
    //    pageInfo = { hasNextPage, endCursor })
    //   + 评论侧既有请求样板(RedditCommentParams.with_after / RedditPageInfo)。
    // **待 M4-T4 回灌确认**:仓内无 reddit 搜索响应真实样本(Step 06 实证);
    //   M4-T4 回灌后逐字段对账(DR-20)。
    // ================================================================

    struct MockHttpResponse {
        status: u16,
        body: Value,
        extra_headers: Vec<(String, String)>,
    }

    impl MockHttpResponse {
        fn json(status: u16, body: Value) -> Self {
            Self {
                status,
                body,
                extra_headers: Vec::new(),
            }
        }

        fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.extra_headers.push((key.into(), value.into()));
            self
        }
    }

    async fn spawn_mock_http_server_with_capture(
        responses: Vec<MockHttpResponse>,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let expected_requests = responses.lock().unwrap().len();
        let captured_requests = requests.clone();

        tokio::spawn(async move {
            for _ in 0..expected_requests {
                let (mut socket, _) = listener.accept().await.unwrap();
                let responses = responses.clone();
                let requests = captured_requests.clone();

                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 4096];
                    let size = socket.read(&mut buffer).await.unwrap();
                    let request_text = String::from_utf8_lossy(&buffer[..size]).to_string();
                    if let Some(request_line) = request_text.lines().next() {
                        requests.lock().unwrap().push(request_line.to_string());
                    }

                    let response = responses.lock().unwrap().pop_front().unwrap_or_else(|| {
                        MockHttpResponse::json(500, json!({"message": "missing mock response"}))
                    });

                    let reason = match response.status {
                        200 => "OK",
                        429 => "Too Many Requests",
                        500 => "Internal Server Error",
                        503 => "Service Unavailable",
                        404 => "Not Found",
                        _ => "Mock Response",
                    };
                    let body = serde_json::to_string(&response.body).unwrap();
                    let mut raw = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                        response.status,
                        reason,
                        body.len()
                    );
                    for (key, value) in response.extra_headers {
                        raw.push_str(&format!("{key}: {value}\r\n"));
                    }
                    raw.push_str("\r\n");
                    raw.push_str(&body);

                    socket.write_all(raw.as_bytes()).await.unwrap();
                    let _ = socket.shutdown().await;
                });
            }
        });

        (format!("http://{}", addr), requests)
    }

    async fn spawn_mock_http_server(responses: Vec<MockHttpResponse>) -> String {
        let (base_url, _) = spawn_mock_http_server_with_capture(responses).await;
        base_url
    }

    /// 单个 reddit 搜索 post(DR-20:取自 `RedditPost` serde 定义;`id` 带 t3_ 前缀,
    /// `post_id()` 去前缀后即 content_id)。
    fn search_post(post_id: &str, title: &str) -> Value {
        // 注:`RedditPost.title` 带 `#[serde(alias = "postTitle")]`,二者只能取其一
        // (同时给会触发 serde duplicate field)——DR-20 形状取 serde 定义,用 `title`。
        json!({
            "id": format!("t3_{post_id}"),
            "__typename": "SubredditPost",
            "title": title,
            "author": { "name": format!("author-{post_id}") },
            "subreddit": { "name": "rust" },
            "createdAt": "2026-01-09T22:17:51+0000",
            "voteCount": 42,
            "commentCount": 7,
            "permalink": format!("/r/rust/comments/{post_id}/")
        })
    }

    fn search_posts(prefix: &str, n: usize) -> Vec<Value> {
        (0..n)
            .map(|i| search_post(&format!("{prefix}-{i}"), "fixture-title"))
            .collect()
    }

    /// 一页搜索响应(DR-20:`data.search.dynamic.components.main.{edges,pageInfo}`;
    /// posts 落在 `edges[0].node.children[].post`,与 `extract_posts_from_search` 遍历路径一致)。
    fn search_page(posts: Vec<Value>, has_next: bool, end_cursor: Option<&str>) -> MockHttpResponse {
        let children: Vec<Value> = posts
            .into_iter()
            .map(|post| json!({ "__typename": "SubredditPost", "post": post }))
            .collect();
        MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": {
                    "search": {
                        "dynamic": {
                            "components": {
                                "__typename": "SearchDynamicComponents",
                                "main": {
                                    "edges": [
                                        { "node": { "__typename": "SearchPosts", "children": children } }
                                    ],
                                    "pageInfo": {
                                        "hasNextPage": has_next,
                                        "endCursor": end_cursor
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        )
    }

    /// 排队 n 个 429(DR-19:带 `retry-after: 0`,且按 tikhub 重试语义排队;
    /// RateLimited 默认 60s × max_retries=3 在 max_delay_ms=0 下实睡 0,
    /// 同一页 429 = max_retries(3)+1 = 4 次 HTTP 请求)。
    fn rate_limited_responses(n: usize) -> Vec<MockHttpResponse> {
        (0..n)
            .map(|_| {
                MockHttpResponse::json(429, json!({"message": "rate limited"}))
                    .with_header("retry-after", "0")
            })
            .collect()
    }

    /// 构造 reddit 适配器,retry_config `max_delay_ms = 0`(DR-19)。
    /// RateLimited 默认 base delay = 60_000ms × backoff,`min(max_delay_ms=0)` ⇒ 实睡 0,
    /// 撞不上 mutants 300s 预算;同一页 429 = 4 次 HTTP 请求。
    /// no-proxy 由测试 harness 的 `NO_PROXY=127.0.0.1,localhost` 保证(127.0.0.1 mock 不走代理)。
    fn fast_retry_adapter(base_url: String) -> RedditAdapter {
        let retry_config = crate::tikhub::TikHubRetryConfig {
            max_retries: 3,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 2.0,
        };
        let client =
            crate::tikhub::TikHubClient::with_retry_config("test-key", base_url, retry_config)
                .unwrap();
        RedditAdapter::new(client)
    }

    fn search_keyword() -> KeywordType {
        KeywordType::Search("rust".to_string())
    }

    fn search_options(count: u32) -> SearchOptions {
        SearchOptions::new("rust")
            .with_platform("reddit")
            .with_count(count)
    }

    // ----------------------------------------------------------------
    // M4-T2 测试载荷(m4-reddit-twitter-p1.md §4 M4-T2,测试 1~9)
    // 全部经 `fetch_by_keyword_with_outcome` 调用;断言 = 计划原文契约。
    // ----------------------------------------------------------------
    use crate::ports::content_gateway::FetchShortfall;

    /// M4-T2 测试 1(T-012 主断言):3 页(各含若干 posts,hasNextPage=true/true/false,
    /// endCursor=c2/c3),count 跨页 → 达量;**捕获请求断言第 2/3 请求转发 after=c2/c3**。
    /// **预期 RED**(现状单次调用 + take):requests.len() == 1 ≠ 3,`after` 从未转发。
    #[tokio::test]
    async fn paginates_after_until_count() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            search_page(search_posts("p1", 20), true, Some("c2")),
            search_page(search_posts("p2", 20), true, Some("c3")),
            search_page(search_posts("p3", 10), false, None),
        ])
        .await;

        let adapter = fast_retry_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 50);
        assert!(outcome.shortfall.is_none());

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert!(
            requests[1].contains("after=c2"),
            "2nd request must forward after=c2, got {:?}",
            requests[1]
        );
        assert!(
            requests[2].contains("after=c3"),
            "3rd request must forward after=c3, got {:?}",
            requests[2]
        );
    }

    /// M4-T2 测试 2(F-003):2 页后 hasNextPage=false 且未达 count → Some(Exhausted)。
    /// **预期 RED**:现状 `left: None, right: Some(Exhausted)`。
    #[tokio::test]
    async fn exhausted_when_has_next_false() {
        let base_url = spawn_mock_http_server(vec![
            search_page(search_posts("p1", 20), true, Some("c2")),
            search_page(search_posts("p2", 10), false, None),
        ])
        .await;

        let adapter = fast_retry_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 30);
        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    }

    /// M4-T2 测试 3(F-004):endCursor 重复(hasNextPage=true 但 endCursor 与上页相同)
    /// → 终止、Some(Exhausted)、不发额外请求(requests.len() == 2)。
    #[tokio::test]
    async fn repeated_end_cursor_stops() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            search_page(search_posts("p1", 20), true, Some("c-loop")),
            search_page(search_posts("p2", 10), true, Some("c-loop")),
        ])
        .await;

        let adapter = fast_retry_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
        assert_eq!(
            requests.lock().unwrap().len(),
            2,
            "repeated end_cursor must stop; no 3rd request"
        );
    }

    /// M4-T2 测试 4(F-005):3 连空页(hasNextPage=true,endCursor 递进)→ Some(Exhausted)。
    #[tokio::test]
    async fn empty_pages_stop_at_limit() {
        let base_url = spawn_mock_http_server(vec![
            search_page(Vec::new(), true, Some("e1")),
            search_page(Vec::new(), true, Some("e2")),
            search_page(Vec::new(), true, Some("e3")),
        ])
        .await;

        let adapter = fast_retry_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    }

    /// M4-T2 测试 5(F-002 / DR-19):第 1 页 20 条 + 第 2 页 429(重试耗尽)
    /// → 首页条目保留、Some(PartialFailure{..}) 含 "rate";请求数 = 1 + 4 == 5。
    #[tokio::test]
    async fn partial_failure_with_progress() {
        let mut responses = vec![search_page(search_posts("p1", 20), true, Some("c2"))];
        responses.extend(rate_limited_responses(4));
        let (base_url, requests) = spawn_mock_http_server_with_capture(responses).await;

        let adapter = fast_retry_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 20);
        match &outcome.shortfall {
            Some(FetchShortfall::PartialFailure { message }) => {
                assert!(
                    message.to_lowercase().contains("rate"),
                    "PartialFailure message should mention rate limiting, got {message:?}"
                );
            }
            other => panic!("expected Some(PartialFailure {{ .. }}) shortfall, got {other:?}"),
        }
        assert_eq!(requests.lock().unwrap().len(), 5);
    }

    /// M4-T2 测试 6(F-001 语义边界,**允许先绿** AG-006 / DR-19):第 1 页即 429(零进展)
    /// → Err,不包装成 PartialFailure;请求数 = 4。
    #[tokio::test]
    async fn zero_progress_error_is_err() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(rate_limited_responses(4)).await;

        let adapter = fast_retry_adapter(base_url);
        let result = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await;

        assert!(
            matches!(result, Err(GatewayError::RateLimited { .. })),
            "zero-progress rate limit must be Err(RateLimited), got {result:?}"
        );
        assert_eq!(requests.lock().unwrap().len(), 4);
    }

    /// M4-T2 测试 7:达量 → shortfall None。
    #[tokio::test]
    async fn shortfall_none_when_reached() {
        let base_url = spawn_mock_http_server(vec![
            search_page(search_posts("p1", 20), true, Some("c2")),
            search_page(search_posts("p2", 20), true, Some("c3")),
            search_page(search_posts("p3", 20), true, Some("c4")),
        ])
        .await;

        let adapter = fast_retry_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 50);
        assert!(outcome.shortfall.is_none());
    }

    /// M4-T2 测试 8(回归,**允许先绿** AG-006):既有 `search()` 同 mock 返回与
    /// outcome.contents 一致(override 不破坏既有方法)。
    #[tokio::test]
    async fn legacy_search_unchanged() {
        let base_url = spawn_mock_http_server(vec![search_page(
            search_posts("p1", 5),
            false,
            None,
        )])
        .await;

        let adapter = fast_retry_adapter(base_url);
        let contents = adapter.search(&search_options(5)).await.unwrap();

        assert_eq!(contents.len(), 5);
        assert_eq!(contents[0].content_id, "p1-0");
        assert_eq!(contents[0].platform, "reddit");
    }

    /// M4-T2 测试 9(DR-10,同 M3 形状):第 1 页有进展 + 第 2 页 HTTP 500(硬错误)
    /// → 整体 Err(PartialFailure 触发集仅 RateLimited,M1 D2 冻结;硬错误不得降级为 Partial)。
    /// **预期 RED**:现状单次调用返回首页 Ok(shortfall=None)→ `expected Err, got Ok(..)`。
    #[tokio::test]
    async fn hard_error_with_progress_is_err() {
        let mut responses = vec![search_page(search_posts("p1", 20), true, Some("c2"))];
        // 第 2 页 500 = ServerError(retryable,max_retries=3)→ 4 次请求,实睡 0(max_delay_ms=0)。
        responses.extend(
            (0..4).map(|_| MockHttpResponse::json(500, json!({"message": "internal error"}))),
        );
        let base_url = spawn_mock_http_server(responses).await;

        let adapter = fast_retry_adapter(base_url);
        let result = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &search_options(50))
            .await;

        assert!(
            result.is_err(),
            "hard error (HTTP 500) with progress must be overall Err, got {result:?}"
        );
    }

    #[test]
    fn test_convert_comment() {
        let comment = RedditComment {
            id: "xyz789".to_string(),
            name: Some("t1_xyz789".to_string()),
            author: "commenter".to_string(),
            body: "Great post!".to_string(),
            body_html: None,
            created_utc: 1234567890,
            score: 50,
            parent_id: None,
            is_reply: false,
            depth: 0,
            subreddit: Some("rust".to_string()),
            permalink: None,
        };

        let domain_comment = RedditAdapter::convert_comment(&comment, "abc123");
        assert_eq!(domain_comment.platform, "reddit");
        assert_eq!(domain_comment.comment_id, "xyz789");
        assert_eq!(domain_comment.content_id, "abc123");
        assert_eq!(domain_comment.text, "Great post!");
        assert_eq!(domain_comment.author, "commenter");
        assert_eq!(domain_comment.likes, 50);
        assert!(!domain_comment.is_reply);
    }
}
