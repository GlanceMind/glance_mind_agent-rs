//! TikHub Adapter - Implements ContentGateway and CommentGateway for TikTok
//!
//! This adapter wraps the TikHubClient to implement the port interfaces.
//! It provides automatic retry and proper error mapping.

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::pagination::{platform_page_cap, PageDecision, PaginationLoop, StopReason};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    content_gateway::{FetchOutcome, FetchShortfall},
    CommentGateway, ContentGateway,
};
use crate::strategies::extra_keys;
use crate::tikhub::{
    AwemeInfo, CommentParams, SearchParams, TikHubClient, TikHubError, TikHubRetryConfig,
    UserVideoParams,
};

/// Delay between fetching consecutive pages, to avoid upstream rate limiting.
/// Zeroed under `cfg(test)` so property tests (hundreds of cases × multiple
/// pages) run instantly; production builds keep the real 500ms delay.
#[cfg(not(test))]
const PAGE_FETCH_DELAY: Duration = Duration::from_millis(500);
#[cfg(test)]
const PAGE_FETCH_DELAY: Duration = Duration::ZERO;

/// One page of video results from a paginated source.
struct VideoPage {
    videos: Vec<AwemeInfo>,
    next_cursor: Option<i64>,
    has_more: bool,
}

/// Abstraction over a single page fetch so the pagination loop can be reused
/// across the search and user-video paths (and tested independently).
#[async_trait]
trait VideoPageFetcher: Sync {
    async fn fetch(&self, cursor: i64, count: u32) -> Result<VideoPage, TikHubError>;
}

/// Accumulate up to `target` videos across pages, deduped by `aweme_id`,
/// truncated to `target`. Mirrors the comment pagination loop in
/// `TikHubClient::fetch_all_comments_safe`.
///
/// `page_size_cap` is the upstream per-page maximum (20 for TikHub search).
async fn paginate_videos(
    target: usize,
    page_size_cap: u32,
    fetcher: &dyn VideoPageFetcher,
) -> Result<Vec<AwemeInfo>, TikHubError> {
    let mut collected: Vec<AwemeInfo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: i64 = 0;
    // Safety guard: bound the number of pages so we never spin forever.
    let max_pages = target / page_size_cap.max(1) as usize + 2;
    let mut pages = 0usize;

    loop {
        let remaining = target.saturating_sub(collected.len());
        if remaining == 0 {
            break;
        }

        if pages >= max_pages {
            tracing::warn!(
                target = target,
                pages = pages,
                collected = collected.len(),
                "TikHub: video pagination hit max_pages guard, stopping"
            );
            break;
        }
        pages += 1;

        let count = (remaining as u32).min(page_size_cap);
        let page = fetcher.fetch(cursor, count).await?;

        // An empty page terminates pagination even if has_more claims otherwise.
        if page.videos.is_empty() {
            break;
        }

        for video in page.videos {
            if seen.insert(video.aweme_id.clone()) {
                collected.push(video);
            }
        }

        tracing::info!(
            page = pages,
            collected = collected.len(),
            target = target,
            has_more = page.has_more,
            next_cursor = ?page.next_cursor,
            "TikHub: video page fetched"
        );

        if !page.has_more {
            break;
        }

        match page.next_cursor {
            Some(next) => {
                cursor = next;
                // Small delay before fetching the NEXT page only (never after
                // the terminal page) to avoid rate limiting.
                tokio::time::sleep(PAGE_FETCH_DELAY).await;
            }
            // No cursor to advance with: stop rather than re-fetch the same page.
            None => break,
        }
    }

    collected.truncate(target);
    Ok(collected)
}

/// How the search-path pagination loop terminated (M3-T2; D-01/D2).
///
/// - `Stop(reason)`: the `PaginationLoop` state machine reached a frozen
///   `StopReason` (达量 / 枯竭族);
/// - `Partial(message)`: a `RateLimited` error surfaced *after* progress
///   (DR-10 触发集仅 RateLimited);零进展时改由调用方原样 `Err`(DR-01)。
enum SearchEnd {
    Stop(StopReason),
    Partial(String),
}

