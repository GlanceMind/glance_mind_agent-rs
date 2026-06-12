//! Twitter Adapter - Implements ContentGateway and CommentGateway for Twitter
//!
//! This adapter wraps the TikHubClient to implement the port interfaces for Twitter.
//! It provides automatic retry and proper error mapping.

use async_trait::async_trait;

use tracing::{debug, info, warn};

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::pagination::{PageDecision, PaginationLoop, StopReason};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    content_gateway::{FetchOutcome, FetchShortfall},
    CommentGateway, ContentGateway,
};
use crate::strategies::twitter::{extra_keys, search_type};
use crate::tikhub::{
    extract_tweet_from_detail_response, TikHubClient, TikHubError, TwitterCommentParams,
    TwitterSearchParams, TwitterTweet, TwitterUserTweetsParams,
};

/// Twitter adapter implementing ContentGateway and CommentGateway
pub struct TwitterAdapter {
    client: TikHubClient,
}

/// content 翻页循环终止信息(M4-T3,适配器私有;镜像 facebook.rs `FbFetchEnd`):
/// - `Stop(reason)`:循环以 M1 D2 的 `StopReason` 语义终止(达量 / 枯竭族);
/// - `Partial(err)`:`RateLimited` 且已有进展(过滤后交付集非空)的部分失败分支
///   (零交付时由 `fetch_by_keyword_with_outcome` 原样 `Err`,DR-01;DR-10:仅 RateLimited)。
#[derive(Debug)]
enum TwitterFetchEnd {
    Stop(StopReason),
    Partial(GatewayError),
}

impl TwitterAdapter {
    /// Create a new Twitter adapter with the given client
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
                GatewayError::AuthFailed(format!("Twitter auth failed: {}", message))
            }
            TikHubError::PaymentRequired { message } => {
                GatewayError::AuthFailed(format!("Twitter payment required: {}", message))
            }
            TikHubError::Forbidden { message } => {
                GatewayError::AuthFailed(format!("Twitter access forbidden: {}", message))
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

    /// Convert Twitter tweet to domain Content
    fn convert_content(tweet: &TwitterTweet) -> Content {
        let tweet_id = tweet.get_tweet_id().unwrap_or("").to_string();
        let screen_name = tweet.author_handle().unwrap_or("");
        let url = if screen_name.is_empty() {
            format!("https://twitter.com/i/web/status/{}", tweet_id)
        } else {
            format!("https://twitter.com/{}/status/{}", screen_name, tweet_id)
        };

        Content {
            platform: "twitter".to_string(),
            content_id: tweet_id.clone(),
            author: screen_name.to_string(),
            author_name: tweet.author_name().map(|s| s.to_string()),
            description: tweet.content().to_string(),
            url: Some(url),
            engagement: Engagement {
                likes: tweet.like_count(),
                comments: tweet.reply_count(),
                shares: tweet.retweet_count(),
                views: tweet.view_count(),
            },
            created_at: tweet.created_at_timestamp(),
            raw_data: serde_json::to_value(tweet).ok(),
        }
    }

    /// Convert Twitter tweet (reply) to domain Comment
    fn convert_comment(tweet: &TwitterTweet, content_id: &str) -> Comment {
        let tweet_id = tweet.get_tweet_id().unwrap_or("").to_string();

        Comment {
            platform: "twitter".to_string(),
            comment_id: tweet_id,
            content_id: content_id.to_string(),
            parent_id: tweet.in_reply_to_status_id_str.clone(),
            author: tweet.author_handle().unwrap_or("").to_string(),
            author_name: tweet.author_name().map(|s| s.to_string()),
            author_uid: tweet.user_id().map(|s| s.to_string()),
            text: tweet.content().to_string(),
            likes: tweet.like_count(),
            reply_count: tweet.reply_count() as i32,
            created_at: tweet.created_at_timestamp(),
            language: tweet.lang.clone(),
            is_reply: tweet.is_reply(),
            raw_data: serde_json::to_value(tweet).ok(),
        }
    }

    fn normalize_search_type(search_type: &str) -> &'static str {
        match search_type.trim().to_ascii_lowercase().as_str() {
            "top" => search_type::TOP,
            "media" => search_type::MEDIA,
            "people" => search_type::PEOPLE,
            "lists" => search_type::LISTS,
            _ => search_type::LATEST,
        }
    }

    fn search_type_from_options(options: &SearchOptions) -> String {
        if let Some(search_type) = options
            .extra
            .get(extra_keys::SEARCH_TYPE)
            .and_then(|value| value.as_str())
        {
            return Self::normalize_search_type(search_type).to_string();
        }

        if let Some(region_value) = options.region.as_deref() {
            let normalized = Self::normalize_search_type(region_value);
            if normalized != search_type::LATEST
                || region_value.eq_ignore_ascii_case(search_type::LATEST)
            {
                return normalized.to_string();
            }
        }

        search_type::LATEST.to_string()
    }

    fn search_params(query: &str, options: &SearchOptions) -> TwitterSearchParams {
        TwitterSearchParams::new(query).with_search_type(Self::search_type_from_options(options))
    }

    fn is_search_result_tweet(tweet: &TwitterTweet) -> bool {
        tweet.get_tweet_id().is_some()
            && tweet
                .tweet_type
                .as_deref()
                .map(|tweet_type| tweet_type.eq_ignore_ascii_case("tweet"))
                .unwrap_or(true)
    }

    fn dedupe_contents(contents: Vec<Content>, limit: usize) -> Vec<Content> {
        let mut seen = std::collections::HashSet::new();
        contents
            .into_iter()
            .filter(|content| seen.insert(content.content_id.clone()))
            .take(limit)
            .collect()
    }

    /// M4-T3:content 搜索 cursor 翻页(M1 `PaginationLoop` + D1 override 落点)。
    ///
    /// 镜像评论侧既有 cursor 模式(`with_cursor`/`data.next_cursor`),每页:
    /// 1. `TwitterSearchParams.with_cursor(cursor?)` 调用搜索;
    /// 2. 提取 timeline tweets,**过滤 `is_search_result_tweet`**(喂入状态机的 ids =
    ///    过滤后条目;达量计数以可用条目为准,噪声不充数 — 反作弊);
    /// 3. `accept_page(&filtered_ids, next)`,`next = data.next_cursor`(缺失→None;
    ///    重复值由 `PaginationLoop` 的 CursorLoop 兜底,F-004 twitter 已知重复)。
    ///
    /// 错误路径(DR-10,冻结):仅 `RateLimited` 且已有进展 → `Partial`(下游降为
    /// `PartialFailure`);硬错误 / 零进展 → 原样 `Err`(由调用方处理)。
    async fn search_content_paginated(
        &self,
        query: &str,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, TwitterFetchEnd)> {
        let mut loop_state = PaginationLoop::new(options.count as usize);
        let mut accepted: Vec<Content> = Vec::new();
        let mut cursor: Option<String> = None;

        let end = loop {
            let mut params = Self::search_params(query, options);
            if let Some(ref c) = cursor {
                params = params.with_cursor(c.clone());
            }

            match self.client.search_twitter_tweets_with_retry(&params).await {
                Ok(response) => {
                    let next = response.data.as_ref().and_then(|d| d.next_cursor.clone());

                    // 过滤计数:仅 `is_search_result_tweet` 的条目喂入状态机(噪声不充数)。
                    let filtered: Vec<Content> = response
                        .data
                        .as_ref()
                        .and_then(|d| d.timeline.as_ref())
                        .map(|list| {
                            list.iter()
                                .filter(|tweet| Self::is_search_result_tweet(tweet))
                                .map(Self::convert_content)
                                .collect()
                        })
                        .unwrap_or_default();

                    let ids: Vec<String> = filtered
                        .iter()
                        .map(|content| content.content_id.clone())
                        .collect();
                    let by_id: std::collections::HashMap<String, Content> = filtered
                        .into_iter()
                        .map(|content| (content.content_id.clone(), content))
                        .collect();

                    let outcome = loop_state.accept_page(&ids, next);
                    let mut by_id = by_id;
                    for id in &outcome.newly_accepted {
                        if let Some(content) = by_id.remove(id) {
                            accepted.push(content);
                        }
                    }

                    match outcome.decision {
                        PageDecision::Stop(reason) => break TwitterFetchEnd::Stop(reason),
                        PageDecision::Continue { cursor: next_cursor } => {
                            cursor = Some(next_cursor);
                        }
                    }
                }
                // DR-10:仅 RateLimited + 已有进展 → Partial;其余(硬错误)/零进展 → Err。
                Err(err @ TikHubError::RateLimited { .. }) if !accepted.is_empty() => {
                    warn!(
                        platform = "twitter",
                        query = %query,
                        collected = accepted.len(),
                        "Twitter content search hit rate limit after partial progress"
                    );
                    break TwitterFetchEnd::Partial(Self::convert_error(err));
                }
                Err(err) => return Err(Self::convert_error(err)),
            }
        };

        Ok((accepted, end))
    }
}

