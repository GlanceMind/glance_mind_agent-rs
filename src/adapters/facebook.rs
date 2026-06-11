//! Facebook Adapter - Implements ContentGateway and CommentGateway via RapidAPI

use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as GovernorRateLimiter};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::pagination::StopReason;
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    content_gateway::{FetchOutcome, FetchShortfall},
    CommentGateway, ContentGateway,
};
use crate::strategies::facebook::{extra_keys, mode};

const DEFAULT_BASE_URL: &str = "https://facebook-scraper3.p.rapidapi.com";
const DEFAULT_HOST: &str = "facebook-scraper3.p.rapidapi.com";
const DEFAULT_REQUEST_INTERVAL_MS: u64 = 500;
const RATE_LIMIT_RETRY_DELAYS_SECS: [u64; 3] = [2, 5, 10];
const MAX_EMPTY_CURSOR_HOPS: usize = 3;

struct ResponsePayload {
    status: StatusCode,
    body: Value,
    retry_after_secs: Option<u64>,
}

/// 翻页循环终止信息(M2-T2,适配器私有):
/// - `Stop(reason)`:循环以 M1 D2 的 `StopReason` 语义终止;
/// - `Partial(err)`:`RateLimited` 且原始进展非空的先例分支,保留原错误
///   (零交付时由 `fetch_by_keyword_with_outcome` 原样 `Err`,DR-01)。
#[derive(Debug)]
enum FbFetchEnd {
    Stop(StopReason),
    Partial(GatewayError),
}

/// Facebook adapter implementing ContentGateway and CommentGateway.
pub struct FacebookAdapter {
    client: Client,
    api_key: String,
    api_host: String,
    base_url: String,
    rate_limiter: Arc<DefaultDirectRateLimiter>,
}

impl FacebookAdapter {
    /// Create a new Facebook adapter.
    pub fn new(
        api_key: impl Into<String>,
        api_host: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, GatewayError> {
        Self::new_with_quota(api_key, api_host, base_url, Self::default_quota())
    }

    /// Create a new Facebook adapter with a custom governor quota.
    pub fn new_with_quota(
        api_key: impl Into<String>,
        api_host: impl Into<String>,
        base_url: impl Into<String>,
        quota: Quota,
    ) -> Result<Self, GatewayError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| GatewayError::Network(err.to_string()))?;