/// Search-path pagination loop driven by `PaginationLoop` (D-01 红线:终止
/// 逻辑全部由状态机承担,不在此手写 seen-set / 空页计数 / max_pages 守卫)。
///
/// `target` = 总量(`options.count`);`page_size` = 单页请求 count(来自
/// `extra["page_size"]`,缺省 `platform_page_cap("tiktok")`)。offset 从 0 起,
/// 每页以响应 `cursor` 为下一 offset(T-051 语义)。
///
/// next_cursor 归一化(DR-18 tiktok 适配):`has_more==Some(1)` 且 `cursor`
/// 存在时 `Some(cursor.to_string())`,否则 `None`(→ UpstreamExhausted)。
/// 错误(DR-10):`RateLimited` 且已有进展 → `Partial`;其余错误 / 零进展 → `Err`。
async fn search_paginated(
    target: usize,
    page_size: u32,
    fetcher: &SearchPageFetcher<'_>,
) -> Result<(Vec<AwemeInfo>, SearchEnd), TikHubError> {
    let mut pl = PaginationLoop::new(target);
    // aweme_id -> AwemeInfo, so newly-accepted ids map back to owned content.
    let mut by_id: std::collections::HashMap<String, AwemeInfo> = std::collections::HashMap::new();
    let mut collected: Vec<AwemeInfo> = Vec::new();
    let mut offset: i64 = 0;
    let mut first_page = true;

    let end = loop {
        if !first_page {
            tokio::time::sleep(PAGE_FETCH_DELAY).await;
        }
        first_page = false;

        let page = match fetcher.fetch(offset, page_size).await {
            Ok(page) => page,
            // DR-10:RateLimited 且已有进展 → Partial;否则原样 Err(零进展 / 硬错误)。
            Err(err @ TikHubError::RateLimited { .. }) if !collected.is_empty() => {
                break SearchEnd::Partial(err.to_string());
            }
            Err(err) => return Err(err),
        };

        let ids: Vec<String> = page.videos.iter().map(|v| v.aweme_id.clone()).collect();
        for video in page.videos {
            by_id.entry(video.aweme_id.clone()).or_insert(video);
        }

        // next_cursor 归一化:has_more=1 且 cursor 存在 → Some;否则 None(枯竭)。
        let next_cursor = if page.has_more {
            page.next_cursor.map(|c| c.to_string())
        } else {
            None
        };

        let outcome = pl.accept_page(&ids, next_cursor);
        for id in outcome.newly_accepted {
            if let Some(video) = by_id.remove(&id) {
                collected.push(video);
            }
        }

        match outcome.decision {
            PageDecision::Continue { cursor } => {
                // tiktok cursor 为 i64;parse 失败(理论不可达)归一化为枯竭。
                match cursor.parse::<i64>() {
                    Ok(next) => offset = next,
                    Err(_) => break SearchEnd::Stop(StopReason::UpstreamExhausted),
                }
            }
            PageDecision::Stop(reason) => break SearchEnd::Stop(reason),
        }
    };

    let stop_reason = match &end {
        SearchEnd::Stop(reason) => Some(reason.clone()),
        SearchEnd::Partial(_) => None,
    };
    tracing::info!(
        platform = "tiktok",
        accepted_count = collected.len(),
        stop = ?stop_reason,
        partial = matches!(end, SearchEnd::Partial(_)),
        "TikHub: search pagination loop terminated"
    );

    Ok((collected, end))
}

/// Per-page request count for the search path: `extra["page_size"]` if present,
/// else the tiktok platform cap (D4 载体;`SearchParams::with_count` 再 clamp 20)。
fn search_page_size(options: &SearchOptions) -> u32 {
    options
        .extra
        .get(extra_keys::PAGE_SIZE)
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or_else(|| platform_page_cap("tiktok"))
}

/// `VideoPageFetcher` backed by the TikHub search endpoint.
struct SearchPageFetcher<'a> {
    client: &'a TikHubClient,
    keyword: String,
    region: Option<String>,
    sort_type: Option<u8>,
    publish_time: Option<u8>,
}