#[async_trait]
impl ContentGateway for TwitterAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let params = Self::search_params(&options.query, options);

        let response = self
            .client
            .search_twitter_tweets_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let tweets: Vec<Content> = response
            .data
            .as_ref()
            .and_then(|d| d.timeline.as_ref())
            .map(|list| {
                list.iter()
                    .filter(|tweet| Self::is_search_result_tweet(tweet))
                    .take(options.count as usize)
                    .map(Self::convert_content)
                    .collect()
            })
            .unwrap_or_default();

        Ok(tweets)
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                let params = Self::search_params(query, options);
                let response = self
                    .client
                    .search_twitter_tweets_with_retry(&params)
                    .await
                    .map_err(Self::convert_error)?;

                let tweets: Vec<Content> = response
                    .data
                    .as_ref()
                    .and_then(|d| d.timeline.as_ref())
                    .map(|list| {
                        list.iter()
                            .filter(|tweet| Self::is_search_result_tweet(tweet))
                            .take(options.count as usize)
                            .map(Self::convert_content)
                            .collect()
                    })
                    .unwrap_or_default();

                Ok(tweets)
            }
            KeywordType::UserId(username) => self.fetch_user_content(username, options.count).await,
            KeywordType::SecUserId(rest_id) => {
                // Fetch by rest_id (Twitter user ID)
                let params = TwitterUserTweetsParams::by_rest_id(rest_id);
                let response = self
                    .client
                    .fetch_twitter_user_tweets_with_retry(&params)
                    .await
                    .map_err(Self::convert_error)?;

                let tweets: Vec<Content> = response
                    .data
                    .as_ref()
                    .and_then(|d| d.timeline.as_ref())
                    .map(|list| {
                        list.iter()
                            .take(options.count as usize)
                            .map(Self::convert_content)
                            .collect()
                    })
                    .unwrap_or_default();

                Ok(tweets)
            }
            KeywordType::ContentId(tweet_id) => self
                .fetch_by_id(tweet_id)
                .await
                .map(|content| content.into_iter().collect()),
        }
    }

    /// M4-T3:override D1 默认方法,content 路径走 `PaginationLoop` 翻页 + shortfall。
    ///
    /// 仅 content 搜索(`Search`/`Hashtag`)走新循环;其余 keyword 类型仍委派
    /// `fetch_by_keyword`(滚动兼容,`shortfall = None`)。公开 `search()`/`fetch_by_keyword`
    /// 行为不变(content 走新循环但丢弃 shortfall)。
    ///
    /// shortfall 映射(语义与 `PaginationLoop::shortfall_for` 一致):
    /// - 达量 / 已交付 ≥ count → `None`;
    /// - 枯竭族(`UpstreamExhausted`/`CursorLoop`/`EmptyPageLimit`)且欠量 → `Exhausted`;
    /// - `Partial`(仅 RateLimited + 进展)→ `PartialFailure`(零进展已在循环内归一为 Err)。
    async fn fetch_by_keyword_with_outcome(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<FetchOutcome> {
        debug!(
            platform = "twitter",
            keyword = ?keyword,
            query = %options.query,
            "Twitter fetch_by_keyword_with_outcome"
        );

        let query = match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => query.clone(),
            // 非 content 搜索路径:滚动兼容默认包装(shortfall = None)。
            _ => {
                return Ok(FetchOutcome {
                    contents: self.fetch_by_keyword(keyword, options).await?,
                    shortfall: None,
                });
            }
        };

        let (contents, end) = self.search_content_paginated(&query, options).await?;
        let delivered = contents.len();
        let target = options.count as usize;

        let shortfall = match end {
            TwitterFetchEnd::Stop(_) if delivered >= target => None,
            TwitterFetchEnd::Stop(StopReason::ReachedMaxCount) => None,
            TwitterFetchEnd::Stop(
                StopReason::UpstreamExhausted
                | StopReason::CursorLoop
                | StopReason::EmptyPageLimit,
            ) => Some(FetchShortfall::Exhausted),
            TwitterFetchEnd::Partial(err) => {
                if contents.is_empty() {
                    // DR-01:零可交付进展 → 原样 Err(F-001 语义)。
                    return Err(err);
                }
                Some(FetchShortfall::PartialFailure {
                    message: err.to_string(),
                })
            }
        };

        // F-02 (GREEN):循环终止结构化日志(platform / accepted_count / 终止态)。
        info!(
            platform = "twitter",
            accepted_count = delivered,
            shortfall = ?shortfall,
            "Twitter content pagination loop terminated"
        );

        Ok(FetchOutcome {
            contents,
            shortfall,
        })
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        let params = TwitterUserTweetsParams::by_screen_name(user_id);

        let response = self
            .client
            .fetch_twitter_user_tweets_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let tweets: Vec<Content> = response
            .data
            .as_ref()
            .and_then(|d| d.timeline.as_ref())
            .map(|list| {
                list.iter()
                    .take(count as usize)
                    .map(Self::convert_content)
                    .collect()
            })
            .unwrap_or_default();

        // Also include pinned tweet if present
        let mut result =
            if let Some(pinned) = response.data.as_ref().and_then(|d| d.pinned.as_ref()) {
                vec![Self::convert_content(pinned)]
            } else {
                vec![]
            };
        result.extend(tweets);

        Ok(Self::dedupe_contents(result, count as usize))
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        let response = self
            .client
            .fetch_twitter_tweet_detail_with_retry(content_id)
            .await
            .map_err(Self::convert_error)?;

        Ok(
            extract_tweet_from_detail_response(&response)
                .map(|tweet| Self::convert_content(&tweet)),
        )
    }

    fn platform(&self) -> &str {
        "twitter"
    }
}