        Ok(Self {
            client,
            api_key: api_key.into(),
            api_host: api_host.into(),
            base_url: base_url.into(),
            rate_limiter: Arc::new(GovernorRateLimiter::direct(quota)),
        })
    }

    /// Create from environment variables.
    pub fn from_env() -> Result<Self, GatewayError> {
        let api_key = std::env::var("FACEBOOK_RAPIDAPI_KEY")
            .map_err(|_| GatewayError::AuthFailed("FACEBOOK_RAPIDAPI_KEY not set".into()))?;
        let api_host =
            std::env::var("FACEBOOK_RAPIDAPI_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
        let base_url = std::env::var("FACEBOOK_RAPIDAPI_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        Self::new(api_key, api_host, base_url)
    }

    fn build_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn default_quota() -> Quota {
        Self::quota_from_interval_ms(DEFAULT_REQUEST_INTERVAL_MS)
    }

    fn quota_from_interval_ms(interval_ms: u64) -> Quota {
        Quota::with_period(Duration::from_millis(interval_ms.max(1)))
            .expect("Facebook governor quota period should be valid")
            .allow_burst(NonZeroU32::new(1).expect("burst size should be non-zero"))
    }

    async fn wait_for_rate_limit(&self, path: &str) {
        self.rate_limiter.until_ready().await;
        debug!(path, "Facebook RapidAPI governor permit acquired");
    }

    async fn request_json_once(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> GatewayResult<ResponsePayload> {
        self.wait_for_rate_limit(path).await;

        let response = self
            .client
            .get(self.build_url(path))
            .header("Content-Type", "application/json")
            .header("x-rapidapi-host", &self.api_host)
            .header("x-rapidapi-key", &self.api_key)
            .query(query)
            .send()
            .await
            .map_err(|err| GatewayError::Network(err.to_string()))?;

        let status = response.status();
        let retry_after_secs = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let body = response
            .text()
            .await
            .map_err(|err| GatewayError::Network(err.to_string()))?;

        let value =
            serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "message": body }));
        Ok(ResponsePayload {
            status,
            body: value,
            retry_after_secs,
        })
    }

    async fn request_json(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> GatewayResult<(StatusCode, Value, Option<u64>)> {
        for (attempt, fallback_delay_secs) in RATE_LIMIT_RETRY_DELAYS_SECS.iter().enumerate() {
            let response = self.request_json_once(path, query).await?;
            if response.status != StatusCode::TOO_MANY_REQUESTS {
                return Ok((response.status, response.body, response.retry_after_secs));
            }

            let delay_secs = response.retry_after_secs.unwrap_or(*fallback_delay_secs);
            warn!(
                path,
                attempt = attempt + 1,
                delay_secs,
                "Facebook RapidAPI rate limited, retrying"
            );
            sleep(Duration::from_secs(delay_secs)).await;
        }

        let response = self.request_json_once(path, query).await?;
        Ok((response.status, response.body, response.retry_after_secs))
    }

    fn body_message(value: &Value) -> String {
        if let Some(message) = value.get("message").and_then(Self::string_value) {
            return message;
        }
        if let Some(detail) = value.get("detail") {
            return detail.to_string();
        }
        value.to_string()
    }

    fn map_http_error(
        status: StatusCode,
        body: &Value,
        retry_after_secs: Option<u64>,
    ) -> GatewayError {
        let message = Self::body_message(body);
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => GatewayError::AuthFailed(message),
            StatusCode::NOT_FOUND => GatewayError::NotFound(message),
            StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST => {
                GatewayError::InvalidParams(message)
            }
            StatusCode::TOO_MANY_REQUESTS => GatewayError::RateLimited { retry_after_secs },
            _ => GatewayError::Api {
                code: status.as_u16() as i32,
                message,
            },
        }
    }

    fn string_value(value: &Value) -> Option<String> {
        match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        }
    }

    fn get_string(value: &Value, key: &str) -> Option<String> {
        value.get(key).and_then(Self::string_value)
    }

    fn get_nested_string(value: &Value, path: &[&str]) -> Option<String> {
        let mut current = value;
        for key in path {
            current = current.get(*key)?;
        }
        Self::string_value(current)
    }

    fn get_i64(value: &Value, key: &str) -> Option<i64> {
        match value.get(key) {
            Some(Value::Number(number)) => number
                .as_i64()
                .or_else(|| number.as_u64().map(|v| v as i64)),
            Some(Value::String(number)) => number.parse().ok(),
            _ => None,
        }
    }

    fn extra_string(options: &SearchOptions, key: &str) -> Option<String> {
        options.extra.get(key).and_then(Self::string_value)
    }

    fn extra_bool(options: &SearchOptions, key: &str) -> Option<bool> {
        options.extra.get(key).and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            },
            _ => None,
        })
    }

    fn parse_date(date: &str, end_of_day: bool) -> Option<i64> {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        let naive = if end_of_day {
            date.and_hms_opt(23, 59, 59)?
        } else {
            date.and_hms_opt(0, 0, 0)?
        };
        Some(Utc.from_utc_datetime(&naive).timestamp())
    }

    fn filter_posts(mut posts: Vec<Content>, options: &SearchOptions) -> Vec<Content> {
        let start_ts = Self::extra_string(options, extra_keys::START_DATE)
            .and_then(|value| Self::parse_date(&value, false));
        let end_ts = Self::extra_string(options, extra_keys::END_DATE)
            .and_then(|value| Self::parse_date(&value, true));
        let recent_posts = Self::extra_bool(options, extra_keys::RECENT_POSTS).unwrap_or(false);

        if start_ts.is_some() || end_ts.is_some() {
            posts.retain(|post| {
                let Some(timestamp) = post.created_at else {
                    return false;
                };
                if let Some(start_ts) = start_ts {
                    if timestamp < start_ts {
                        return false;
                    }
                }
                if let Some(end_ts) = end_ts {
                    if timestamp > end_ts {
                        return false;
                    }
                }
                true
            });
        }

        if recent_posts || start_ts.is_some() || end_ts.is_some() {
            posts.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        }

        posts.truncate(options.count as usize);
        posts
    }

    fn discovery_query(query: &str, location: Option<String>) -> String {
        match location {
            Some(location) if !location.trim().is_empty() => format!("{query} {}", location.trim()),
            _ => query.to_string(),
        }
    }

    fn collect_results(body: &Value) -> Vec<&Value> {
        body.get("results")
            .and_then(|value| value.as_array())
            .map(|items| items.iter().collect())
            .unwrap_or_default()
    }

    fn next_cursor(body: &Value) -> Option<String> {
        Self::get_string(body, "cursor").filter(|cursor| !cursor.trim().is_empty())
    }

    fn append_cursor_query(
        query: &[(String, String)],
        cursor: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut next_query = query.to_vec();
        if let Some(cursor) = cursor.filter(|cursor| !cursor.trim().is_empty()) {
            next_query.push(("cursor".to_string(), cursor.to_string()));
        }
        next_query
    }

    fn convert_content(post: &Value) -> Option<Content> {
        let post_id = Self::get_string(post, "post_id")?;
        let author_id = Self::get_nested_string(post, &["author", "id"]);
        let author_name = Self::get_nested_string(post, &["author", "name"]);
        let author = author_id
            .clone()
            .or_else(|| author_name.clone())
            .unwrap_or_default();

        Some(Content {
            platform: "facebook".to_string(),
            content_id: post_id,
            author,
            author_name,
            description: Self::get_string(post, "message")
                .or_else(|| Self::get_string(post, "message_rich"))
                .unwrap_or_default(),
            url: Self::get_string(post, "url"),
            engagement: Engagement {
                likes: Self::get_i64(post, "reactions_count").unwrap_or(0),
                comments: Self::get_i64(post, "comments_count").unwrap_or(0),
                shares: Self::get_i64(post, "reshare_count").unwrap_or(0),
                views: 0,
            },
            created_at: Self::get_i64(post, "timestamp"),
            raw_data: Some(post.clone()),
        })
    }

    fn comment_text(comment: &Value) -> String {
        Self::get_string(comment, "message")
            .or_else(|| Self::get_nested_string(comment, &["sticker", "label"]))
            .unwrap_or_else(|| "[facebook comment without text]".to_string())
    }

    fn convert_comment(comment: &Value, post_id: &str) -> Option<Comment> {
        let comment_id = Self::get_string(comment, "legacy_comment_id")
            .or_else(|| Self::get_string(comment, "comment_id"))?;
        let author_id = Self::get_nested_string(comment, &["author", "id"]);
        let author_name = Self::get_nested_string(comment, &["author", "name"]);
        let author = author_id
            .clone()
            .or_else(|| author_name.clone())
            .unwrap_or_default();
        let depth = Self::get_i64(comment, "depth").unwrap_or(0);

        Some(Comment {
            platform: "facebook".to_string(),
            comment_id,
            content_id: post_id.to_string(),
            parent_id: Self::get_string(comment, "parent_comment_id"),
            author,
            author_name,
            author_uid: author_id,
            text: Self::comment_text(comment),
            likes: Self::get_i64(comment, "reactions_count").unwrap_or(0),
            reply_count: Self::get_i64(comment, "replies_count").unwrap_or(0) as i32,
            created_at: Self::get_i64(comment, "created_time"),
            language: None,
            is_reply: depth > 0 || Self::get_string(comment, "parent_comment_id").is_some(),
            raw_data: Some(comment.clone()),
        })
    }

    async fn search_posts_page(
        &self,
        query: &str,
        cursor: Option<&str>,
    ) -> GatewayResult<(Vec<Content>, Option<String>)> {
        let request_query =
            Self::append_cursor_query(&[("query".to_string(), query.to_string())], cursor);
        let (status, body, retry_after_secs) =
            self.request_json("/search/posts", &request_query).await?;
        if !status.is_success() {
            return Err(Self::map_http_error(status, &body, retry_after_secs));
        }

        Ok((
            Self::collect_results(&body)
                .into_iter()
                .filter_map(Self::convert_content)
                .collect(),
            Self::next_cursor(&body),
        ))
    }

    async fn search_pages_page(
        &self,
        query: &str,
        path: &str,
        cursor: Option<&str>,
    ) -> GatewayResult<(Vec<Value>, Option<String>)> {
        let request_query =
            Self::append_cursor_query(&[("query".to_string(), query.to_string())], cursor);
        let (status, body, retry_after_secs) = self.request_json(path, &request_query).await?;
        if !status.is_success() {
            return Err(Self::map_http_error(status, &body, retry_after_secs));
        }

        Ok((
            Self::collect_results(&body).into_iter().cloned().collect(),
            Self::next_cursor(&body),
        ))
    }

    async fn fetch_page_posts_page(
        &self,
        page_id: &str,
        cursor: Option<&str>,
    ) -> GatewayResult<(Vec<Content>, Option<String>)> {
        let request_query =
            Self::append_cursor_query(&[("page_id".to_string(), page_id.to_string())], cursor);
        let (status, body, retry_after_secs) =
            self.request_json("/page/posts", &request_query).await?;
        if !status.is_success() {
            if status == StatusCode::SERVICE_UNAVAILABLE
                && body.get("results").is_some_and(Value::is_null)
            {
                return Ok((Vec::new(), None));
            }
            return Err(Self::map_http_error(status, &body, retry_after_secs));
        }

        Ok((
            Self::collect_results(&body)
                .into_iter()
                .filter_map(Self::convert_content)
                .collect(),
            Self::next_cursor(&body),
        ))
    }

    async fn resolve_page_id(&self, identifier: &str) -> GatewayResult<Option<String>> {
        if identifier.chars().all(|ch| ch.is_ascii_digit()) {
            return Ok(Some(identifier.to_string()));
        }

        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        let mut empty_hops = 0;

        loop {
            let (pages, next_cursor) = self
                .search_pages_page(identifier, "/search/pages", cursor.as_deref())
                .await?;

            if let Some(page_id) = pages
                .into_iter()
                .find_map(|page| Self::get_string(&page, "facebook_id"))
            {
                return Ok(Some(page_id));
            }

            if next_cursor.is_none() {
                return Ok(None);
            }

            empty_hops += 1;
            if empty_hops >= MAX_EMPTY_CURSOR_HOPS {
                warn!(
                    identifier,
                    empty_hops, "Facebook page resolution exhausted empty cursor hops"
                );
                return Ok(None);
            }

            let next_cursor = next_cursor.expect("checked above");
            if !seen_cursors.insert(next_cursor.clone()) {
                return Ok(None);
            }
            cursor = Some(next_cursor);
        }
    }

    async fn fetch_post(&self, post_lookup_id: &str) -> GatewayResult<Option<Content>> {
        let (status, body, retry_after_secs) = self
            .request_json(
                "/post",
                &[("post_id".to_string(), post_lookup_id.to_string())],
            )
            .await?;

        if !status.is_success() {
            if status == StatusCode::SERVICE_UNAVAILABLE
                && body.get("results").is_some_and(Value::is_null)
            {
                return Ok(None);
            }
            return Err(Self::map_http_error(status, &body, retry_after_secs));
        }

        Ok(body.get("results").and_then(Self::convert_content))
    }

    fn reached_post_limit(posts: &[Content], options: &SearchOptions) -> bool {
        Self::filter_posts(posts.to_vec(), options).len() >= options.count as usize
    }

    async fn search_posts_paginated(
        &self,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, FbFetchEnd)> {
        let mut posts = Vec::new();
        let mut seen_post_ids = HashSet::new();
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        let mut empty_hops = 0;

        let end = loop {
            match self
                .search_posts_page(&options.query, cursor.as_deref())
                .await
            {
                Ok((page_posts, next_cursor)) => {
                    let mut newly_added = 0_usize;
                    for post in page_posts {
                        if seen_post_ids.insert(post.content_id.clone()) {
                            posts.push(post);
                            newly_added += 1;
                        }
                    }

                    if Self::reached_post_limit(&posts, options) {
                        break FbFetchEnd::Stop(StopReason::ReachedMaxCount);
                    }

                    // D-15:空页计数按「本页新增(去重后)数 == 0」递增
                    // (对齐 M1 D2 empty_streak 冻结语义,重复内容页与字面空页同等计入)。
                    if newly_added == 0 {
                        empty_hops += 1;
                        if empty_hops >= MAX_EMPTY_CURSOR_HOPS {
                            warn!(
                                query = %options.query,
                                empty_hops,
                                "Facebook post search exhausted empty cursor hops"
                            );
                            break FbFetchEnd::Stop(StopReason::EmptyPageLimit);
                        }
                    } else {
                        empty_hops = 0;
                    }

                    let Some(next_cursor) = next_cursor else {
                        break FbFetchEnd::Stop(StopReason::UpstreamExhausted);
                    };
                    if !seen_cursors.insert(next_cursor.clone()) {
                        break FbFetchEnd::Stop(StopReason::CursorLoop);
                    }
                    cursor = Some(next_cursor);
                }
                Err(err @ GatewayError::RateLimited { .. }) if !posts.is_empty() => {
                    warn!(
                        query = %options.query,
                        collected = posts.len(),
                        "Facebook post search hit rate limit after partial progress"
                    );
                    break FbFetchEnd::Partial(err);
                }
                Err(err) => return Err(err),
            }
        };

        Ok((Self::filter_posts(posts, options), end))
    }

    async fn fetch_page_posts_paginated(
        &self,
        page_id: &str,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, FbFetchEnd)> {
        let mut posts = Vec::new();
        let mut seen_post_ids = HashSet::new();
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        let mut empty_hops = 0;

        let end = loop {
            match self.fetch_page_posts_page(page_id, cursor.as_deref()).await {
                Ok((page_posts, next_cursor)) => {
                    let mut newly_added = 0_usize;
                    for post in page_posts {
                        if seen_post_ids.insert(post.content_id.clone()) {
                            posts.push(post);
                            newly_added += 1;
                        }
                    }

                    if Self::reached_post_limit(&posts, options) {
                        break FbFetchEnd::Stop(StopReason::ReachedMaxCount);
                    }

                    // D-15:空页计数按「本页新增(去重后)数 == 0」递增(对齐 M1 D2)。
                    if newly_added == 0 {
                        empty_hops += 1;
                        if empty_hops >= MAX_EMPTY_CURSOR_HOPS {
                            warn!(
                                page_id,
                                empty_hops, "Facebook page/posts exhausted empty cursor hops"
                            );
                            break FbFetchEnd::Stop(StopReason::EmptyPageLimit);
                        }
                    } else {
                        empty_hops = 0;
                    }

                    let Some(next_cursor) = next_cursor else {
                        break FbFetchEnd::Stop(StopReason::UpstreamExhausted);
                    };
                    if !seen_cursors.insert(next_cursor.clone()) {
                        break FbFetchEnd::Stop(StopReason::CursorLoop);
                    }
                    cursor = Some(next_cursor);
                }
                Err(err @ GatewayError::RateLimited { .. }) if !posts.is_empty() => {
                    warn!(
                        page_id,
                        collected = posts.len(),
                        "Facebook page/posts hit rate limit after partial progress"
                    );
                    break FbFetchEnd::Partial(err);
                }
                Err(err) => return Err(err),
            }
        };

        Ok((Self::filter_posts(posts, options), end))
    }

    async fn fetch_posts_from_search_candidates(
        &self,
        query: &str,
        path: &str,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, FbFetchEnd)> {
        let mut posts = Vec::new();
        let mut seen_post_ids = HashSet::new();
        let mut candidate_cursor = None;
        let mut seen_candidate_cursors = HashSet::new();
        let mut empty_hops = 0;

        let end = loop {
            match self
                .search_pages_page(query, path, candidate_cursor.as_deref())
                .await
            {
                Ok((candidates, next_cursor)) => {
                    let posts_before = posts.len();

                    for candidate in candidates {
                        let Some(page_id) = Self::get_string(&candidate, "facebook_id") else {
                            continue;
                        };

                        let remaining = options
                            .count
                            .saturating_sub(Self::filter_posts(posts.clone(), options).len() as u32)
                            .max(1);
                        let page_options = options.clone().with_count(remaining);
                        // DR-17c:内层 fetch_page_posts_paginated 的终止信息(枯竭/环/空页)
                        // 不外泄为整体 shortfall;shortfall 只由最外层最终出口决定。
                        let (page_posts, _inner_end) = match self
                            .fetch_page_posts_paginated(&page_id, &page_options)
                            .await
                        {
                            Ok(inner) => inner,
                            Err(err @ GatewayError::RateLimited { .. }) if !posts.is_empty() => {
                                warn!(
                                    page_id,
                                    collected = posts.len(),
                                    "Facebook candidate page crawl hit rate limit after partial progress"
                                );
                                return Ok((
                                    Self::filter_posts(posts, options),
                                    FbFetchEnd::Partial(err),
                                ));
                            }
                            Err(err) => return Err(err),
                        };

                        for post in page_posts {
                            if seen_post_ids.insert(post.content_id.clone()) {
                                posts.push(post);
                            }
                        }

                        if Self::reached_post_limit(&posts, options) {
                            return Ok((
                                Self::filter_posts(posts, options),
                                FbFetchEnd::Stop(StopReason::ReachedMaxCount),
                            ));
                        }
                    }

                    // D-15:空页计数按「本候选页新增(去重后)帖子数 == 0」递增(对齐 M1 D2)。
                    if posts.len() == posts_before {
                        empty_hops += 1;
                        if empty_hops >= MAX_EMPTY_CURSOR_HOPS {
                            warn!(
                                query,
                                empty_hops, "Facebook candidate search exhausted empty cursor hops"
                            );
                            break FbFetchEnd::Stop(StopReason::EmptyPageLimit);
                        }
                    } else {
                        empty_hops = 0;
                    }

                    let Some(next_cursor) = next_cursor else {
                        break FbFetchEnd::Stop(StopReason::UpstreamExhausted);
                    };
                    if !seen_candidate_cursors.insert(next_cursor.clone()) {
                        break FbFetchEnd::Stop(StopReason::CursorLoop);
                    }
                    candidate_cursor = Some(next_cursor);
                }
                Err(err @ GatewayError::RateLimited { .. }) if !posts.is_empty() => {
                    warn!(
                        query,
                        collected = posts.len(),
                        "Facebook candidate search hit rate limit after partial progress"
                    );
                    break FbFetchEnd::Partial(err);
                }
                Err(err) => return Err(err),
            }
        };

        Ok((Self::filter_posts(posts, options), end))
    }

    async fn search_keyword_mode(
        &self,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, FbFetchEnd)> {
        let search_type = Self::extra_string(options, extra_keys::SEARCH_TYPE)
            .unwrap_or_else(|| "posts".to_string());
        let location = Self::extra_string(options, extra_keys::LOCATION);
        let discovery_query = Self::discovery_query(&options.query, location);

        match search_type.as_str() {
            "posts" => {
                self.search_posts_paginated(&options.with_query(discovery_query))
                    .await
            }
            "pages" => {
                self.fetch_posts_from_search_candidates(&discovery_query, "/search/pages", options)
                    .await
            }
            "places" => {
                self.fetch_posts_from_search_candidates(&discovery_query, "/search/places", options)
                    .await
            }
            other => Err(GatewayError::InvalidParams(format!(
                "unsupported facebook search_type: {other}"
            ))),
        }
    }

    async fn search_page_mode(
        &self,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, Option<FbFetchEnd>)> {
        let Some(page_id) = self.resolve_page_id(&options.query).await? else {
            return Ok((Vec::new(), None));
        };
        let (posts, end) = self.fetch_page_posts_paginated(&page_id, options).await?;
        Ok((posts, Some(end)))
    }

    async fn search_post_mode(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let lookup_id = Self::extra_string(options, extra_keys::POST_LOOKUP_ID)
            .unwrap_or_else(|| options.query.clone());
        Ok(self.fetch_post(&lookup_id).await?.into_iter().collect())
    }

    /// `search()` 的带终止信息形态(M2-T2):`end == None` 表示该路径无翻页终止信息
    /// (post 模式 / page 解析失败的空结果),沿 D1 滚动兼容语义映射 `shortfall = None`。
    async fn search_with_end(
        &self,
        options: &SearchOptions,
    ) -> GatewayResult<(Vec<Content>, Option<FbFetchEnd>)> {
        let mode = Self::extra_string(options, extra_keys::MODE)
            .unwrap_or_else(|| mode::KEYWORD.to_string());

        info!(
            query = %options.query,
            mode = %mode,
            search_type = ?Self::extra_string(options, extra_keys::SEARCH_TYPE),
            "Facebook search request"
        );

        match mode.as_str() {
            mode::KEYWORD => {
                let (posts, end) = self.search_keyword_mode(options).await?;
                Ok((posts, Some(end)))
            }
            mode::PAGE => self.search_page_mode(options).await,
            mode::POST_URL => Ok((self.search_post_mode(options).await?, None)),
            other => Err(GatewayError::InvalidParams(format!(
                "unsupported facebook search mode: {other}"
            ))),
        }
    }
}