#[async_trait]
impl VideoPageFetcher for SearchPageFetcher<'_> {
    async fn fetch(&self, cursor: i64, count: u32) -> Result<VideoPage, TikHubError> {
        let mut params = SearchParams::new(&self.keyword)
            .with_count(count)
            .with_offset(cursor.max(0) as u32);

        if let Some(ref region) = self.region {
            params = params.with_region(region);
        }
        if let Some(sort_type) = self.sort_type {
            params = params.with_sort_type(sort_type);
        }
        if let Some(publish_time) = self.publish_time {
            params = params.with_publish_time(publish_time);
        }

        let resp = self.client.search_videos_with_retry(&params).await?;

        // `extract_videos` borrows from `resp`; clone into owned values before
        // `resp` is dropped at the end of this scope.
        let videos: Vec<AwemeInfo> = TikHubClient::extract_videos(&resp)
            .into_iter()
            .cloned()
            .collect();

        let next_cursor = resp.data.as_ref().and_then(|d| d.cursor);
        let has_more = resp
            .data
            .as_ref()
            .and_then(|d| d.has_more)
            .map(|h| h == 1)
            .unwrap_or(false);

        Ok(VideoPage {
            videos,
            next_cursor,
            has_more,
        })
    }
}

/// Which kind of user identifier a `UserVideoPageFetcher` carries.
enum UserVideoId {
    UniqueId(String),
    SecUserId(String),
}

/// `VideoPageFetcher` backed by the TikHub user-post-videos endpoint.
///
/// Supports both `unique_id` and `sec_user_id` lookups; pagination advances by
/// `max_cursor` (NOT offset), so `fetch` assigns the loop cursor directly to
/// `UserVideoParams.max_cursor`.
struct UserVideoPageFetcher<'a> {
    client: &'a TikHubClient,
    id: UserVideoId,
}

impl<'a> UserVideoPageFetcher<'a> {
    fn unique_id(client: &'a TikHubClient, unique_id: impl Into<String>) -> Self {
        Self {
            client,
            id: UserVideoId::UniqueId(unique_id.into()),
        }
    }

    fn sec_user_id(client: &'a TikHubClient, sec_user_id: impl Into<String>) -> Self {
        Self {
            client,
            id: UserVideoId::SecUserId(sec_user_id.into()),
        }
    }
}

#[async_trait]
impl VideoPageFetcher for UserVideoPageFetcher<'_> {
    async fn fetch(&self, cursor: i64, count: u32) -> Result<VideoPage, TikHubError> {
        let mut params = match &self.id {
            UserVideoId::UniqueId(id) => UserVideoParams::by_unique_id(id),
            UserVideoId::SecUserId(id) => UserVideoParams::by_sec_user_id(id),
        }
        .with_count(count);
        // This path paginates by max_cursor; the cursor is already i64.
        params.max_cursor = cursor;

        let resp = self.client.fetch_user_videos_with_retry(&params).await?;

        // `extract_user_videos` borrows from `resp`; clone into owned values
        // before `resp` is dropped at the end of this scope.
        let videos: Vec<AwemeInfo> = TikHubClient::extract_user_videos(&resp)
            .into_iter()
            .cloned()
            .collect();

        let next_cursor = resp.data.as_ref().and_then(|d| d.max_cursor);
        let has_more = resp.data.as_ref().and_then(|d| d.has_more) == Some(1);

        Ok(VideoPage {
            videos,
            next_cursor,
            has_more,
        })
    }
}

/// TikHub adapter implementing ContentGateway and CommentGateway
pub struct TikHubAdapter {
    client: TikHubClient,
}

impl TikHubAdapter {
    /// Create a new TikHub adapter with the given client
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

    /// Create with custom retry configuration
    pub fn with_retry_config(
        api_key: impl Into<String>,
        base_url: Option<String>,
        retry_config: TikHubRetryConfig,
    ) -> Result<Self, TikHubError> {
        let base = base_url.unwrap_or_else(|| "https://api.tikhub.io".to_string());
        let client = TikHubClient::with_retry_config(api_key, base, retry_config)?;
        Ok(Self { client })
    }