#[async_trait]
impl CommentGateway for TwitterAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        let mut params = TwitterCommentParams::new(content_id);

        if let Some(ref cursor) = options.cursor {
            params = params.with_cursor(cursor);
        }

        let response = self
            .client
            .fetch_twitter_comments_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let comments: Vec<Comment> = response
            .data
            .as_ref()
            .and_then(|d| d.thread.as_ref())
            .map(|list| {
                list.iter()
                    .filter(|tweet| tweet.get_tweet_id() != Some(content_id))
                    .take(options.count as usize)
                    .map(|t| Self::convert_comment(t, content_id))
                    .collect()
            })
            .unwrap_or_default();

        let next_cursor = response.data.as_ref().and_then(|d| d.next_cursor.clone());
        let has_more = next_cursor.is_some();

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
        let mut seen_comment_ids = std::collections::HashSet::new();

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

            for comment in result.comments {
                if comment.comment_id == content_id {
                    continue;
                }

                if seen_comment_ids.insert(comment.comment_id.clone()) {
                    all_comments.push(comment);
                }

                if all_comments.len() >= max_count as usize {
                    break;
                }
            }

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
        // Twitter replies are in the same thread, no separate endpoint needed
        Ok(vec![])
    }

    fn platform(&self) -> &str {
        "twitter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::content_gateway::FetchShortfall;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct MockHttpResponse {
        status: u16,
        body: serde_json::Value,
    }

    impl MockHttpResponse {
        fn json(status: u16, body: serde_json::Value) -> Self {
            Self { status, body }
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
                        404 => "Not Found",
                        _ => "Mock Response",
                    };
                    let body = serde_json::to_string(&response.body).unwrap();
                    let raw = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.status,
                        reason,
                        body.len(),
                        body
                    );

                    socket.write_all(raw.as_bytes()).await.unwrap();
                    let _ = socket.shutdown().await;
                });
            }
        });

        (format!("http://{}", addr), requests)
    }

    fn test_tweet(tweet_id: &str, screen_name: &str, text: &str) -> serde_json::Value {
        json!({
            "tweet_id": tweet_id,
            "type": "tweet",
            "text": text,
            "created_at": "Fri Jan 09 22:17:51 +0000 2026",
            "conversation_id": tweet_id,
            "favorites": 11,
            "retweets": 3,
            "replies": 2,
            "quotes": 1,
            "bookmarks": 4,
            "views": "20",
            "user_info": {
                "rest_id": format!("user-{tweet_id}"),
                "screen_name": screen_name,
                "name": format!("User {screen_name}"),
                "followers_count": 55
            }
        })
    }

    // ================================================================
    // M4-T3 content cursor-pagination helpers (mock-HTTP)
    //
    // DR-20: mock 形状取自 `src/tikhub/twitter_types.rs` serde 定义
    // (`TwitterTimelineData { timeline, next_cursor, .. }`)+ 评论侧既有样板
    // (fetch_all_comments mock 的 `data.next_cursor` 字段)。
    // **待 M4-T4 回灌确认**:仓内无 twitter 搜索响应真实样本(Step 06 实证),
    // M4-T4 回灌后逐字段对账。
    // ================================================================

    /// 搜索 timeline 一页:`data.timeline = tweets`,`data.next_cursor = cursor`。
    fn search_page(tweets: Vec<serde_json::Value>, next_cursor: Option<&str>) -> serde_json::Value {
        json!({
            "code": 200,
            "message": "success",
            "data": {
                "timeline": tweets,
                "next_cursor": next_cursor,
            }
        })
    }

    /// 429 限流响应,带 `Retry-After: 0`(DR-19:防止 RateLimited 默认 60s 实睡)。
    fn rate_limited_page() -> MockHttpResponse {
        MockHttpResponse {
            status: 429,
            body: json!({"message": "Too Many Requests"}),
        }
    }

    /// 构造 twitter 适配器,retry_config `max_delay_ms = 0`(DR-19)。
    /// RateLimited 默认 base delay = 60_000ms × backoff,`min(max_delay_ms=0)` ⇒ 实睡 0,
    /// 撞不上 mutants 300s 预算;同一页 429 = max_retries(3)+1 = 4 次 HTTP 请求。
    fn fast_retry_adapter(base_url: String) -> TwitterAdapter {
        let retry_config = crate::tikhub::TikHubRetryConfig {
            max_retries: 3,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 2.0,
        };
        let client =
            crate::tikhub::TikHubClient::with_retry_config("test-key", base_url, retry_config)
                .unwrap();
        TwitterAdapter::new(client)
    }

    /// content 搜索路径选项:keyword search + count。
    fn search_options(query: &str, count: u32) -> SearchOptions {
        SearchOptions::new(query)
            .with_platform("twitter")
            .with_count(count)
    }

    #[test]
    fn test_convert_content() {
        let tweet = TwitterTweet {
            tweet_id: Some("123456".to_string()),
            id: None,
            rest_id: None,
            tweet_type: Some("tweet".to_string()),
            text: Some("Test tweet content".to_string()),
            screen_name: Some("testuser".to_string()),
            created_at: Some("Fri Jan 09 22:17:51 +0000 2026".to_string()),
            conversation_id: None,
            lang: Some("en".to_string()),
            bookmarks: Some(5),
            favorites: Some(100),
            likes: None,
            quotes: Some(10),
            replies: Some(25),
            retweets: Some(50),
            views: Some(1000),
            user_info: Some(crate::tikhub::TwitterUser {
                rest_id: Some("u123".to_string()),
                name: Some("Test User".to_string()),
                screen_name: Some("testuser".to_string()),
                description: None,
                followers_count: Some(500),
                avatar: None,
                verified: Some(false),
                blue_verified: Some(false),
            }),
            author: None,
            media: None,
            entities: None,
            in_reply_to_status_id_str: None,
            in_reply_to_user_id_str: None,
        };

        let content = TwitterAdapter::convert_content(&tweet);
        assert_eq!(content.platform, "twitter");
        assert_eq!(content.content_id, "123456");
        assert_eq!(content.author, "testuser");
        assert_eq!(content.engagement.likes, 100);
        assert_eq!(content.engagement.views, 1000);
        assert_eq!(content.engagement.shares, 50); // retweets
    }

    #[test]
    fn test_convert_comment() {
        let tweet = TwitterTweet {
            tweet_id: Some("reply123".to_string()),
            id: None,
            rest_id: None,
            tweet_type: Some("tweet".to_string()),
            text: Some("Great tweet!".to_string()),
            screen_name: Some("replier".to_string()),
            created_at: None,
            conversation_id: None,
            lang: Some("en".to_string()),
            bookmarks: None,
            favorites: Some(50),
            likes: None,
            quotes: None,
            replies: Some(5),
            retweets: None,
            views: None,
            user_info: None,
            author: None,
            media: None,
            entities: None,
            in_reply_to_status_id_str: Some("123456".to_string()),
            in_reply_to_user_id_str: Some("u123".to_string()),
        };

        let comment = TwitterAdapter::convert_comment(&tweet, "123456");
        assert_eq!(comment.platform, "twitter");
        assert_eq!(comment.comment_id, "reply123");
        assert_eq!(comment.content_id, "123456");
        assert_eq!(comment.text, "Great tweet!");
        assert_eq!(comment.author, "replier");
        assert_eq!(comment.likes, 50);
        assert!(comment.is_reply);
    }

    #[test]
    fn test_search_type_from_options_prefers_extra() {
        let options =
            SearchOptions::new("rust").with_extra_value(extra_keys::SEARCH_TYPE, json!("media"));
        assert_eq!(TwitterAdapter::search_type_from_options(&options), "Media");
    }

    #[tokio::test]
    async fn test_search_forwards_configured_search_type() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
                200,
                json!({
                    "code": 200,
                    "message": "success",
                    "data": {
                        "timeline": [test_tweet("tw-1", "rustacean", "hello world")]
                    }
                }),
            )])
            .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("rust")
            .with_platform("twitter")
            .with_count(1)
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("Top"));

        let contents = adapter.search(&options).await.unwrap();
        assert_eq!(contents.len(), 1);

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains(
            "GET /api/v1/twitter/web/fetch_search_timeline?keyword=rust&search_type=Top HTTP/1.1"
        ));
    }

    #[tokio::test]
    async fn test_fetch_by_id_uses_detail_endpoint() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
                200,
                json!({
                    "code": 200,
                    "message": "success",
                    "data": {
                        "tweet": test_tweet("1808168603721650364", "jack", "detail text")
                    }
                }),
            )])
            .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();

        let content = adapter
            .fetch_by_id("1808168603721650364")
            .await
            .unwrap()
            .expect("detail endpoint should return a tweet");

        assert_eq!(content.content_id, "1808168603721650364");
        assert_eq!(content.author, "jack");

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains(
            "GET /api/v1/twitter/web/fetch_tweet_detail?tweet_id=1808168603721650364 HTTP/1.1"
        ));
    }

    #[tokio::test]
    async fn test_fetch_by_keyword_content_id_returns_single_tweet() {
        let base_url = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": test_tweet("1808168603721650364", "jack", "detail text")
            }),
        )])
        .await
        .0;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let keyword = KeywordType::ContentId("1808168603721650364".to_string());
        let options = SearchOptions::new("1808168603721650364")
            .with_platform("twitter")
            .with_count(1);

        let results = adapter.fetch_by_keyword(&keyword, &options).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content_id, "1808168603721650364");
    }

    #[tokio::test]
    async fn test_fetch_all_comments_skips_root_tweet_and_deduplicates() {
        let base_url = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "code": 200,
                    "message": "success",
                    "data": {
                        "thread": [
                            test_tweet("tweet-1", "author", "root"),
                            json!({
                                "tweet_id": "reply-1",
                                "type": "tweet",
                                "text": "reply one",
                                "created_at": "Fri Jan 09 22:17:51 +0000 2026",
                                "in_reply_to_status_id_str": "tweet-1",
                                "author": {
                                    "rest_id": "u1",
                                    "screen_name": "replier1",
                                    "name": "Replier 1"
                                }
                            })
                        ],
                        "next_cursor": "cursor-2"
                    }
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "code": 200,
                    "message": "success",
                    "data": {
                        "thread": [
                            json!({
                                "tweet_id": "reply-1",
                                "type": "tweet",
                                "text": "reply one",
                                "created_at": "Fri Jan 09 22:17:51 +0000 2026",
                                "in_reply_to_status_id_str": "tweet-1",
                                "author": {
                                    "rest_id": "u1",
                                    "screen_name": "replier1",
                                    "name": "Replier 1"
                                }
                            }),
                            json!({
                                "tweet_id": "reply-2",
                                "type": "tweet",
                                "text": "reply two",
                                "created_at": "Fri Jan 09 22:18:51 +0000 2026",
                                "in_reply_to_status_id_str": "tweet-1",
                                "author": {
                                    "rest_id": "u2",
                                    "screen_name": "replier2",
                                    "name": "Replier 2"
                                }
                            })
                        ],
                        "next_cursor": null
                    }
                }),
            ),
        ])
        .await
        .0;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();

        let comments = adapter.fetch_all_comments("tweet-1", 5).await.unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].comment_id, "reply-1");
        assert_eq!(comments[1].comment_id, "reply-2");
    }

    // ================================================================
    // Failure-path and edge-case tests
    // ================================================================

    #[tokio::test]
    async fn test_search_401_returns_auth_failed() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            401,
            json!({"message": "Unauthorized"}),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("bad-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("test")
            .with_platform("twitter")
            .with_count(1);

        let err = adapter.search(&options).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::AuthFailed(_)),
            "401 should map to AuthFailed, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_search_402_returns_auth_failed() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            402,
            json!({"message": "Payment Required"}),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("test")
            .with_platform("twitter")
            .with_count(1);

        let err = adapter.search(&options).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::AuthFailed(_)),
            "402 should map to AuthFailed, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_search_403_returns_auth_failed() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            403,
            json!({"message": "Forbidden"}),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("test")
            .with_platform("twitter")
            .with_count(1);

        let err = adapter.search(&options).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::AuthFailed(_)),
            "403 should map to AuthFailed, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_search_429_returns_rate_limited() {
        let responses = (0..4)
            .map(|_| MockHttpResponse::json(429, json!({"message": "Too Many Requests"})))
            .collect();
        let (base_url, _) = spawn_mock_http_server_with_capture(responses).await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("test")
            .with_platform("twitter")
            .with_count(1);

        let err = adapter.search(&options).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::RateLimited { .. }),
            "429 should map to RateLimited, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_search_500_returns_api_error() {
        let responses = (0..4)
            .map(|_| MockHttpResponse::json(500, json!({"message": "Internal Server Error"})))
            .collect();
        let (base_url, _) = spawn_mock_http_server_with_capture(responses).await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("test")
            .with_platform("twitter")
            .with_count(1);

        let err = adapter.search(&options).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::Api { .. }),
            "500 should map to Api error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_search_empty_timeline_returns_empty_vec() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": {
                    "timeline": [],
                    "next_cursor": null
                }
            }),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("nothing")
            .with_platform("twitter")
            .with_count(5);

        let contents = adapter.search(&options).await.unwrap();
        assert!(
            contents.is_empty(),
            "empty timeline should produce empty results"
        );
    }

    #[tokio::test]
    async fn test_search_null_data_returns_empty_vec() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": null
            }),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();
        let options = SearchOptions::new("nothing")
            .with_platform("twitter")
            .with_count(5);

        let contents = adapter.search(&options).await.unwrap();
        assert!(
            contents.is_empty(),
            "null data should produce empty results"
        );
    }

    #[tokio::test]
    async fn test_fetch_by_id_empty_detail_returns_none() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": {}
            }),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();

        let result = adapter.fetch_by_id("nonexistent").await.unwrap();
        assert!(result.is_none(), "empty detail response should return None");
    }

    #[tokio::test]
    async fn test_fetch_by_id_404_returns_not_found() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            404,
            json!({"message": "Not Found"}),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();

        let err = adapter.fetch_by_id("nonexistent").await.unwrap_err();
        assert!(
            matches!(err, GatewayError::NotFound(_)),
            "404 should map to NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_fetch_comments_empty_thread_returns_empty() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": {
                    "thread": [],
                    "next_cursor": null
                }
            }),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();

        let result = adapter.fetch_all_comments("tweet-1", 10).await.unwrap();
        assert!(
            result.is_empty(),
            "empty thread should produce empty comments"
        );
    }

    #[tokio::test]
    async fn test_fetch_comments_null_thread_returns_empty() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            json!({
                "code": 200,
                "message": "success",
                "data": {
                    "thread": null,
                    "next_cursor": null
                }
            }),
        )])
        .await;
        let adapter = TwitterAdapter::with_api_key("test-key", Some(base_url)).unwrap();

        let result = adapter.fetch_all_comments("tweet-1", 10).await.unwrap();
        assert!(
            result.is_empty(),
            "null thread should produce empty comments"
        );
    }

    #[test]
    fn test_normalize_search_type_known_values() {
        assert_eq!(TwitterAdapter::normalize_search_type("top"), "Top");
        assert_eq!(TwitterAdapter::normalize_search_type("Top"), "Top");
        assert_eq!(TwitterAdapter::normalize_search_type("TOP"), "Top");
        assert_eq!(TwitterAdapter::normalize_search_type("latest"), "Latest");
        assert_eq!(TwitterAdapter::normalize_search_type("Latest"), "Latest");
        assert_eq!(TwitterAdapter::normalize_search_type("media"), "Media");
        assert_eq!(TwitterAdapter::normalize_search_type("people"), "People");
        assert_eq!(TwitterAdapter::normalize_search_type("lists"), "Lists");
    }

    #[test]
    fn test_normalize_search_type_unknown_defaults_to_latest() {
        assert_eq!(TwitterAdapter::normalize_search_type("unknown"), "Latest");
        assert_eq!(TwitterAdapter::normalize_search_type(""), "Latest");
        assert_eq!(TwitterAdapter::normalize_search_type("  "), "Latest");
    }

    #[test]
    fn test_is_search_result_tweet_filters_non_tweet_type() {
        let tweet = TwitterTweet {
            tweet_id: Some("123".to_string()),
            tweet_type: Some("tombstone".to_string()),
            ..Default::default()
        };
        assert!(
            !TwitterAdapter::is_search_result_tweet(&tweet),
            "non-tweet type should be filtered"
        );
    }

    #[test]
    fn test_is_search_result_tweet_accepts_tweet_type() {
        let tweet = TwitterTweet {
            tweet_id: Some("123".to_string()),
            tweet_type: Some("tweet".to_string()),
            ..Default::default()
        };
        assert!(TwitterAdapter::is_search_result_tweet(&tweet));
    }

    #[test]
    fn test_is_search_result_tweet_accepts_none_type() {
        let tweet = TwitterTweet {
            tweet_id: Some("123".to_string()),
            tweet_type: None,
            ..Default::default()
        };
        assert!(TwitterAdapter::is_search_result_tweet(&tweet));
    }

    #[test]
    fn test_is_search_result_tweet_rejects_no_id() {
        let tweet = TwitterTweet {
            tweet_id: None,
            id: None,
            tweet_type: Some("tweet".to_string()),
            ..Default::default()
        };
        assert!(!TwitterAdapter::is_search_result_tweet(&tweet));
    }

    // ================================================================
    // M4-T3: twitter content 路径 cursor 翻页测试(经 fetch_by_keyword_with_outcome)
    //
    // 现状(RED 依据):content 搜索路径 `search_params`(twitter.rs:161-163)从不设
    // cursor,`fetch_by_keyword_with_outcome` 未 override → 默认包装 `fetch_by_keyword`,
    // 单次调用 + take(count),shortfall 恒 None。迁移后镜像评论侧 cursor 模式
    // (`with_cursor`/`next_cursor`)接 PaginationLoop。
    //
    // 断言形态镜像 M4-T2(reddit after 翻页),差异点:cursor/next_cursor。
    // mock 形状 DR-20 待 M4-T4 回灌对账(见 search_page helper 注释)。
    // ================================================================

    /// 测试 1(T-013 主断言):cursor 跨页转发。
    /// 3 页(next_cursor = c2 / c3 / None),断言第 2/3 请求转发 `cursor=c2` / `cursor=c3`。
    /// 现状从不转发 cursor(search_params 不设 cursor)→ 单次调用,RED。
    #[tokio::test]
    async fn paginates_cursor_until_count() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                search_page(
                    vec![
                        test_tweet("tw-a1", "u1", "a1"),
                        test_tweet("tw-a2", "u2", "a2"),
                    ],
                    Some("c2"),
                ),
            ),
            MockHttpResponse::json(
                200,
                search_page(
                    vec![
                        test_tweet("tw-b1", "u3", "b1"),
                        test_tweet("tw-b2", "u4", "b2"),
                    ],
                    Some("c3"),
                ),
            ),
            MockHttpResponse::json(
                200,
                search_page(
                    vec![
                        test_tweet("tw-c1", "u5", "c1"),
                        test_tweet("tw-c2", "u6", "c2"),
                    ],
                    None,
                ),
            ),
        ])
        .await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 6);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 6, "all 3 pages collected to count");

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3, "should issue one request per page");
        assert!(
            requests[1].contains("cursor=c2"),
            "2nd request must forward cursor=c2, got: {}",
            requests[1]
        );
        assert!(
            requests[2].contains("cursor=c3"),
            "3rd request must forward cursor=c3, got: {}",
            requests[2]
        );
    }

    /// 测试 2(F-003):next_cursor 缺失 → Some(Exhausted)。
    /// 2 页后 next_cursor=None 且未达 count → 上游枯竭。
    #[tokio::test]
    async fn exhausted_when_next_cursor_missing() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                search_page(vec![test_tweet("tw-a1", "u1", "a1")], Some("c2")),
            ),
            MockHttpResponse::json(
                200,
                search_page(vec![test_tweet("tw-b1", "u2", "b1")], None),
            ),
        ])
        .await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 50);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "missing next_cursor before reaching count must surface Exhausted"
        );
        assert_eq!(outcome.contents.len(), 2, "both pages' items retained");
    }

    /// 测试 3(F-004):重复 next_cursor → CursorLoop → Some(Exhausted),不发额外请求。
    /// **twitter 已知会返回重复 cursor**;状态机 CursorLoop 兜底。
    #[tokio::test]
    async fn repeated_next_cursor_stops() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                search_page(vec![test_tweet("tw-a1", "u1", "a1")], Some("c1")),
            ),
            // 第 2 页回吐与第 1 页相同的 cursor(twitter 已知行为)。
            MockHttpResponse::json(
                200,
                search_page(vec![test_tweet("tw-b1", "u2", "b1")], Some("c1")),
            ),
        ])
        .await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 50);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "repeated cursor (CursorLoop) maps to Exhausted shortfall"
        );

        let requests = requests.lock().unwrap().clone();
        assert_eq!(
            requests.len(),
            2,
            "repeated cursor must stop without issuing a 3rd request"
        );
    }

    /// 测试 4(F-005):连续空页达上限 → Some(Exhausted)。
    /// 3 连空页(next_cursor 递进 e1/e2/e3)→ EmptyPageLimit → Exhausted。
    #[tokio::test]
    async fn empty_pages_stop_at_limit() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(200, search_page(vec![], Some("e1"))),
            MockHttpResponse::json(200, search_page(vec![], Some("e2"))),
            MockHttpResponse::json(200, search_page(vec![], Some("e3"))),
        ])
        .await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 50);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "consecutive empty pages must stop at limit with Exhausted"
        );
        assert!(outcome.contents.is_empty(), "no items from empty pages");
    }

    /// 测试 5(F-002 + DR-19):第 2 页 429 → 首页条目保留 + Some(PartialFailure{含 "rate"})。
    /// DR-19:429 带 Retry-After: 0 语义,fast_retry_adapter max_delay_ms=0;
    /// 同一页 429 = max_retries(3)+1 = 4 次 HTTP 请求。请求数期望按 (1 首页 + 4) 计。
    #[tokio::test]
    async fn partial_failure_with_progress() {
        let mut responses = vec![MockHttpResponse::json(
            200,
            search_page(vec![test_tweet("tw-a1", "u1", "a1")], Some("c2")),
        )];
        // 第 2 页:同一 cursor 4 次 429(1 + 3 retries),全部限流。
        for _ in 0..4 {
            responses.push(rate_limited_page());
        }
        let (base_url, requests) = spawn_mock_http_server_with_capture(responses).await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 50);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 1, "first page item retained");
        match outcome.shortfall {
            Some(FetchShortfall::PartialFailure { message }) => {
                assert!(
                    message.to_lowercase().contains("rate"),
                    "partial failure message should mention rate limit, got: {message}"
                );
            }
            other => panic!("expected PartialFailure with progress, got {other:?}"),
        }

        let requests = requests.lock().unwrap().clone();
        assert_eq!(
            requests.len(),
            5,
            "1 first-page + 4 (429 retried) requests (DR-19)"
        );
    }

    /// 测试 6(F-001 + DR-19):第 1 页即 429 → Err(零进展)。**允许先绿**(AG-006)。
    /// DR-19:同测试 5,第 1 页 4 次 429 = 4 次 HTTP 请求。
    #[tokio::test]
    async fn zero_progress_error_is_err() {
        let responses = (0..4)
            .map(|_| rate_limited_page())
            .collect();
        let (base_url, requests) = spawn_mock_http_server_with_capture(responses).await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 50);

        let result = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await;

        assert!(
            result.is_err(),
            "zero-progress rate-limit on first page must be Err (F-001), got {result:?}"
        );

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 4, "first page 429 retried 4x total (DR-19)");
    }

    /// 测试 7:达量 → shortfall None。
    /// 第 1 页即满足 count(next_cursor 仍存在但已达量)→ ReachedMaxCount → None。
    #[tokio::test]
    async fn shortfall_none_when_reached() {
        let (base_url, _) = spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
            200,
            search_page(
                vec![
                    test_tweet("tw-a1", "u1", "a1"),
                    test_tweet("tw-a2", "u2", "a2"),
                ],
                Some("c2"),
            ),
        )])
        .await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 2);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 2, "exactly count items");
        assert_eq!(
            outcome.shortfall, None,
            "reaching count must yield shortfall None"
        );
    }

    /// 测试 8:既有 `search()` 回归不变。**允许先绿**(AG-006)。
    /// 单次 mock 返回与 search() 结果一致(legacy 路径未迁移,单次调用语义保持)。
    #[tokio::test]
    async fn legacy_search_unchanged() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
                200,
                search_page(
                    vec![
                        test_tweet("tw-a1", "u1", "a1"),
                        test_tweet("tw-a2", "u2", "a2"),
                    ],
                    Some("c2"),
                ),
            )])
            .await;
        let adapter = fast_retry_adapter(base_url);
        let options = search_options("rust", 5);

        let contents = adapter.search(&options).await.unwrap();

        assert_eq!(contents.len(), 2, "legacy search returns the single page");
        assert_eq!(contents[0].content_id, "tw-a1");

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1, "legacy search() stays single-call");
    }

    /// 测试 9(过滤计数 / is_search_result_tweet):含非 tweet 噪声条目的页。
    /// 计数以**过滤后**条目为准:噪声条目(tweet_type != "tweet" / 无 id)不充数,
    /// 达量判定不被噪声充数。count=4,每页 2 tweet + 1 噪声;需翻页才达量。
    /// **反作弊**:过滤计数语义不得弱化为「按原始条目计数」。
    #[tokio::test]
    async fn filtered_items_drive_counting() {
        let noise = || {
            json!({
                "tweet_id": "noise-x",
                "type": "tombstone",
                "text": "promoted/non-tweet noise"
            })
        };
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                search_page(
                    vec![
                        test_tweet("tw-a1", "u1", "a1"),
                        noise(),
                        test_tweet("tw-a2", "u2", "a2"),
                    ],
                    Some("c2"),
                ),
            ),
            MockHttpResponse::json(
                200,
                search_page(
                    vec![
                        test_tweet("tw-b1", "u3", "b1"),
                        noise(),
                        test_tweet("tw-b2", "u4", "b2"),
                    ],
                    Some("c3"),
                ),
            ),
        ])
        .await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 4);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await
            .unwrap();

        // 过滤后每页 2 条 → 需要第 2 页才达 count=4;噪声不充数。
        assert_eq!(
            outcome.contents.len(),
            4,
            "count must be measured on filtered tweets, not raw items"
        );
        assert!(
            outcome
                .contents
                .iter()
                .all(|c| c.content_id != "noise-x"),
            "non-tweet noise must be filtered out"
        );
        assert_eq!(outcome.shortfall, None, "reached count on filtered items");

        let requests = requests.lock().unwrap().clone();
        assert_eq!(
            requests.len(),
            2,
            "noise must not inflate count: must page twice to reach 4 filtered items"
        );
    }

    /// 测试 10(DR-10):第 1 页有进展 + 第 2 页 HTTP 500(硬错误)→ 整体 Err。
    /// **PartialFailure 触发集仅 RateLimited(M1 D2 冻结)**;硬错误不得降级为 Partial。
    /// **预期 RED**:现状单次调用返回首页 Ok(shortfall=None)→ expected Err, got Ok(..)。
    #[tokio::test]
    async fn hard_error_with_progress_is_err() {
        let mut responses = vec![MockHttpResponse::json(
            200,
            search_page(vec![test_tweet("tw-a1", "u1", "a1")], Some("c2")),
        )];
        // 第 2 页:HTTP 500 是可重试错误,4 次后耗尽 → 硬错误。
        for _ in 0..4 {
            responses.push(MockHttpResponse::json(
                500,
                json!({"message": "Internal Server Error"}),
            ));
        }
        let (base_url, _) = spawn_mock_http_server_with_capture(responses).await;
        let adapter = fast_retry_adapter(base_url);
        let keyword = KeywordType::Search("rust".to_string());
        let options = search_options("rust", 50);

        let result = adapter
            .fetch_by_keyword_with_outcome(&keyword, &options)
            .await;

        assert!(
            result.is_err(),
            "hard error (HTTP 500) after progress must be Err, not downgraded to Partial (DR-10), got {result:?}"
        );
    }
}