#[async_trait]
impl ContentGateway for FacebookAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let (posts, _end) = self.search_with_end(options).await?;
        Ok(posts)
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        debug!(keyword = ?keyword, query = %options.query, "Facebook fetch_by_keyword");
        self.search(options).await
    }

    /// M2-T2:override D1 默认方法,把翻页循环的终止/失败原因翻译为 `shortfall`
    /// (映射语义与 `PaginationLoop::shortfall_for` 一致;DR-09:`delivered` = 过滤后交付集)。
    async fn fetch_by_keyword_with_outcome(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<FetchOutcome> {
        debug!(keyword = ?keyword, query = %options.query, "Facebook fetch_by_keyword_with_outcome");
        let (contents, end) = self.search_with_end(options).await?;
        let delivered = contents.len();
        let target = options.count as usize;

        let shortfall = match end {
            // 无翻页终止信息的路径(post 模式等):滚动兼容语义,shortfall = None。
            None => None,
            Some(FbFetchEnd::Stop(_)) if delivered >= target => None,
            Some(FbFetchEnd::Stop(StopReason::ReachedMaxCount)) => None,
            Some(FbFetchEnd::Stop(
                StopReason::UpstreamExhausted
                | StopReason::CursorLoop
                | StopReason::EmptyPageLimit,
            )) => Some(FetchShortfall::Exhausted),
            Some(FbFetchEnd::Partial(err)) => {
                if contents.is_empty() {
                    // DR-01:零可交付进展(含 raw 有进展但全被 date-filter 滤除)→ 原样 Err。
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
        let options = SearchOptions::new(user_id.to_string())
            .with_platform("facebook")
            .with_count(count)
            .with_extra_value(extra_keys::MODE, json!(mode::PAGE))
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("posts"));
        self.search(&options).await
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        self.fetch_post(content_id).await
    }

    fn platform(&self) -> &str {
        "facebook"
    }
}

#[async_trait]
impl CommentGateway for FacebookAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        let mut query = vec![("post_id".to_string(), content_id.to_string())];
        if let Some(cursor) = &options.cursor {
            query.push(("cursor".to_string(), cursor.clone()));
        }

        let (status, body, retry_after_secs) = self.request_json("/post/comments", &query).await?;
        if !status.is_success() {
            if status == StatusCode::SERVICE_UNAVAILABLE
                && body.get("results").is_some_and(Value::is_null)
            {
                return Ok(FetchCommentsResult::empty());
            }
            return Err(Self::map_http_error(status, &body, retry_after_secs));
        }

        let comments: Vec<Comment> = Self::collect_results(&body)
            .into_iter()
            .filter_map(|comment| Self::convert_comment(comment, content_id))
            .take(options.count as usize)
            .collect();
        let next_cursor = Self::get_string(&body, "cursor");
        let has_more = next_cursor.is_some();
        let total = body
            .get("results")
            .and_then(|value| value.as_array())
            .map(|items| items.len() as i64);

        Ok(FetchCommentsResult::new(comments)
            .with_pagination(has_more, next_cursor)
            .with_total(total.unwrap_or(0)))
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        let mut comments = Vec::new();
        let mut seen_comment_ids = HashSet::new();
        let mut cursor = None;
        let mut seen_cursors = HashSet::new();
        let mut empty_hops = 0;

        while comments.len() < max_count as usize {
            let fetch_count = (max_count - comments.len() as u32).min(50);
            let result = match self
                .fetch_comments(
                    content_id,
                    &FetchCommentsOptions::new(fetch_count).with_cursor_optional(cursor.clone()),
                )
                .await
            {
                Ok(result) => result,
                Err(GatewayError::RateLimited { .. }) if !comments.is_empty() => {
                    warn!(
                        content_id,
                        collected = comments.len(),
                        "Facebook comments pagination hit rate limit after partial progress"
                    );
                    break;
                }
                Err(err) => return Err(err),
            };

            let page_count = result.comments.len();

            for comment in result.comments {
                if seen_comment_ids.insert(comment.comment_id.clone()) {
                    comments.push(comment);
                }
            }
            comments.truncate(max_count as usize);

            let Some(next_cursor) = result.next_cursor else {
                break;
            };

            if page_count == 0 {
                empty_hops += 1;
                if empty_hops >= MAX_EMPTY_CURSOR_HOPS {
                    warn!(
                        content_id,
                        empty_hops, "Facebook comments pagination exhausted empty cursor hops"
                    );
                    break;
                }
            } else {
                empty_hops = 0;
            }

            if !result.has_more || !seen_cursors.insert(next_cursor.clone()) {
                break;
            }
            cursor = Some(next_cursor);
        }

        Ok(comments)
    }

    async fn fetch_replies(
        &self,
        _content_id: &str,
        _comment_id: &str,
        _options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        Ok(Vec::new())
    }

    fn platform(&self) -> &str {
        "facebook"
    }
}