    /// Convert TikHubError to GatewayError with proper categorization
    fn convert_error(err: TikHubError) -> GatewayError {
        match err {
            // Fatal errors - authentication/authorization/payment
            TikHubError::Unauthorized { message } => {
                GatewayError::AuthFailed(format!("TikHub auth failed: {}", message))
            }
            TikHubError::PaymentRequired { message } => {
                GatewayError::AuthFailed(format!("TikHub payment required: {}", message))
            }
            TikHubError::Forbidden { message } => {
                GatewayError::AuthFailed(format!("TikHub access forbidden: {}", message))
            }
            TikHubError::MissingApiKey => {
                GatewayError::AuthFailed("TikHub API key not configured".into())
            }

            // Retryable errors
            TikHubError::RateLimited { retry_after_secs } => {
                GatewayError::RateLimited { retry_after_secs }
            }
            TikHubError::ServerError { status, message } => GatewayError::Api {
                code: status as i32,
                message: format!("Server error: {}", message),
            },
            TikHubError::NetworkError { message } => GatewayError::Network(message),

            // Skippable errors
            TikHubError::BadRequest { message } => {
                GatewayError::InvalidParams(format!("Bad request: {}", message))
            }
            TikHubError::NotFound { message } => GatewayError::NotFound(message),
            TikHubError::EmptyData => GatewayError::EmptyResponse,

            // Other errors
            TikHubError::ParseError(msg) => GatewayError::ParseError(msg),
            TikHubError::InvalidParam(msg) => GatewayError::InvalidParams(msg),
        }
    }

    /// Check if the TikHub error is fatal (should stop the entire task)
    pub fn is_fatal_error(err: &TikHubError) -> bool {
        err.is_fatal()
    }

    /// Check if the TikHub error is skippable (can continue with next item)
    pub fn is_skippable_error(err: &TikHubError) -> bool {
        err.is_skippable()
    }

    /// Run the search-path `PaginationLoop` and map its termination to a
    /// `(contents, shortfall)` pair (M3-T2; D1 shortfall semantics mirror
    /// `PaginationLoop::shortfall_for`). Shared by the public `search` wrapper
    /// (discards shortfall) and the `fetch_by_keyword_with_outcome` override.
    ///
    /// DR-01:零进展的可恢复失败已在 `search_paginated` 原样 `Err`;到这里的
    /// `Partial` 必有非空 `collected`。
    async fn search_with_outcome(
        &self,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, Option<FetchShortfall>)> {
        let fetcher = SearchPageFetcher {
            client: &self.client,
            keyword: options.query.clone(),
            region: options.region.clone(),
            sort_type: options.sort_type,
            publish_time: options.publish_time,
        };

        let target = options.count as usize;
        let page_size = search_page_size(options);

        let (videos, end) = search_paginated(target, page_size, &fetcher)
            .await
            .map_err(Self::convert_error)?;

        let contents: Vec<Content> = videos.iter().map(Self::convert_content).collect();
        // shortfall 映射与 `PaginationLoop::shortfall_for` 一致:
        // 达量 → None;枯竭族 且 delivered<target → Exhausted;Partial → PartialFailure。
        let shortfall = match end {
            SearchEnd::Stop(StopReason::ReachedMaxCount) => None,
            SearchEnd::Stop(
                StopReason::UpstreamExhausted
                | StopReason::CursorLoop
                | StopReason::EmptyPageLimit,
            ) => {
                if contents.len() >= target {
                    None
                } else {
                    Some(FetchShortfall::Exhausted)
                }
            }
            SearchEnd::Partial(message) => Some(FetchShortfall::PartialFailure { message }),
        };

        Ok((contents, shortfall))
    }

    /// Convert TikHub AwemeInfo to domain Content
    fn convert_content(aweme: &crate::tikhub::AwemeInfo) -> Content {
        Content {
            platform: "tiktok".to_string(),
            content_id: aweme.aweme_id.clone(),
            author: aweme.author_username().unwrap_or("").to_string(),
            author_name: aweme.author_name().map(|s| s.to_string()),
            description: aweme.description().to_string(),
            url: aweme.share_url.clone(),
            engagement: Engagement {
                likes: aweme.likes(),
                comments: aweme.comments(),
                shares: aweme.shares(),
                views: aweme.views(),
            },
            created_at: aweme.create_time,
            raw_data: serde_json::to_value(aweme).ok(),
        }
    }

    /// Convert TikHub TikTokComment to domain Comment
    fn convert_comment(comment: &crate::tikhub::TikTokComment, content_id: &str) -> Comment {
        Comment {
            platform: "tiktok".to_string(),
            comment_id: comment.cid.clone(),
            content_id: content_id.to_string(),
            parent_id: comment.reply_id.clone().filter(|id| id != "0"),
            author: comment.username().unwrap_or("").to_string(),
            author_name: comment.nickname().map(|s| s.to_string()),
            author_uid: comment.user.as_ref().and_then(|u| u.uid.clone()),
            text: comment.content().to_string(),
            likes: comment.likes(),
            reply_count: comment.reply_comment_total.unwrap_or(0),
            created_at: comment.create_time,
            language: comment.comment_language.clone(),
            is_reply: comment.is_reply(),
            raw_data: serde_json::to_value(comment).ok(),
        }
    }
}

#[async_trait]
impl ContentGateway for TikHubAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        tracing::debug!(
            keyword = %options.query,
            region = ?options.region,
            sort_type = ?options.sort_type,
            publish_time = ?options.publish_time,
            count = options.count,
            "TikHub search params"
        );

        // TikHub's search endpoint returns at most 20 items per request, so we
        // paginate (offset/cursor loop) to reach the requested total. The search
        // path is driven by the shared `PaginationLoop` (M3-T2, D-01); the public
        // `search` wrapper discards the shortfall and returns just the contents.
        let (contents, _shortfall) = self.search_with_outcome(options).await?;
        Ok(contents)
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                // Use with_query to create modified options
                self.search(&options.with_query(query)).await
            }
            KeywordType::UserId(user_id) => self.fetch_user_content(user_id, options.count).await,
            KeywordType::SecUserId(sec_uid) => {
                // TikHub's user-videos endpoint returns at most 20 items per
                // request, so paginate (max_cursor loop) to reach the total.
                let fetcher = UserVideoPageFetcher::sec_user_id(&self.client, sec_uid);
                let videos = paginate_videos(options.count as usize, 20, &fetcher)
                    .await
                    .map_err(Self::convert_error)?;

                Ok(videos.iter().map(Self::convert_content).collect())
            }
            KeywordType::ContentId(content_id) => match self.fetch_by_id(content_id).await? {
                Some(content) => Ok(vec![content]),
                None => Ok(vec![]),
            },
        }
    }

    /// D1 override (M3-T2):search 路径(Search/Hashtag keyword)走带 shortfall
    /// 的 `PaginationLoop` 路径并上报欠交付原因;user-videos / content 路径仍走
    /// 默认包装(`fetch_by_keyword`,shortfall=None,滚动兼容)。
    async fn fetch_by_keyword_with_outcome(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<FetchOutcome> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                let (contents, shortfall) =
                    self.search_with_outcome(&options.with_query(query)).await?;
                Ok(FetchOutcome {
                    contents,
                    shortfall,
                })
            }
            // user-videos / content 路径不在 M3 范围:默认滚动兼容语义(shortfall=None)。
            _ => Ok(FetchOutcome {
                contents: self.fetch_by_keyword(keyword, options).await?,
                shortfall: None,
            }),
        }
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        // TikHub's user-videos endpoint returns at most 20 items per request,
        // so paginate (max_cursor loop) to reach the requested total.
        let fetcher = UserVideoPageFetcher::unique_id(&self.client, user_id);
        let videos = paginate_videos(count as usize, 20, &fetcher)
            .await
            .map_err(Self::convert_error)?;

        Ok(videos.iter().map(Self::convert_content).collect())
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        // TikHub doesn't have a direct video-by-ID endpoint for app API
        // We would need to use a different endpoint or return NotFound
        // For now, we'll return NotFound as a placeholder
        Err(GatewayError::NotFound(format!(
            "Direct video fetch not supported for ID: {}",
            content_id
        )))
    }

    fn platform(&self) -> &str {
        "tiktok"
    }
}