trait CursorExt {
    fn with_cursor_optional(self, cursor: Option<String>) -> Self;
}

impl CursorExt for FetchCommentsOptions {
    fn with_cursor_optional(mut self, cursor: Option<String>) -> Self {
        self.cursor = cursor;
        self
    }
}

/// Test-only helpers on FacebookAdapter.
/// Adds a constructor that bypasses system proxy (required when http_proxy is set
/// on the dev machine so that 127.0.0.1 mock servers are not proxied through it).
#[cfg(test)]
impl FacebookAdapter {
    /// Build a test adapter with a no-proxy reqwest client and custom governor quota.
    /// Use this instead of `new_with_quota` inside all `#[cfg(test)]` contexts.
    fn new_no_proxy_with_quota(
        api_key: impl Into<String>,
        api_host: impl Into<String>,
        base_url: impl Into<String>,
        quota: Quota,
    ) -> Result<Self, GatewayError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .map_err(|err| GatewayError::Network(err.to_string()))?;
        Ok(Self {
            client,
            api_key: api_key.into(),
            api_host: api_host.into(),
            base_url: base_url.into(),
            rate_limiter: Arc::new(GovernorRateLimiter::direct(quota)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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

    fn test_post(post_id: &str, message: &str) -> Value {
        json!({
            "post_id": post_id,
            "type": "status",
            "url": format!("https://www.facebook.com/test/posts/{post_id}"),
            "message": message,
            "message_rich": message,
            "timestamp": 1_763_449_200,
            "reactions_count": 10,
            "comments_count": 5,
            "reshare_count": 2,
            "author": {
                "id": "page-1",
                "name": "Test Page",
                "url": "https://www.facebook.com/test-page",
                "profile_picture_url": "https://mock-cdn.example.com/facebook/test-page.jpg"
            }
        })
    }

    fn test_comment(comment_id: &str, message: &str) -> Value {
        json!({
            "legacy_comment_id": comment_id,
            "comment_id": comment_id,
            "message": message,
            "author": {
                "id": format!("user-{comment_id}"),
                "name": format!("User {comment_id}"),
                "url": format!("https://www.facebook.com/{comment_id}"),
                "profile_image": format!("https://mock-cdn.example.com/facebook/{comment_id}.jpg")
            },
            "reactions_count": 1,
            "replies_count": 0,
            "depth": 0,
            "created_time": 1_763_449_260
        })
    }

    #[test]
    fn test_discovery_query_with_location() {
        let query = FacebookAdapter::discovery_query("travel", Some("beijing,china".to_string()));
        assert_eq!(query, "travel beijing,china");
    }

    #[test]
    fn test_filter_posts_by_date_range() {
        let options = SearchOptions::new("travel")
            .with_count(10)
            .with_extra_value(extra_keys::START_DATE, json!("2026-01-10"))
            .with_extra_value(extra_keys::END_DATE, json!("2026-01-12"));
        let posts = vec![
            Content::new("facebook", "1")
                .with_created_at(FacebookAdapter::parse_date("2026-01-10", false).unwrap()),
            Content::new("facebook", "2")
                .with_created_at(FacebookAdapter::parse_date("2026-01-13", false).unwrap()),
        ];

        let filtered = FacebookAdapter::filter_posts(posts, &options);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content_id, "1");
    }

    #[test]
    fn test_convert_comment_uses_sticker_label() {
        let comment = json!({
            "legacy_comment_id": "123",
            "depth": 0,
            "message": null,
            "sticker": { "label": "Sticker reply" },
            "author": { "id": "user-1", "name": "Sticker User" },
            "replies_count": 0,
            "reactions_count": "2",
            "created_time": 1700000000
        });

        let converted = FacebookAdapter::convert_comment(&comment, "post-1").unwrap();
        assert_eq!(converted.text, "Sticker reply");
        assert_eq!(converted.comment_id, "123");
    }

    #[tokio::test]
    async fn test_search_pages_uses_all_candidates_until_post_found() {
        let base_url = spawn_mock_http_server(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [
                        {"facebook_id": "page-empty", "name": "Empty Page"},
                        {"facebook_id": "page-good", "name": "Good Page"}
                    ],
                    "cursor": null
                }),
            ),
            MockHttpResponse::json(200, json!({"results": [], "cursor": null})),
            MockHttpResponse::json(
                200,
                json!({"results": [test_post("post-good", "from-second-page-candidate")], "cursor": null}),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let options = SearchOptions::new("museum")
            .with_platform("facebook")
            .with_count(1)
            .with_extra_value(extra_keys::MODE, json!(mode::KEYWORD))
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("pages"));

        let posts = adapter.search(&options).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].content_id, "post-good");
    }

    #[tokio::test]
    async fn test_search_posts_forwards_cursor_between_requests() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_post("post-1", "page-one")],
                    "cursor": "cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_post("post-2", "page-two")],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let options = SearchOptions::new("travel")
            .with_platform("facebook")
            .with_count(2)
            .with_extra_value(extra_keys::MODE, json!(mode::KEYWORD))
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("posts"));

        let posts = adapter.search(&options).await.unwrap();
        assert_eq!(posts.len(), 2);

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("GET /search/posts?query=travel HTTP/1.1"));
        assert!(requests[1].contains("GET /search/posts?query=travel&cursor=cursor-2 HTTP/1.1"));
    }

    #[tokio::test]
    async fn test_search_posts_continues_across_empty_page_when_cursor_advances() {
        let base_url = spawn_mock_http_server(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [],
                    "cursor": "cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_post("post-2", "page-two")],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let options = SearchOptions::new("travel")
            .with_platform("facebook")
            .with_count(1)
            .with_extra_value(extra_keys::MODE, json!(mode::KEYWORD))
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("posts"));

        let posts = adapter.search(&options).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].content_id, "post-2");
    }

    #[tokio::test]
    async fn test_search_page_mode_paginates_page_posts_until_count_reached() {
        let base_url = spawn_mock_http_server(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_post("post-1", "page-one")],
                    "cursor": "cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_post("post-2", "page-two")],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let options = SearchOptions::new("123456")
            .with_platform("facebook")
            .with_count(2)
            .with_extra_value(extra_keys::MODE, json!(mode::PAGE));

        let posts = adapter.search(&options).await.unwrap();
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].content_id, "post-1");
        assert_eq!(posts[1].content_id, "post-2");
    }

    #[tokio::test]
    async fn test_page_mode_resolves_page_id_across_search_cursor_pages() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [],
                    "cursor": "page-search-cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [{"facebook_id": "page-2", "name": "Second Page"}],
                    "cursor": null
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_post("post-2", "resolved-after-second-page-search")],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let options = SearchOptions::new("NatGeoMuseum")
            .with_platform("facebook")
            .with_count(1)
            .with_extra_value(extra_keys::MODE, json!(mode::PAGE));

        let posts = adapter.search(&options).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].content_id, "post-2");

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].contains("GET /search/pages?query=NatGeoMuseum HTTP/1.1"));
        assert!(requests[1]
            .contains("GET /search/pages?query=NatGeoMuseum&cursor=page-search-cursor-2 HTTP/1.1"));
        assert!(requests[2].contains("GET /page/posts?page_id=page-2 HTTP/1.1"));
    }

    #[tokio::test]
    async fn test_fetch_all_comments_forwards_cursor_between_requests() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_comment("comment-1", "first")],
                    "cursor": "comment-cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_comment("comment-2", "second")],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let comments = adapter.fetch_all_comments("post-1", 2).await.unwrap();
        assert_eq!(comments.len(), 2);

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("GET /post/comments?post_id=post-1 HTTP/1.1"));
        assert!(requests[1]
            .contains("GET /post/comments?post_id=post-1&cursor=comment-cursor-2 HTTP/1.1"));
    }

    #[tokio::test]
    async fn test_fetch_all_comments_continues_across_empty_page_when_cursor_advances() {
        let base_url = spawn_mock_http_server(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [],
                    "cursor": "comment-cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [test_comment("comment-2", "second")],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let comments = adapter.fetch_all_comments("post-1", 1).await.unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].comment_id, "comment-2");
    }

    #[tokio::test]
    async fn test_fetch_all_comments_deduplicates_overlapping_cursor_pages() {
        let base_url = spawn_mock_http_server(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [
                        test_comment("comment-1", "first"),
                        test_comment("comment-2", "second")
                    ],
                    "cursor": "comment-cursor-2"
                }),
            ),
            MockHttpResponse::json(
                200,
                json!({
                    "results": [
                        test_comment("comment-2", "second"),
                        test_comment("comment-3", "third")
                    ],
                    "cursor": null
                }),
            ),
        ])
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();
        let comments = adapter.fetch_all_comments("post-1", 10).await.unwrap();
        let ids = comments
            .iter()
            .map(|comment| comment.comment_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["comment-1", "comment-2", "comment-3"]);
    }

    #[tokio::test]
    async fn test_governor_rate_limiter_applies_to_retries() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(
            (0..4)
                .map(|_| {
                    MockHttpResponse::json(429, json!({"message": "rate limited"}))
                        .with_header("retry-after", "0")
                })
                .collect(),
        )
        .await;

        let adapter = FacebookAdapter::new_no_proxy_with_quota(
            "test-key",
            DEFAULT_HOST,
            base_url,
            FacebookAdapter::quota_from_interval_ms(30),
        )
        .unwrap();

        let started = std::time::Instant::now();
        let error = adapter.search_posts_page("travel", None).await.unwrap_err();
        let elapsed = started.elapsed();

        assert!(matches!(error, GatewayError::RateLimited { .. }));
        assert_eq!(requests.lock().unwrap().len(), 4);
        assert!(
            elapsed >= Duration::from_millis(70),
            "governor should throttle retries, elapsed was {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_fetch_all_comments_returns_partial_after_rate_limit() {
        let mut responses = vec![MockHttpResponse::json(
            200,
            json!({
                "results": [
                    test_comment("comment-1", "first"),
                    test_comment("comment-2", "second")
                ],
                "cursor": "cursor-2"
            }),
        )];
        responses.extend((0..4).map(|_| {
            MockHttpResponse::json(429, json!({"message": "rate limited"}))
                .with_header("retry-after", "0")
        }));

        let base_url = spawn_mock_http_server(responses).await;
        let adapter = FacebookAdapter::new_no_proxy_with_quota("test-key", DEFAULT_HOST, base_url, FacebookAdapter::quota_from_interval_ms(5)).unwrap();

        let comments = adapter.fetch_all_comments("post-1", 5).await.unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].comment_id, "comment-1");
        assert_eq!(comments[1].comment_id, "comment-2");
    }

    // ──────────────────────────────────────────────────────────────────
    // M2-T2 测试载荷(m2-facebook-p0.md §4 M2-T2,测试 1~13;断言 = 计划原文契约)
    // 全部经 `fetch_by_keyword_with_outcome` 调用;helper 仅扩展既有 mock-HTTP 样板
    // (FR-003,零新依赖)。
    // ──────────────────────────────────────────────────────────────────

    use crate::pagination::{PageDecision, PaginationLoop};
    use crate::ports::content_gateway::{FetchOutcome, FetchShortfall};

    /// 带可控 created_at 的 `test_post` 变体(date-filter 测试用):沿既有 fixture
    /// 形状,仅覆写 `timestamp` 字段,不凭空捏造字段(PV-001)。
    fn test_post_with_timestamp(post_id: &str, message: &str, timestamp: i64) -> Value {
        let mut post = test_post(post_id, message);
        post["timestamp"] = json!(timestamp);
        post
    }

    fn unique_posts(prefix: &str, n: usize) -> Vec<Value> {
        (0..n)
            .map(|i| test_post(&format!("{prefix}-{i}"), "fixture-message"))
            .collect()
    }

    fn page_response(posts: Vec<Value>, cursor: Option<&str>) -> MockHttpResponse {
        MockHttpResponse::json(200, json!({ "results": posts, "cursor": cursor }))
    }

    /// n 个排队的 429 响应(各带 `retry-after: 0`,DR-05 同形;
    /// 沿 test_governor_rate_limiter_applies_to_retries 样板)。
    fn rate_limited_responses(n: usize) -> Vec<MockHttpResponse> {
        (0..n)
            .map(|_| {
                MockHttpResponse::json(429, json!({"message": "rate limited"}))
                    .with_header("retry-after", "0")
            })
            .collect()
    }

    /// 测试用快速 governor 装配(沿 new_with_quota 既有样板,避免默认 500ms 间隔拖慢测试)。
    /// 使用 no_proxy 客户端,避免系统代理(如 http_proxy=127.0.0.1:7890)干扰 mock HTTP 服务器。
    fn fast_adapter(base_url: impl Into<String>) -> FacebookAdapter {
        FacebookAdapter::new_no_proxy_with_quota(
            "test-key",
            DEFAULT_HOST,
            base_url,
            FacebookAdapter::quota_from_interval_ms(5),
        )
        .unwrap()
    }

    fn search_keyword() -> KeywordType {
        KeywordType::Search("travel".to_string())
    }

    fn keyword_search_options(count: u32) -> SearchOptions {
        SearchOptions::new("travel")
            .with_platform("facebook")
            .with_count(count)
            .with_extra_value(extra_keys::MODE, json!(mode::KEYWORD))
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("posts"))
    }

    /// M2-T2 测试 1(R-002 / T-010 / PV-001 充足形状)。
    /// 允许先绿 + AG-006 金丝雀标注(DR-12):RED 基线下默认方法包装的既有循环已具备
    /// 达量行为;金丝雀程序 = 临时 truncate 交付集(只动生产代码)须使本测试变红,
    /// 还原复绿,输出留存(归实现/收尾上下文执行)。
    #[tokio::test]
    async fn fetch_outcome_reaches_count_no_shortfall() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            page_response(unique_posts("p1", 20), Some("cursor-2")),
            page_response(unique_posts("p2", 20), Some("cursor-3")),
            page_response(unique_posts("p3", 10), None),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 50);
        assert!(outcome.shortfall.is_none());

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert!(requests[1].contains("GET /search/posts?query=travel&cursor=cursor-2 HTTP/1.1"));
        assert!(requests[2].contains("GET /search/posts?query=travel&cursor=cursor-3 HTTP/1.1"));
    }

    /// M2-T2 测试 2(F-003):cursor 在达 count 前断链 → Exhausted。
    #[tokio::test]
    async fn fetch_outcome_exhausted_when_cursor_null_before_count() {
        let base_url = spawn_mock_http_server(vec![
            page_response(unique_posts("p1", 20), Some("cursor-2")),
            page_response(unique_posts("p2", 10), None),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 30);
        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    }

    /// M2-T2 测试 3(F-004):第 2 页返回与第 1 页相同 cursor、未达 count
    /// → seen-cursor 终止按枯竭语义接 shortfall。
    #[tokio::test]
    async fn fetch_outcome_cursor_loop_maps_exhausted() {
        let base_url = spawn_mock_http_server(vec![
            page_response(unique_posts("p1", 20), Some("cursor-loop")),
            page_response(unique_posts("p2", 10), Some("cursor-loop")),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    }

    /// M2-T2 测试 4(F-005):连续 3 空页(cursor 各异)、未达 count
    /// → MAX_EMPTY_CURSOR_HOPS 出口接 shortfall。
    #[tokio::test]
    async fn fetch_outcome_empty_page_limit_maps_exhausted() {
        let base_url = spawn_mock_http_server(vec![
            page_response(Vec::new(), Some("empty-1")),
            page_response(Vec::new(), Some("empty-2")),
            page_response(Vec::new(), Some("empty-3")),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    }

    /// M2-T2 测试 5(F-002 / F-006 / DR-05):第 1 页 20 条 + 第 2 页 4×429
    /// (重试耗尽)→ 有进展走 PartialFailure;请求数 = 1 + 4 == 5。
    #[tokio::test]
    async fn fetch_outcome_partial_failure_with_progress() {
        let mut responses = vec![page_response(unique_posts("p1", 20), Some("cursor-2"))];
        responses.extend(rate_limited_responses(4));
        let (base_url, requests) = spawn_mock_http_server_with_capture(responses).await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
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

    /// M2-T2 测试 6(F-001 语义边界):第 1 页即 4×429(零进展)
    /// → Err(RateLimited),不包装成 PartialFailure;请求数 4。
    #[tokio::test]
    async fn fetch_zero_progress_rate_limit_is_err() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(rate_limited_responses(4)).await;

        let adapter = fast_adapter(base_url);
        let result = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await;

        assert!(
            matches!(result, Err(GatewayError::RateLimited { .. })),
            "zero-progress rate limit must be Err(RateLimited), got {result:?}"
        );
        assert_eq!(requests.lock().unwrap().len(), 4);
    }

    /// M2-T2 测试 7(回归):既有 `fetch_by_keyword`(无 outcome)对同 mock 3 页
    /// (20+20+10)仍返回 50 条 Vec(override 不破坏既有方法)。
    /// 允许先绿 + AG-006 手工金丝雀程序标注(DR-12):临时让 `fetch_by_keyword`
    /// 返回截断 Vec(如 truncate(10))→ 本测试须红;只动生产代码、RED 输出留存后还原
    /// (归实现/收尾上下文执行)。
    #[tokio::test]
    async fn default_fetch_by_keyword_still_returns_contents() {
        let base_url = spawn_mock_http_server(vec![
            page_response(unique_posts("p1", 20), Some("cursor-2")),
            page_response(unique_posts("p2", 20), Some("cursor-3")),
            page_response(unique_posts("p3", 10), None),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let posts = adapter
            .fetch_by_keyword(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        assert_eq!(posts.len(), 50);
    }

    /// 形状载体:每页 = (条目 id 列表, next_cursor);facebook mock 与 PaginationLoop
    /// 喂入完全相同的形状(M2-T2 测试 8 对齐契约用)。
    fn pagination_loop_shortfall(
        pages: &[(Vec<String>, Option<String>)],
        max_count: usize,
    ) -> Option<FetchShortfall> {
        let mut lp = PaginationLoop::new(max_count);
        for (item_ids, next_cursor) in pages {
            let out = lp.accept_page(item_ids, next_cursor.clone());
            if let PageDecision::Stop(reason) = out.decision {
                return lp.shortfall_for(&reason);
            }
        }
        panic!("shape did not terminate PaginationLoop (test shape must terminate)");
    }

    fn id_list(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}-{i}")).collect()
    }

    async fn facebook_outcome_for_shape(
        shape: &[(Vec<String>, Option<String>)],
        count: u32,
    ) -> GatewayResult<FetchOutcome> {
        let responses = shape
            .iter()
            .map(|(ids, cursor)| {
                page_response(
                    ids.iter().map(|id| test_post(id, "shape-fixture")).collect(),
                    cursor.as_deref(),
                )
            })
            .collect();
        let base_url = spawn_mock_http_server(responses).await;
        let adapter = fast_adapter(base_url);
        adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(count))
            .await
    }

    /// M2-T2 测试 8(对齐契约,D-01 强制项;对齐域声明 DR-17a:覆盖无 date-filter
    /// 子空间的五形状——达量 / cursor 缺失 / cursor 环 / 空页上限 / 重复内容页)。
    /// facebook 实跑 outcome.shortfall 与同形状喂 PaginationLoop 的 shortfall_for
    /// 逐一相等;date-filter 子空间的对齐覆盖由测试 9/12 承担。
    #[tokio::test]
    async fn shortfall_matches_pagination_loop_semantics() {
        let shapes: Vec<(&str, Vec<(Vec<String>, Option<String>)>)> = vec![
            (
                "reached-max",
                vec![
                    (id_list("a", 20), Some("c1".to_string())),
                    (id_list("b", 20), Some("c2".to_string())),
                    (id_list("c", 10), Some("c3".to_string())),
                ],
            ),
            (
                "cursor-missing",
                vec![
                    (id_list("d", 20), Some("c1".to_string())),
                    (id_list("e", 10), None),
                ],
            ),
            (
                "cursor-loop",
                vec![
                    (id_list("f", 20), Some("c-loop".to_string())),
                    (id_list("g", 10), Some("c-loop".to_string())),
                ],
            ),
            (
                "empty-page-limit",
                vec![
                    (Vec::new(), Some("e1".to_string())),
                    (Vec::new(), Some("e2".to_string())),
                    (Vec::new(), Some("e3".to_string())),
                ],
            ),
            (
                "repeated-content-pages",
                vec![
                    (id_list("h", 20), Some("r1".to_string())),
                    (id_list("h", 20), Some("r2".to_string())),
                    (id_list("h", 20), Some("r3".to_string())),
                    (id_list("h", 20), Some("r4".to_string())),
                ],
            ),
        ];

        for (label, shape) in shapes {
            let expected = pagination_loop_shortfall(&shape, 50);
            let outcome = facebook_outcome_for_shape(&shape, 50)
                .await
                .unwrap_or_else(|err| {
                    panic!("facebook outcome for shape {label} should be Ok, got Err({err:?})")
                });
            assert_eq!(
                outcome.shortfall, expected,
                "shape {label}: facebook shortfall must match PaginationLoop::shortfall_for"
            );
        }
    }

    /// M2-T2 测试 9(DR-01 M2 侧 / D1 构造不变量):date-filter 滤空第 1 页全部 20 条
    /// + 第 2 页 4×429 → 零可交付进展(过滤后为空)的失败一律走 Err,不得包成 PartialFailure。
    #[tokio::test]
    async fn date_filter_empty_delivery_with_429_is_err() {
        let out_of_range = FacebookAdapter::parse_date("2026-01-20", false).unwrap();
        let mut responses = vec![page_response(
            (0..20)
                .map(|i| {
                    test_post_with_timestamp(&format!("dated-{i}"), "out-of-range", out_of_range)
                })
                .collect(),
            Some("cursor-2"),
        )];
        responses.extend(rate_limited_responses(4));
        let (base_url, requests) = spawn_mock_http_server_with_capture(responses).await;

        let adapter = fast_adapter(base_url);
        let options = keyword_search_options(50)
            .with_extra_value(extra_keys::START_DATE, json!("2026-01-10"))
            .with_extra_value(extra_keys::END_DATE, json!("2026-01-12"));
        let result = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &options)
            .await;

        assert!(
            matches!(result, Err(GatewayError::RateLimited { .. })),
            "zero filtered delivery + 429 must be Err(RateLimited), got {result:?}"
        );
        assert_eq!(requests.lock().unwrap().len(), 5);
    }

    /// M2-T2 测试 10(DR-10 M2 侧):第 1 页 20 条 + 第 2 页 HTTP 500 → 整体 Err
    /// (PartialFailure 触发集冻结 = 仅 GatewayError::RateLimited;其余错误即使有进展也整体 Err)。
    /// 允许先绿 + AG-006 金丝雀标注:RED 基线下默认方法包装的既有循环已具备该行为;
    /// 金丝雀 = 临时把硬错误收敛为 PartialFailure,须使本测试红,还原复绿,输出留存
    /// (归实现/收尾上下文执行)。
    #[tokio::test]
    async fn hard_error_with_progress_is_err() {
        let base_url = spawn_mock_http_server(vec![
            page_response(unique_posts("p1", 20), Some("cursor-2")),
            MockHttpResponse::json(500, json!({"message": "internal error"})),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let result = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await;

        assert!(
            result.is_err(),
            "hard error (HTTP 500) with progress must be overall Err, got {result:?}"
        );
    }

    /// M2-T2 测试 11(D-15 / DR-03 fb 对齐):第 1 页 20 条,随后 3 连页返回与第 1 页
    /// 相同内容(cursor 各异)→ 终止、Some(Exhausted)、不发第 5 请求(requests.len() == 4;
    /// 空页计数按「本页新增(去重后)数 == 0」递增,D-15 生产行改动的 killing 测试)。
    #[tokio::test]
    async fn repeated_content_pages_stop_via_empty_limit() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            page_response(unique_posts("rep", 20), Some("cursor-2")),
            page_response(unique_posts("rep", 20), Some("cursor-3")),
            page_response(unique_posts("rep", 20), Some("cursor-4")),
            page_response(unique_posts("rep", 20), Some("cursor-5")),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 20);
        assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
        assert_eq!(
            requests.lock().unwrap().len(),
            4,
            "must stop after 3rd repeated-content page; no 5th request"
        );
    }

    /// M2-T2 测试 12(DR-17b):达量/枯竭判定按过滤后交付计数。
    /// 形状 A:raw 50 / 过滤后 30 / cursor 链尽 / count=50 → 30 条 + Some(Exhausted);
    /// 形状 B:过滤后达 count → None。
    #[tokio::test]
    async fn date_filter_shortfall_uses_filtered_count() {
        let in_range = FacebookAdapter::parse_date("2026-01-11", false).unwrap();
        let out_of_range = FacebookAdapter::parse_date("2026-01-20", false).unwrap();
        let dated_posts = |prefix: &str, n_in: usize, n_out: usize| -> Vec<Value> {
            let mut posts: Vec<Value> = (0..n_in)
                .map(|i| {
                    test_post_with_timestamp(&format!("{prefix}-in-{i}"), "in-range", in_range)
                })
                .collect();
            posts.extend((0..n_out).map(|i| {
                test_post_with_timestamp(&format!("{prefix}-out-{i}"), "out-of-range", out_of_range)
            }));
            posts
        };

        // 形状 A:raw 50(3 页 20+20+10,各页含 10/10/10 条在范围内)、过滤后 30、cursor 链尽。
        let base_url = spawn_mock_http_server(vec![
            page_response(dated_posts("a1", 10, 10), Some("cursor-2")),
            page_response(dated_posts("a2", 10, 10), Some("cursor-3")),
            page_response(dated_posts("a3", 10, 0), None),
        ])
        .await;
        let adapter = fast_adapter(base_url);
        let options = keyword_search_options(50)
            .with_extra_value(extra_keys::START_DATE, json!("2026-01-10"))
            .with_extra_value(extra_keys::END_DATE, json!("2026-01-12"));
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &options)
            .await
            .unwrap();
        assert_eq!(outcome.contents.len(), 30, "shape A: delivery = filtered count");
        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "shape A: filtered 30 < count=50 with exhausted cursor chain"
        );

        // 形状 B:单页 raw 30(20 条在范围内)、count=20 → 过滤后达 count → None。
        let base_url = spawn_mock_http_server(vec![page_response(
            dated_posts("b1", 20, 10),
            Some("cursor-b2"),
        )])
        .await;
        let adapter = fast_adapter(base_url);
        let options = keyword_search_options(20)
            .with_extra_value(extra_keys::START_DATE, json!("2026-01-10"))
            .with_extra_value(extra_keys::END_DATE, json!("2026-01-12"));
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &options)
            .await
            .unwrap();
        assert_eq!(outcome.contents.len(), 20, "shape B: filtered delivery reaches count");
        assert!(
            outcome.shortfall.is_none(),
            "shape B: filtered delivery reached count, got {:?}",
            outcome.shortfall
        );
    }

    /// M2-T2 测试 13(DR-17c):search_type=pages(两级 candidates 循环),
    /// candidate1 内页枯竭(仅 10 条)、candidate2 补足达 count → shortfall == None
    /// (内层 fetch_page_posts_paginated 的枯竭不得外泄为整体 shortfall)。
    /// 允许先绿 + AG-006 金丝雀标注:RED 基线下默认方法包装的既有循环已具备该行为;
    /// 金丝雀 = 临时让内层枯竭外泄为 Some(Exhausted),须使本测试红,还原复绿,输出留存
    /// (归实现/收尾上下文执行)。
    #[tokio::test]
    async fn candidates_inner_exhaustion_not_leaked() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                200,
                json!({
                    "results": [
                        {"facebook_id": "fb-page-1", "name": "Candidate One"},
                        {"facebook_id": "fb-page-2", "name": "Candidate Two"}
                    ],
                    "cursor": null
                }),
            ),
            page_response(unique_posts("cand1", 10), None),
            page_response(unique_posts("cand2", 20), None),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let options = SearchOptions::new("museum")
            .with_platform("facebook")
            .with_count(30)
            .with_extra_value(extra_keys::MODE, json!(mode::KEYWORD))
            .with_extra_value(extra_keys::SEARCH_TYPE, json!("pages"));
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&KeywordType::Search("museum".to_string()), &options)
            .await
            .unwrap();

        assert_eq!(outcome.contents.len(), 30);
        assert!(
            outcome.shortfall.is_none(),
            "candidate1 inner exhaustion must not leak into overall shortfall, got {:?}",
            outcome.shortfall
        );

        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert!(requests[1].contains("page_id=fb-page-1"));
        assert!(requests[2].contains("page_id=fb-page-2"));
    }

    // ──────────────────────────────────────────────────────────────────
    // M2-T3 测试载荷(m2-facebook-p0.md §4 M2-T3,测试 1~4)
    // 专注「请求级行为」——请求次数、cursor 转发停止、空页计数复位。
    // 允许先绿(AG-006);AG-012 预检 = 手工金丝雀(DR-12,见计划 §4 M2-T3)。
    // ──────────────────────────────────────────────────────────────────

    /// M2-T3 测试 1 (F-004 请求级):第 2 页返回与第 1 页相同 cursor → 适配器不发
    /// 第 3 个相同 cursor 请求(requests.len() == 2);收集首两页内容。
    ///
    /// 允许先绿 + AG-006 金丝雀标注(DR-12):
    ///   T3.1 金丝雀 = 临时注释 seen-cursor break(facebook.rs 581-583)→ 须红;
    ///   还原复绿,输出留存(归实现/收尾上下文执行)。
    #[tokio::test]
    async fn cursor_loop_stops_and_does_not_refetch() {
        // 第 1 页:20 条 + cursor "cursor-loop"
        // 第 2 页:10 条 + 同一 cursor "cursor-loop"(触发 seen-cursor 防环)
        // 若防环失效,适配器将无限发送第 3 个请求;mock 仅准备 2 个响应。
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            page_response(unique_posts("p1", 20), Some("cursor-loop")),
            page_response(unique_posts("p2", 10), Some("cursor-loop")),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        // 请求级断言:seen-cursor 出口在第 2 页后终止,不发第 3 个请求。
        assert_eq!(
            requests.lock().unwrap().len(),
            2,
            "cursor_loop: seen-cursor must stop after 2nd request, got {}",
            requests.lock().unwrap().len()
        );
        // 语义断言:cursor 环 → Exhausted shortfall(与 M2-T2 测试 3 同语义,从请求计数角度验证)。
        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "cursor_loop: shortfall must be Exhausted, got {:?}",
            outcome.shortfall
        );
    }

    /// M2-T3 测试 2 (F-005 请求级):连续 3 空页(cursor 各异 c2/c3/c4)→ 适配器在第 3
    /// 空页后停止(requests.len() == 3,不发第 4 请求);总收集 0 条;shortfall == Exhausted。
    ///
    /// 允许先绿 + AG-006 金丝雀标注(DR-12):
    ///   T3.2/3.3 金丝雀 = 临时改 MAX_EMPTY_CURSOR_HOPS 判定(3→999)→ 须红;
    ///   还原复绿,输出留存(归实现/收尾上下文执行)。
    #[tokio::test]
    async fn empty_page_streak_stops_at_three() {
        // 3 连空页,cursor 各异(c2/c3/c4)。mock 只注册 3 个响应,若发第 4 请求即超出。
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            page_response(Vec::new(), Some("c2")),
            page_response(Vec::new(), Some("c3")),
            page_response(Vec::new(), Some("c4")),
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        // 请求级断言:MAX_EMPTY_CURSOR_HOPS=3 → 恰好发 3 个请求后停止。
        assert_eq!(
            requests.lock().unwrap().len(),
            3,
            "empty streak: must stop after 3rd empty-page request (MAX_EMPTY_CURSOR_HOPS=3)"
        );
        // 收集断言:3 连空页,无帖子交付。
        assert_eq!(
            outcome.contents.len(),
            0,
            "empty streak: 0 posts collected from 3 empty pages"
        );
        // 语义断言:空页上限出口 → Exhausted。
        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "empty streak: shortfall must be Exhausted, got {:?}",
            outcome.shortfall
        );
    }

    /// M2-T3 测试 3 (F-005 复位,D-15):空页 → 有新增页(20 条)→ 空页 序列;count=50。
    /// 计数在本页有新增(去重后)时复位(D-15 语义),不在第 2 空页(共经历 1+1=2 次空跳)误停。
    ///
    /// 序列(3 个请求):
    ///   req 1: empty(cursor c2) → empty_hops=1
    ///   req 2: 20 posts(cursor c3) → empty_hops=0(复位)
    ///   req 3: empty(cursor=None) → empty_hops=1,cursor=None → UpstreamExhausted(终止)
    ///
    /// 允许先绿 + AG-006 金丝雀标注(DR-12):
    ///   T3.3 金丝雀:D-15 reset 语义已由 M2-T2 测试 11(`repeated_content_pages_stop_via_empty_limit`)
    ///   独立证明并留存金丝雀证据(任何修改 empty_hops 复位行的变异均使该测试红)。
    ///   本 T3.3 专注于「空页→有新增页→空页不误停」的请求级断言(requests==3 且收集到 20 条),
    ///   引用 T2-11 的金丝雀证据而不重复相同对象的金丝雀程序(参见 DR-12 约定:
    ///   若某金丝雀与 T2 已留存证据完全同对象,引用之并注明,不重复)。
    #[tokio::test]
    async fn empty_streak_resets_on_nonempty_page() {
        // 序列:空页 c2 → 有新增页 20 条 c3 → 空页(cursor=None 终止)
        // 空页计数在 req 2(有新增)后复位为 0;req 3(第 2 空页)时 empty_hops=1,不触发 EmptyPageLimit。
        // 3 个请求全部处理,收集 20 条。
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            page_response(Vec::new(), Some("c2")),              // req 1:空页,empty_hops → 1
            page_response(unique_posts("mid", 20), Some("c3")), // req 2:有新增,empty_hops → 0(复位)
            page_response(Vec::new(), None),                    // req 3:空页,empty_hops → 1,cursor=None → 终止
        ])
        .await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        // 请求级断言:3 个请求全发(复位有效,不在 req 3 的第 2 空页误停于 EmptyPageLimit)。
        assert_eq!(
            requests.lock().unwrap().len(),
            3,
            "empty_streak_reset: must make 3 requests (empty→nonempty→empty+null-cursor), got {}",
            requests.lock().unwrap().len()
        );
        // 收集断言:req 2 的 20 条进入交付集。
        assert_eq!(
            outcome.contents.len(),
            20,
            "empty_streak_reset: should collect 20 posts from the nonempty middle page"
        );
        // 语义断言:cursor 链断 → Exhausted。
        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "empty_streak_reset: shortfall should be Exhausted (cursor null), got {:?}",
            outcome.shortfall
        );
    }

    /// M2-T3 测试 4 (F-006 请求级):第 1 页 20 条 + 第 2 页 4×429(retry-after: 0)
    /// → requests.len() == 1 + 4 == 5;返回首页 20 条(重试耗尽后进 partial,不无限重试)。
    ///
    /// 允许先绿 + AG-006 金丝雀标注(DR-12):
    ///   T3.4 金丝雀 = 临时移除 RateLimited-partial 分支(facebook.rs 586-593)→ 须红;
    ///   还原复绿,输出留存(归实现/收尾上下文执行)。
    #[tokio::test]
    async fn rate_limited_after_progress_stops_with_collected() {
        // 第 1 页:20 条 + cursor "cursor-2"(有进展)
        // 第 2~5 请求:4×429(retry-after: 0;适配器对 429 自动重试 ×3 后第 4 次仍 429 → Partial)
        let mut responses = vec![page_response(unique_posts("p1", 20), Some("cursor-2"))];
        responses.extend(rate_limited_responses(4));
        let (base_url, requests) = spawn_mock_http_server_with_capture(responses).await;

        let adapter = fast_adapter(base_url);
        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .unwrap();

        // 请求级断言:1 次正常请求 + 4 次 429 重试 = 5 次请求。
        assert_eq!(
            requests.lock().unwrap().len(),
            5,
            "rate_limited_after_progress: expected 1+4=5 requests (1 success + 4 rate-limit retries), got {}",
            requests.lock().unwrap().len()
        );
        // 收集断言:首页 20 条进入 partial 交付。
        assert_eq!(
            outcome.contents.len(),
            20,
            "rate_limited_after_progress: should deliver first page's 20 posts"
        );
        // shortfall 断言:有进展的 429 → PartialFailure。
        match &outcome.shortfall {
            Some(FetchShortfall::PartialFailure { message }) => {
                assert!(
                    message.to_lowercase().contains("rate"),
                    "rate_limited_after_progress: PartialFailure message should mention rate, got {message:?}"
                );
            }
            other => panic!(
                "rate_limited_after_progress: expected Some(PartialFailure {{ .. }}), got {other:?}"
            ),
        }
    }
}