#[async_trait]
impl CommentGateway for TikHubAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        let cursor = options.cursor.as_deref().unwrap_or("0");

        let params = CommentParams::new()
            .with_cursor(cursor)
            .with_count(options.count);

        // Use retry-enabled fetch
        let response = self
            .client
            .fetch_comments_with_retry(content_id, &params)
            .await
            .map_err(Self::convert_error)?;

        let comments = TikHubClient::extract_comments(&response);
        let domain_comments: Vec<Comment> = comments
            .iter()
            .map(|c| Self::convert_comment(c, content_id))
            .collect();

        let data = response.data.as_ref();
        let has_more = data
            .and_then(|d| d.has_more)
            .map(|h| h == 1)
            .unwrap_or(false);
        let next_cursor = data.and_then(|d| d.cursor).map(|c| c.to_string());
        let total = data.and_then(|d| d.total);

        let result = FetchCommentsResult {
            comments: domain_comments,
            has_more,
            next_cursor,
            total,
        };

        Ok(result)
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        let comments = self
            .client
            .fetch_all_comments(content_id, max_count)
            .await
            .map_err(Self::convert_error)?;

        Ok(comments
            .iter()
            .map(|c| Self::convert_comment(c, content_id))
            .collect())
    }

    async fn fetch_replies(
        &self,
        _content_id: &str,
        _comment_id: &str,
        _options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        // TikHub doesn't have a direct reply fetch endpoint in the current implementation
        // Return empty for now
        Ok(vec![])
    }

    fn platform(&self) -> &str {
        "tiktok"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tikhub::AwemeInfo;

    #[test]
    fn test_convert_content() {
        let aweme = AwemeInfo {
            aweme_id: "123456".to_string(),
            desc: Some("Test video".to_string()),
            create_time: Some(1234567890),
            share_url: Some("https://tiktok.com/video/123".to_string()),
            author: Some(crate::tikhub::Author {
                uid: Some("u123".to_string()),
                unique_id: Some("testuser".to_string()),
                nickname: Some("Test User".to_string()),
                sec_uid: None,
                avatar_thumb: None,
                custom_verify: None,
                follower_count: None,
            }),
            statistics: Some(crate::tikhub::Statistics {
                digg_count: Some(100),
                comment_count: Some(10),
                share_count: Some(5),
                play_count: Some(1000),
                collect_count: None,
                download_count: None,
            }),
            video: None,
            music: None,
        };

        let content = TikHubAdapter::convert_content(&aweme);
        assert_eq!(content.content_id, "123456");
        assert_eq!(content.author, "testuser");
        assert_eq!(content.engagement.likes, 100);
        assert_eq!(content.engagement.views, 1000);
    }

    #[test]
    fn test_convert_comment() {
        let comment = crate::tikhub::TikTokComment {
            cid: "c123".to_string(),
            text: Some("Great video!".to_string()),
            create_time: Some(1234567890),
            digg_count: Some(50),
            reply_id: Some("0".to_string()),
            reply_comment_total: Some(5),
            aweme_id: Some("v123".to_string()),
            user: Some(crate::tikhub::CommentUser {
                uid: Some("u456".to_string()),
                unique_id: Some("commenter".to_string()),
                nickname: Some("Commenter Name".to_string()),
                avatar_thumb: None,
                sec_uid: None,
            }),
            is_author_digged: None,
            comment_language: Some("en".to_string()),
        };

        let domain_comment = TikHubAdapter::convert_comment(&comment, "v123");
        assert_eq!(domain_comment.comment_id, "c123");
        assert_eq!(domain_comment.text, "Great video!");
        assert_eq!(domain_comment.author, "commenter");
        assert_eq!(domain_comment.likes, 50);
        assert!(!domain_comment.is_reply);
    }

    /// Property-based tests for the core pagination accumulation logic
    /// (`paginate_videos`), driven by an in-memory fake fetcher (no HTTP).
    ///
    /// These pin the loop contract: dedup by `aweme_id`, never exceed `target`,
    /// bounded by supply, exact accumulation for well-formed scripts, and a
    /// bounded page-call count even when a script claims `has_more` forever.
    mod proptests {
        use super::*;
        use proptest::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        const CAP: u32 = 20;

        /// A `VideoPageFetcher` that replays a scripted sequence of pages,
        /// ignoring cursor/count. The pagination loop alone decides when to
        /// stop; the fake just supplies whatever the script says.
        struct FakeFetcher {
            script: Vec<VideoPage>,
            calls: AtomicUsize,
        }

        impl FakeFetcher {
            fn new(script: Vec<VideoPage>) -> Self {
                Self {
                    script,
                    calls: AtomicUsize::new(0),
                }
            }

            fn calls(&self) -> usize {
                self.calls.load(Ordering::SeqCst)
            }
        }

        #[async_trait]
        impl VideoPageFetcher for FakeFetcher {
            async fn fetch(&self, _cursor: i64, _count: u32) -> Result<VideoPage, TikHubError> {
                let idx = self.calls.fetch_add(1, Ordering::SeqCst);
                match self.script.get(idx) {
                    Some(page) => Ok(VideoPage {
                        videos: page.videos.clone(),
                        next_cursor: page.next_cursor,
                        has_more: page.has_more,
                    }),
                    // Defensive: script exhausted -> empty terminal page.
                    None => Ok(VideoPage {
                        videos: Vec::new(),
                        next_cursor: None,
                        has_more: false,
                    }),
                }
            }
        }

        /// Minimal `AwemeInfo` carrying only the id the loop dedups on.
        fn aweme(id: i64) -> AwemeInfo {
            AwemeInfo {
                aweme_id: id.to_string(),
                desc: None,
                create_time: None,
                share_url: None,
                author: None,
                statistics: None,
                video: None,
                music: None,
            }
        }

        /// Block on the async helper from a sync proptest body.
        fn run(target: usize, fake: &FakeFetcher) -> Vec<AwemeInfo> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            rt.block_on(paginate_videos(target, CAP, fake))
                .expect("fake fetcher never errors")
        }

        /// Strategy: a single page as (ids, has_more). next_cursor is derived.
        /// Ids may overlap across pages (deliberately) to exercise dedup.
        fn arb_page() -> impl Strategy<Value = (Vec<i64>, bool)> {
            (
                prop::collection::vec(0i64..50, 0..=(CAP as usize)),
                any::<bool>(),
            )
        }

        /// Strategy: a free-form script of up to 12 pages plus a target.
        fn arb_script() -> impl Strategy<Value = (usize, Vec<(Vec<i64>, bool)>)> {
            (0usize..=200, prop::collection::vec(arb_page(), 0..=12))
        }

        /// Turn a (ids, has_more) spec into a `VideoPage` with a sensible cursor.
        fn page_from(ids: Vec<i64>, has_more: bool, cursor_seed: i64) -> VideoPage {
            VideoPage {
                videos: ids.into_iter().map(aweme).collect(),
                next_cursor: Some(cursor_seed + 1),
                has_more,
            }
        }

        proptest! {
            /// A. Returned aweme_ids are always unique, for ANY script
            /// (including overlapping ids across pages).
            #[test]
            fn prop_dedup_unique((target, pages) in arb_script()) {
                let script: Vec<VideoPage> = pages
                    .into_iter()
                    .enumerate()
                    .map(|(i, (ids, has_more))| page_from(ids, has_more, i as i64))
                    .collect();
                let fake = FakeFetcher::new(script);
                let result = run(target, &fake);

                let unique: std::collections::HashSet<&String> =
                    result.iter().map(|v| &v.aweme_id).collect();
                prop_assert_eq!(unique.len(), result.len());
            }

            /// B. Result never exceeds target.
            #[test]
            fn prop_never_exceeds_target((target, pages) in arb_script()) {
                let script: Vec<VideoPage> = pages
                    .into_iter()
                    .enumerate()
                    .map(|(i, (ids, has_more))| page_from(ids, has_more, i as i64))
                    .collect();
                let fake = FakeFetcher::new(script);
                let result = run(target, &fake);

                prop_assert!(result.len() <= target);
            }

            /// C. Result is bounded by the total unique ids supplied across the
            /// pages the loop actually consumed (the tighter supply oracle).
            #[test]
            fn prop_bounded_by_supply((target, pages) in arb_script()) {
                // Keep an owned copy of the id lists to compute the supply oracle
                // after the loop reports how many pages it consumed.
                let id_lists: Vec<Vec<i64>> =
                    pages.iter().map(|(ids, _)| ids.clone()).collect();
                let script: Vec<VideoPage> = pages
                    .into_iter()
                    .enumerate()
                    .map(|(i, (ids, has_more))| page_from(ids, has_more, i as i64))
                    .collect();
                let fake = FakeFetcher::new(script);
                let result = run(target, &fake);

                // The loop fetches `calls()` pages; the first `calls()` entries
                // of the script were consumed. Unique ids across those pages is
                // an upper bound on what could be collected.
                let consumed = fake.calls();
                let supplied_unique: std::collections::HashSet<i64> = id_lists
                    .iter()
                    .take(consumed)
                    .flatten()
                    .copied()
                    .collect();
                prop_assert!(result.len() <= supplied_unique.len());
            }

            /// E. Page-consumption guard: even when every page claims
            /// has_more=true forever, the loop makes at most
            /// `target / cap + 2` fetch calls.
            #[test]
            fn prop_calls_bounded_by_guard(target in 0usize..=200) {
                // Script longer than the guard, every page non-empty + has_more.
                // Use globally-unique ids so dedup never short-circuits supply.
                let guard = target / (CAP as usize) + 2;
                let n_pages = guard + 5;
                let mut next_id = 0i64;
                let script: Vec<VideoPage> = (0..n_pages)
                    .map(|i| {
                        let ids: Vec<i64> = (0..CAP as i64)
                            .map(|_| {
                                let id = next_id;
                                next_id += 1;
                                id
                            })
                            .collect();
                        page_from(ids, true, i as i64)
                    })
                    .collect();
                let fake = FakeFetcher::new(script);
                let _ = run(target, &fake);

                prop_assert!(fake.calls() <= guard);
            }
        }

        /// Well-formed script: globally-unique ids, every page non-empty,
        /// has_more=true on all but the last (last has_more=false).
        ///
        /// The number of pages is capped at the loop's `max_pages` guard for the
        /// chosen target (`target / CAP + 2`), so the guard never terminates the
        /// loop mid-stream — the only stop conditions are reaching `target` or
        /// exhausting the (finite, well-formed) script. That makes the exact
        /// oracle `min(target, total_supplied)` hold without modeling the guard.
        fn arb_wellformed() -> impl Strategy<Value = (usize, Vec<Vec<i64>>)> {
            (0usize..=200)
                .prop_flat_map(|target| {
                    let guard = target / (CAP as usize) + 2;
                    let max_pages = guard.min(12);
                    // Page sizes 1..=CAP; ids assigned uniquely below.
                    (
                        Just(target),
                        prop::collection::vec(1usize..=(CAP as usize), 1..=max_pages),
                    )
                })
                .prop_map(|(target, sizes)| {
                    let mut next_id = 0i64;
                    let pages: Vec<Vec<i64>> = sizes
                        .into_iter()
                        .map(|size| {
                            (0..size)
                                .map(|_| {
                                    let id = next_id;
                                    next_id += 1;
                                    id
                                })
                                .collect()
                        })
                        .collect();
                    (target, pages)
                })
        }

        proptest! {
            /// D. Exact accumulation for well-formed scripts: result length is
            /// exactly min(target, total_supplied), since ids are globally
            /// unique and there is no mid-stream termination.
            #[test]
            fn prop_exact_for_wellformed((target, pages) in arb_wellformed()) {
                let total_supplied: usize = pages.iter().map(|p| p.len()).sum();
                let last = pages.len() - 1;
                let script: Vec<VideoPage> = pages
                    .into_iter()
                    .enumerate()
                    .map(|(i, ids)| page_from(ids, i != last, i as i64))
                    .collect();
                let fake = FakeFetcher::new(script);
                let result = run(target, &fake);

                prop_assert_eq!(result.len(), target.min(total_supplied));
            }
        }
    }
}
