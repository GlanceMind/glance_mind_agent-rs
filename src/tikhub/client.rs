//! TikHub HTTP Client Implementation
//!
//! Provides robust TikHub API access with:
//! - Automatic retry with exponential backoff
//! - Proper error classification based on HTTP status codes
//! - Partial data recovery on pagination failures
//!
//! Supports multiple platforms:
//! - TikTok
//! - Instagram
//! - Reddit
//! - Twitter

use reqwest::{header, Client};
use std::future::Future;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use super::error::{PartialFetchResult, TikHubError, TikHubRetryConfig};
use super::instagram_types::*;
use super::reddit_types::*;
use super::twitter_types::*;
use super::types::*;

/// TikHub API Client
///
/// Provides typed access to TikHub API endpoints for TikTok data with
/// automatic retry and robust error handling.
///
/// # Example
///
/// ```no_run
/// use glance_mind_agent_rs::tikhub::{TikHubClient, SearchParams, CommentParams};
///
/// #[tokio::main]
/// async fn main() {
///     let client = TikHubClient::from_env().unwrap();
///     
///     // Search videos with retry
///     let params = SearchParams::new("travel").with_region("US").with_count(10);
///     let response = client.search_videos_with_retry(&params).await.unwrap();
///     
///     // Fetch comments with partial recovery
///     let result = client.fetch_all_comments_safe("7327061675382260482", 300).await;
///     println!("Fetched {} comments (partial: {})", result.data.len(), result.is_partial);
/// }
/// ```
pub struct TikHubClient {
    client: Client,
    base_url: String,
    api_key: String,
    retry_config: TikHubRetryConfig,
}

impl TikHubClient {
    /// Create a new TikHub client with explicit configuration
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, TikHubError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(TikHubError::MissingApiKey);
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(TikHubError::from)?;

        Ok(Self {
            client,
            base_url: base_url.into(),
            api_key,
            retry_config: TikHubRetryConfig::default(),
        })
    }

    /// Create a new TikHub client with custom retry configuration
    pub fn with_retry_config(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        retry_config: TikHubRetryConfig,
    ) -> Result<Self, TikHubError> {
        let mut client = Self::new(api_key, base_url)?;
        client.retry_config = retry_config;
        Ok(client)
    }

    /// Create a TikHub client from environment variables
    ///
    /// Expects:
    /// - TIKHUB_API_KEY
    /// - TIKHUB_BASE_URL (optional, defaults to https://api.tikhub.io)
    pub fn from_env() -> Result<Self, TikHubError> {
        let api_key = std::env::var("TIKHUB_API_KEY").map_err(|_| TikHubError::MissingApiKey)?;

        let base_url = std::env::var("TIKHUB_BASE_URL")
            .unwrap_or_else(|_| "https://api.tikhub.io".to_string());

        Self::new(api_key, base_url)
    }

    /// Build authorization header
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    // ============================================================
    // Core Request Methods (with HTTP status handling)
    // ============================================================

    /// Make a GET request and handle HTTP status codes properly
    async fn get_with_status_handling<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<T, TikHubError> {
        let response = self
            .client
            .get(url)
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_TYPE, "application/json")
            .query(query)
            .send()
            .await?;

        let status = response.status();

        // Handle HTTP-level errors first
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(
                status = %status.as_u16(),
                body = %body,
                "TikHub HTTP error"
            );
            return Err(TikHubError::from_status(status.as_u16(), body));
        }

        // Get response body as text first for better error diagnostics
        let body = response.text().await?;

        // Parse JSON response with detailed error logging
        match serde_json::from_str::<T>(&body) {
            Ok(data) => Ok(data),
            Err(e) => {
                // Log parse error with body snippet for debugging
                let body_preview = if body.len() > 500 {
                    format!("{}...(truncated, total {} bytes)", &body[..500], body.len())
                } else {
                    body.clone()
                };
                warn!(
                    error = %e,
                    body_preview = %body_preview,
                    "TikHub JSON parse error"
                );
                Err(TikHubError::ParseError(format!(
                    "{} (body preview: {})",
                    e,
                    if body.len() > 200 {
                        &body[..200]
                    } else {
                        &body
                    }
                )))
            }
        }
    }

    /// Execute a request with automatic retry based on error type
    async fn with_retry<T, F, Fut>(&self, operation: &str, f: F) -> Result<T, TikHubError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, TikHubError>>,
    {
        let mut last_error: Option<TikHubError> = None;

        for attempt in 0..=self.retry_config.max_retries {
            match f().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!(
                            operation = %operation,
                            attempt = attempt,
                            "TikHub request succeeded after retry"
                        );
                    }
                    return Ok(result);
                }
                Err(e) => {
                    // Fatal errors - return immediately
                    if e.is_fatal() {
                        error!(
                            operation = %operation,
                            error = %e,
                            "TikHub fatal error, stopping immediately"
                        );
                        return Err(e);
                    }

                    // Check if we should retry
                    let max_retries = self.retry_config.max_retries_for_error(&e);
                    if !e.is_retryable() || attempt >= max_retries {
                        warn!(
                            operation = %operation,
                            attempt = attempt,
                            max_retries = max_retries,
                            error = %e,
                            "TikHub request failed, not retrying"
                        );
                        return Err(e);
                    }

                    // Calculate delay and wait
                    let delay_ms = self.retry_config.delay_for_attempt(attempt, &e);

                    warn!(
                        operation = %operation,
                        attempt = attempt + 1,
                        max_attempts = max_retries + 1,
                        delay_ms = delay_ms,
                        error = %e,
                        "TikHub request failed, retrying..."
                    );

                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| TikHubError::NetworkError {
            message: "Unknown error after retries".to_string(),
        }))
    }

    // ============================================================
    // Search Videos
    // ============================================================

    /// Search videos by keyword (basic, no retry)
    ///
    /// Endpoint: `/api/v1/tiktok/app/v3/fetch_video_search_result`
    pub async fn search_videos(
        &self,
        params: &SearchParams,
    ) -> Result<SearchResponse, TikHubError> {
        let url = format!(
            "{}/api/v1/tiktok/app/v3/fetch_video_search_result",
            self.base_url
        );

        // Handle "GLOBAL" region - TikHub expects ISO country codes
        let region = if params.region.to_uppercase() == "GLOBAL" {
            "US"
        } else {
            &params.region
        };

        info!(
            keyword = %params.keyword,
            count = params.count,
            region = %region,
            "TikHub: Searching videos"
        );

        let data: SearchResponse = self
            .get_with_status_handling(
                &url,
                &[
                    ("keyword", params.keyword.as_str()),
                    ("offset", &params.offset.to_string()),
                    ("count", &params.count.to_string()),
                    ("region", region),
                    ("sort_type", &params.sort_type.to_string()),
                    ("publish_time", &params.publish_time.to_string()),
                ],
            )
            .await?;

        // Check API-level response code
        if data.code != 200 {
            warn!(
                code = data.code,
                message = %data.message,
                "TikHub API error in response body"
            );
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let video_count = data
            .data
            .as_ref()
            .and_then(|d| d.search_item_list.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(
            keyword = %params.keyword,
            video_count = video_count,
            "TikHub: Search completed"
        );

        Ok(data)
    }

    /// Search videos with automatic retry
    pub async fn search_videos_with_retry(
        &self,
        params: &SearchParams,
    ) -> Result<SearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_videos", || {
            let p = params.clone();
            async move { self.search_videos(&p).await }
        })
        .await
    }

    // ============================================================
    // Fetch Comments
    // ============================================================

    /// Fetch comments for a video (single page, no retry)
    ///
    /// Endpoint: `/api/v1/tiktok/web/fetch_post_comment`
    pub async fn fetch_comments(
        &self,
        aweme_id: &str,
        params: &CommentParams,
    ) -> Result<CommentsResponse, TikHubError> {
        let url = format!("{}/api/v1/tiktok/web/fetch_post_comment", self.base_url);

        debug!(
            aweme_id = %aweme_id,
            cursor = %params.cursor,
            count = params.count,
            "TikHub: Fetching comments"
        );

        let data: CommentsResponse = self
            .get_with_status_handling(
                &url,
                &[
                    ("aweme_id", aweme_id),
                    ("cursor", &params.cursor),
                    ("count", &params.count.to_string()),
                    ("current_region", ""),
                ],
            )
            .await?;

        // Check API-level response code
        if data.code != 200 {
            warn!(
                code = data.code,
                message = %data.message,
                aweme_id = %aweme_id,
                "TikHub API error in response body"
            );
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let comment_count = data
            .data
            .as_ref()
            .and_then(|d| d.comments.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        debug!(
            aweme_id = %aweme_id,
            comment_count = comment_count,
            "TikHub: Comments fetched"
        );

        Ok(data)
    }

    /// Fetch comments for a single page with retry
    pub async fn fetch_comments_with_retry(
        &self,
        aweme_id: &str,
        params: &CommentParams,
    ) -> Result<CommentsResponse, TikHubError> {
        let aweme_id = aweme_id.to_string();
        let params = params.clone();
        self.with_retry("fetch_comments", || {
            let id = aweme_id.clone();
            let p = params.clone();
            async move { self.fetch_comments(&id, &p).await }
        })
        .await
    }

    /// Fetch all comments for a video with automatic pagination (basic, no retry)
    ///
    /// This method will automatically paginate through all available comments
    /// until `max_count` is reached or no more comments are available.
    pub async fn fetch_all_comments(
        &self,
        aweme_id: &str,
        max_count: u32,
    ) -> Result<Vec<TikTokComment>, TikHubError> {
        let result = self.fetch_all_comments_safe(aweme_id, max_count).await;

        if result.is_partial {
            // If we have partial data, return it. Otherwise return the error.
            if result.has_data() {
                warn!(
                    aweme_id = %aweme_id,
                    fetched = result.data.len(),
                    "TikHub: Returning partial comments due to error"
                );
                Ok(result.data)
            } else if let Some(e) = result.error {
                Err(e)
            } else {
                Ok(result.data)
            }
        } else {
            Ok(result.data)
        }
    }

    /// Fetch all comments with partial data recovery
    ///
    /// This method provides more control by returning a `PartialFetchResult`
    /// that indicates whether the fetch was complete or partial.
    pub async fn fetch_all_comments_safe(
        &self,
        aweme_id: &str,
        max_count: u32,
    ) -> PartialFetchResult<TikTokComment> {
        let mut all_comments = Vec::new();
        let mut cursor = "0".to_string();
        let mut page = 0;

        info!(
            aweme_id = %aweme_id,
            max_count = max_count,
            "TikHub: Starting to fetch all comments"
        );

        loop {
            page += 1;
            let remaining = max_count.saturating_sub(all_comments.len() as u32);
            if remaining == 0 {
                break;
            }

            let params = CommentParams::new()
                .with_cursor(&cursor)
                .with_count(remaining.min(100));

            // Use retry for each page
            match self.fetch_comments_with_retry(aweme_id, &params).await {
                Ok(response) => {
                    let data = match response.data {
                        Some(d) => d,
                        None => break,
                    };

                    let comments = data.comments.unwrap_or_default();
                    if comments.is_empty() {
                        info!(
                            aweme_id = %aweme_id,
                            "TikHub: No more comments, stopping pagination"
                        );
                        break;
                    }

                    all_comments.extend(comments);

                    // Check for more pages
                    if data.has_more != Some(1) {
                        break;
                    }

                    cursor = data
                        .cursor
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "0".to_string());

                    info!(
                        aweme_id = %aweme_id,
                        page = page,
                        total = all_comments.len(),
                        max_count = max_count,
                        has_more = data.has_more.unwrap_or(0) == 1,
                        "TikHub: Page fetched"
                    );

                    // Small delay to avoid rate limiting
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Err(e) => {
                    // Fatal errors - return immediately
                    if e.is_fatal() {
                        error!(
                            aweme_id = %aweme_id,
                            error = %e,
                            "TikHub: Fatal error during pagination"
                        );
                        if all_comments.is_empty() {
                            return PartialFetchResult {
                                data: vec![],
                                is_partial: true,
                                error: Some(e),
                            };
                        }
                        return PartialFetchResult::partial(all_comments, e);
                    }

                    // Non-fatal errors with data - return partial result
                    if !all_comments.is_empty() {
                        warn!(
                            aweme_id = %aweme_id,
                            fetched = all_comments.len(),
                            error = %e,
                            "TikHub: Returning partial data due to error"
                        );
                        return PartialFetchResult::partial(all_comments, e);
                    }

                    // No data yet - return error
                    warn!(
                        aweme_id = %aweme_id,
                        error = %e,
                        "TikHub: Failed to fetch any comments"
                    );
                    return PartialFetchResult {
                        data: vec![],
                        is_partial: true,
                        error: Some(e),
                    };
                }
            }
        }

        info!(
            aweme_id = %aweme_id,
            total = all_comments.len(),
            pages = page,
            "TikHub: Completed fetching all comments"
        );

        PartialFetchResult::complete(all_comments)
    }

    // ============================================================
    // Fetch User Videos
    // ============================================================

    /// Fetch user's videos (basic, no retry)
    ///
    /// Endpoint: `/api/v1/tiktok/app/v3/fetch_user_post_videos`
    pub async fn fetch_user_videos(
        &self,
        params: &UserVideoParams,
    ) -> Result<UserVideosResponse, TikHubError> {
        let url = format!(
            "{}/api/v1/tiktok/app/v3/fetch_user_post_videos",
            self.base_url
        );

        let sec_user_id = params.sec_user_id.as_deref().unwrap_or("");
        let unique_id = params.unique_id.as_deref().unwrap_or("");

        if sec_user_id.is_empty() && unique_id.is_empty() {
            return Err(TikHubError::InvalidParam(
                "Either sec_user_id or unique_id is required".to_string(),
            ));
        }

        let user_info = if !unique_id.is_empty() {
            format!("@{}", unique_id)
        } else {
            format!("sec_uid:{}", sec_user_id)
        };

        info!(
            user = %user_info,
            count = params.count,
            "TikHub: Fetching user videos"
        );

        let data: UserVideosResponse = self
            .get_with_status_handling(
                &url,
                &[
                    ("sec_user_id", sec_user_id),
                    ("unique_id", unique_id),
                    ("max_cursor", &params.max_cursor.to_string()),
                    ("count", &params.count.to_string()),
                    ("sort_type", &params.sort_type.to_string()),
                ],
            )
            .await?;

        // Check API-level response code
        if data.code != 200 {
            warn!(
                code = data.code,
                message = %data.message,
                "TikHub API error in response body"
            );
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let video_count = data
            .data
            .as_ref()
            .and_then(|d| d.aweme_list.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(
            user = %user_info,
            video_count = video_count,
            "TikHub: User videos fetched"
        );

        Ok(data)
    }

    /// Fetch user's videos with automatic retry
    pub async fn fetch_user_videos_with_retry(
        &self,
        params: &UserVideoParams,
    ) -> Result<UserVideosResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_user_videos", || {
            let p = params.clone();
            async move { self.fetch_user_videos(&p).await }
        })
        .await
    }

    // ============================================================
    // Instagram API Methods
    // ============================================================

    /// Search Instagram posts by hashtag
    ///
    /// Endpoint: `/api/v1/instagram/v2/fetch_hashtag_posts`
    pub async fn search_hashtag_posts(
        &self,
        params: &HashtagSearchParams,
    ) -> Result<HashtagSearchResponse, TikHubError> {
        let url = format!("{}/api/v1/instagram/v2/fetch_hashtag_posts", self.base_url);

        info!(
            keyword = %params.keyword,
            feed_type = %params.feed_type,
            "Instagram: Searching hashtag posts"
        );

        let mut query: Vec<(&str, &str)> = vec![
            ("keyword", &params.keyword),
            ("feed_type", &params.feed_type),
        ];

        let pagination_token_str;
        if let Some(ref token) = params.pagination_token {
            pagination_token_str = token.clone();
            query.push(("pagination_token", &pagination_token_str));
        }

        let data: HashtagSearchResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(keyword = %params.keyword, post_count = post_count, "Instagram: Hashtag search completed");

        Ok(data)
    }

    /// Search Instagram posts by hashtag with retry
    pub async fn search_hashtag_posts_with_retry(
        &self,
        params: &HashtagSearchParams,
    ) -> Result<HashtagSearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_hashtag_posts", || {
            let p = params.clone();
            async move { self.search_hashtag_posts(&p).await }
        })
        .await
    }

    /// Search Instagram posts and related media by query using V3 general search.
    ///
    /// Endpoint: `/api/v1/instagram/v3/general_search`
    pub async fn search_instagram_general(
        &self,
        query: &str,
    ) -> Result<GeneralSearchResponse, TikHubError> {
        let url = format!("{}/api/v1/instagram/v3/general_search", self.base_url);

        info!(query = %query, "Instagram: Searching via V3 general_search");

        let query_params: Vec<(&str, &str)> = vec![("query", query), ("enable_metadata", "true")];

        let data: GeneralSearchResponse =
            self.get_with_status_handling(&url, &query_params).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram V3 API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = Self::extract_instagram_general_posts(&data).len();

        info!(query = %query, post_count = post_count, "Instagram: V3 general_search completed");

        Ok(data)
    }

    /// Search Instagram posts and related media by query using V3 general search with retry.
    pub async fn search_instagram_general_with_retry(
        &self,
        query: &str,
    ) -> Result<GeneralSearchResponse, TikHubError> {
        let query = query.to_string();
        self.with_retry("search_instagram_general", || {
            let q = query.clone();
            async move { self.search_instagram_general(&q).await }
        })
        .await
    }

    /// Search Instagram posts and related media by keyword using V2 general search.
    ///
    /// Endpoint: `/api/v1/instagram/v2/general_search`
    pub async fn search_instagram_general_v2(
        &self,
        keyword: &str,
    ) -> Result<GeneralSearchV2Response, TikHubError> {
        let url = format!("{}/api/v1/instagram/v2/general_search", self.base_url);
        let keyword = keyword.trim().trim_start_matches('#');

        if keyword.is_empty() {
            return Err(TikHubError::InvalidParam(
                "Instagram V2 general_search keyword is required".to_string(),
            ));
        }

        info!(keyword = %keyword, "Instagram: Searching via V2 general_search");

        let query_params: Vec<(&str, &str)> = vec![("keyword", keyword)];
        let data: GeneralSearchV2Response =
            self.get_with_status_handling(&url, &query_params).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram V2 API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = Self::extract_instagram_general_v2_posts(&data).len();

        info!(keyword = %keyword, post_count = post_count, "Instagram: V2 general_search completed");

        Ok(data)
    }

    /// Search Instagram posts and related media by keyword using V2 general search with retry.
    pub async fn search_instagram_general_v2_with_retry(
        &self,
        keyword: &str,
    ) -> Result<GeneralSearchV2Response, TikHubError> {
        let keyword = keyword.to_string();
        self.with_retry("search_instagram_general_v2", || {
            let q = keyword.clone();
            async move { self.search_instagram_general_v2(&q).await }
        })
        .await
    }

    /// Search Instagram Reels by keyword
    ///
    /// Endpoint: `/api/v1/instagram/v2/search_reels`
    pub async fn search_instagram_reels(
        &self,
        params: &ReelsSearchParams,
    ) -> Result<ReelsSearchResponse, TikHubError> {
        let url = format!("{}/api/v1/instagram/v2/search_reels", self.base_url);

        info!(keyword = %params.keyword, "Instagram: Searching reels");

        let mut query: Vec<(&str, &str)> = vec![("keyword", &params.keyword)];

        let pagination_token_str;
        if let Some(ref token) = params.pagination_token {
            pagination_token_str = token.clone();
            query.push(("pagination_token", &pagination_token_str));
        }

        let data: ReelsSearchResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let reel_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(keyword = %params.keyword, reel_count = reel_count, "Instagram: Reels search completed");

        Ok(data)
    }

    /// Search Instagram Reels with retry
    pub async fn search_instagram_reels_with_retry(
        &self,
        params: &ReelsSearchParams,
    ) -> Result<ReelsSearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_instagram_reels", || {
            let p = params.clone();
            async move { self.search_instagram_reels(&p).await }
        })
        .await
    }

    /// Search Instagram posts by hashtag using Web/APP API (more stable)
    ///
    /// Endpoint: `/api/v1/instagram/web_app/fetch_hashtag_posts_by_keyword`
    ///
    /// This is the recommended API for searching Instagram content.
    /// feed_type can be: "top" (default), "recent", or "clips" (Reels only)
    pub async fn search_hashtag_posts_web(
        &self,
        params: &HashtagSearchParams,
    ) -> Result<HashtagSearchResponse, TikHubError> {
        let url = format!(
            "{}/api/v1/instagram/web_app/fetch_hashtag_posts_by_keyword",
            self.base_url
        );

        info!(
            keyword = %params.keyword,
            feed_type = %params.feed_type,
            "Instagram: Searching hashtag posts (web_app API)"
        );

        let mut query: Vec<(&str, &str)> = vec![
            ("keyword", &params.keyword),
            ("feed_type", &params.feed_type),
        ];

        let pagination_token_str;
        if let Some(ref token) = params.pagination_token {
            pagination_token_str = token.clone();
            query.push(("pagination_token", &pagination_token_str));
        }

        let data: HashtagSearchResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(keyword = %params.keyword, post_count = post_count, "Instagram: Hashtag search completed (web_app API)");

        Ok(data)
    }

    /// Search Instagram posts by hashtag with retry using Web/APP API
    pub async fn search_hashtag_posts_web_with_retry(
        &self,
        params: &HashtagSearchParams,
    ) -> Result<HashtagSearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_hashtag_posts_web", || {
            let p = params.clone();
            async move { self.search_hashtag_posts_web(&p).await }
        })
        .await
    }

    /// Search Instagram posts by hashtag using V1 API
    ///
    /// Endpoint: `/api/v1/instagram/v1/fetch_hashtag_posts`
    /// Note: This is the more reliable V1 API that uses `hashtag` parameter
    pub async fn search_hashtag_posts_v1(
        &self,
        hashtag: &str,
        end_cursor: Option<&str>,
    ) -> Result<HashtagSearchV1Response, TikHubError> {
        let url = format!("{}/api/v1/instagram/v1/fetch_hashtag_posts", self.base_url);

        // Remove # prefix if present
        let hashtag = hashtag.trim_start_matches('#');

        info!(hashtag = %hashtag, "Instagram V1: Searching hashtag posts");

        let mut query: Vec<(&str, &str)> = vec![("hashtag", hashtag)];

        if let Some(cursor) = end_cursor {
            query.push(("end_cursor", cursor));
        }

        let data: HashtagSearchV1Response = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram V1 API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.hashtag.as_ref())
            .and_then(|h| h.edge_hashtag_to_media.as_ref())
            .and_then(|e| e.edges.as_ref())
            .map(|edges| edges.len())
            .unwrap_or(0);

        info!(hashtag = %hashtag, post_count = post_count, "Instagram V1: Hashtag search completed");

        Ok(data)
    }

    /// Search Instagram posts by hashtag using V1 API with retry
    pub async fn search_hashtag_posts_v1_with_retry(
        &self,
        hashtag: &str,
        end_cursor: Option<&str>,
    ) -> Result<HashtagSearchV1Response, TikHubError> {
        let hashtag = hashtag.to_string();
        let cursor = end_cursor.map(|s| s.to_string());
        self.with_retry("search_hashtag_posts_v1", || {
            let h = hashtag.clone();
            let c = cursor.clone();
            async move { self.search_hashtag_posts_v1(&h, c.as_deref()).await }
        })
        .await
    }

    /// Fetch Instagram user's posts
    ///
    /// Endpoint: `/api/v1/instagram/v2/fetch_user_posts`
    pub async fn fetch_instagram_user_posts(
        &self,
        params: &UserPostsParams,
    ) -> Result<UserPostsResponse, TikHubError> {
        let url = format!("{}/api/v1/instagram/v2/fetch_user_posts", self.base_url);

        let user_str = params
            .username
            .as_deref()
            .map(|u| format!("@{}", u))
            .or_else(|| params.user_id.as_ref().map(|id| format!("user_id={}", id)))
            .unwrap_or_else(|| "unknown".to_string());

        info!(user = %user_str, "Instagram: Fetching user posts");

        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(ref username) = params.username {
            query.push(("username", username.clone()));
        } else if let Some(ref user_id) = params.user_id {
            query.push(("user_id", user_id.clone()));
        } else {
            return Err(TikHubError::InvalidParam(
                "Either username or user_id is required".to_string(),
            ));
        }

        if let Some(ref token) = params.pagination_token {
            query.push(("pagination_token", token.clone()));
        }

        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let data: UserPostsResponse = self.get_with_status_handling(&url, &query_refs).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(user = %user_str, post_count = post_count, "Instagram: User posts fetched");

        Ok(data)
    }

    /// Fetch Instagram user's posts with retry
    pub async fn fetch_instagram_user_posts_with_retry(
        &self,
        params: &UserPostsParams,
    ) -> Result<UserPostsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_instagram_user_posts", || {
            let p = params.clone();
            async move { self.fetch_instagram_user_posts(&p).await }
        })
        .await
    }

    /// Fetch Instagram post comments
    ///
    /// Endpoint: `/api/v1/instagram/v2/fetch_post_comments`
    pub async fn fetch_instagram_comments(
        &self,
        params: &InstagramCommentParams,
    ) -> Result<PostCommentsResponse, TikHubError> {
        let url = format!("{}/api/v1/instagram/v2/fetch_post_comments", self.base_url);

        debug!(
            code_or_url = %params.code_or_url,
            sort_by = %params.sort_by,
            "Instagram: Fetching post comments"
        );

        let mut query: Vec<(&str, &str)> = vec![
            ("code_or_url", &params.code_or_url),
            ("sort_by", &params.sort_by),
        ];

        let pagination_token_str;
        if let Some(ref token) = params.pagination_token {
            pagination_token_str = token.clone();
            query.push(("pagination_token", &pagination_token_str));
        }

        let data: PostCommentsResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let comment_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        debug!(comment_count = comment_count, "Instagram: Comments fetched");

        Ok(data)
    }

    /// Fetch Instagram post comments with retry
    pub async fn fetch_instagram_comments_with_retry(
        &self,
        params: &InstagramCommentParams,
    ) -> Result<PostCommentsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_instagram_comments", || {
            let p = params.clone();
            async move { self.fetch_instagram_comments(&p).await }
        })
        .await
    }

    /// Fetch Instagram user's Reels
    ///
    /// Endpoint: `/api/v1/instagram/v2/fetch_user_reels`
    pub async fn fetch_instagram_user_reels(
        &self,
        params: &UserPostsParams,
    ) -> Result<UserReelsResponse, TikHubError> {
        let url = format!("{}/api/v1/instagram/v2/fetch_user_reels", self.base_url);

        let user_str = params
            .username
            .as_deref()
            .map(|u| format!("@{}", u))
            .or_else(|| params.user_id.as_ref().map(|id| format!("user_id={}", id)))
            .unwrap_or_else(|| "unknown".to_string());

        info!(user = %user_str, "Instagram: Fetching user reels");

        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(ref username) = params.username {
            query.push(("username", username.clone()));
        } else if let Some(ref user_id) = params.user_id {
            query.push(("user_id", user_id.clone()));
        } else {
            return Err(TikHubError::InvalidParam(
                "Either username or user_id is required".to_string(),
            ));
        }

        if let Some(ref token) = params.pagination_token {
            query.push(("pagination_token", token.clone()));
        }

        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let data: UserReelsResponse = self.get_with_status_handling(&url, &query_refs).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let reel_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(user = %user_str, reel_count = reel_count, "Instagram: User reels fetched");

        Ok(data)
    }

    /// Fetch Instagram user's Reels with retry
    pub async fn fetch_instagram_user_reels_with_retry(
        &self,
        params: &UserPostsParams,
    ) -> Result<UserReelsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_instagram_user_reels", || {
            let p = params.clone();
            async move { self.fetch_instagram_user_reels(&p).await }
        })
        .await
    }

    /// Fetch Instagram comment replies
    ///
    /// Endpoint: `/api/v1/instagram/v2/fetch_comment_replies`
    pub async fn fetch_instagram_comment_replies(
        &self,
        params: &CommentRepliesParams,
    ) -> Result<CommentRepliesResponse, TikHubError> {
        let url = format!(
            "{}/api/v1/instagram/v2/fetch_comment_replies",
            self.base_url
        );

        debug!(
            code_or_url = %params.code_or_url,
            comment_id = %params.comment_id,
            "Instagram: Fetching comment replies"
        );

        let mut query: Vec<(&str, &str)> = vec![
            ("code_or_url", &params.code_or_url),
            ("comment_id", &params.comment_id),
        ];

        let pagination_token_str;
        if let Some(ref token) = params.pagination_token {
            pagination_token_str = token.clone();
            query.push(("pagination_token", &pagination_token_str));
        }

        let data: CommentRepliesResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Instagram API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let reply_count = data
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        debug!(
            reply_count = reply_count,
            "Instagram: Comment replies fetched"
        );

        Ok(data)
    }

    /// Fetch Instagram comment replies with retry
    pub async fn fetch_instagram_comment_replies_with_retry(
        &self,
        params: &CommentRepliesParams,
    ) -> Result<CommentRepliesResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_instagram_comment_replies", || {
            let p = params.clone();
            async move { self.fetch_instagram_comment_replies(&p).await }
        })
        .await
    }

    // ============================================================
    // Reddit API Methods
    // ============================================================

    /// Search Reddit posts dynamically
    ///
    /// Endpoint: `/api/v1/reddit/app/fetch_dynamic_search`
    pub async fn search_reddit_posts(
        &self,
        params: &RedditSearchParams,
    ) -> Result<RedditSearchResponse, TikHubError> {
        let url = format!("{}/api/v1/reddit/app/fetch_dynamic_search", self.base_url);

        info!(query = %params.query, "Reddit: Searching posts");

        let mut query: Vec<(&str, &str)> = vec![
            ("query", &params.query),
            ("safe_search", &params.safe_search),
            ("allow_nsfw", &params.allow_nsfw),
        ];

        let after_str;
        if let Some(ref after) = params.after {
            after_str = after.clone();
            query.push(("after", &after_str));
        }

        let data: RedditSearchResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Reddit API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = data
            .data
            .as_ref()
            .map(|d| extract_posts_from_search(d).len())
            .unwrap_or(0);

        info!(query = %params.query, post_count = post_count, "Reddit: Search completed");

        Ok(data)
    }

    /// Search Reddit posts with retry
    pub async fn search_reddit_posts_with_retry(
        &self,
        params: &RedditSearchParams,
    ) -> Result<RedditSearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_reddit_posts", || {
            let p = params.clone();
            async move { self.search_reddit_posts(&p).await }
        })
        .await
    }

    /// Fetch Reddit post comments
    ///
    /// Endpoint: `/api/v1/reddit/app/fetch_post_comments`
    pub async fn fetch_reddit_comments(
        &self,
        params: &RedditCommentParams,
    ) -> Result<RedditCommentsResponse, TikHubError> {
        let url = format!("{}/api/v1/reddit/app/fetch_post_comments", self.base_url);

        debug!(
            post_id = %params.post_id,
            sort = %params.sort,
            limit = params.limit,
            "Reddit: Fetching post comments"
        );

        let limit_str = params.limit.to_string();
        let mut query: Vec<(&str, &str)> = vec![
            ("post_id", &params.post_id),
            ("sort", &params.sort),
            ("limit", &limit_str),
        ];

        let after_str;
        if let Some(ref after) = params.after {
            after_str = after.clone();
            query.push(("after", &after_str));
        }

        let data: RedditCommentsResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Reddit API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let comment_count = data
            .data
            .as_ref()
            .and_then(|d| d.post_info_by_id.as_ref())
            .and_then(|p| p.comment_forest.as_ref())
            .and_then(|f| f.trees.as_ref())
            .map(|trees| extract_comments_from_trees(trees, None).len())
            .unwrap_or(0);

        debug!(post_id = %params.post_id, comment_count = comment_count, "Reddit: Comments fetched");

        Ok(data)
    }

    /// Fetch Reddit post comments with retry
    pub async fn fetch_reddit_comments_with_retry(
        &self,
        params: &RedditCommentParams,
    ) -> Result<RedditCommentsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_reddit_comments", || {
            let p = params.clone();
            async move { self.fetch_reddit_comments(&p).await }
        })
        .await
    }

    /// Fetch Reddit user posts
    ///
    /// Endpoint: `/api/v1/reddit/app/fetch_user_posts`
    pub async fn fetch_reddit_user_posts(
        &self,
        params: &RedditUserPostsParams,
    ) -> Result<RedditUserPostsResponse, TikHubError> {
        let url = format!("{}/api/v1/reddit/app/fetch_user_posts", self.base_url);

        info!(username = %params.username, sort = %params.sort, "Reddit: Fetching user posts");

        let mut query: Vec<(&str, &str)> =
            vec![("username", &params.username), ("sort", &params.sort)];

        let after_str;
        if let Some(ref after) = params.after {
            after_str = after.clone();
            query.push(("after", &after_str));
        }

        let data: RedditUserPostsResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Reddit API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let post_count = data
            .data
            .as_ref()
            .and_then(|d| d.post_feed.as_ref())
            .and_then(|f| f.elements.as_ref())
            .and_then(|e| e.edges.as_ref())
            .map(|edges| edges.len())
            .unwrap_or(0);

        info!(username = %params.username, post_count = post_count, "Reddit: User posts fetched");

        Ok(data)
    }

    /// Fetch Reddit user posts with retry
    pub async fn fetch_reddit_user_posts_with_retry(
        &self,
        params: &RedditUserPostsParams,
    ) -> Result<RedditUserPostsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_reddit_user_posts", || {
            let p = params.clone();
            async move { self.fetch_reddit_user_posts(&p).await }
        })
        .await
    }

    /// Fetch Reddit post details in batch
    ///
    /// Endpoint: `/api/v1/reddit/app/fetch_post_details_batch_large`
    /// Note: Maximum 30 post IDs per request
    pub async fn fetch_reddit_post_details_batch(
        &self,
        post_ids: &[String],
    ) -> Result<RedditBatchPostsResponse, TikHubError> {
        let url = format!(
            "{}/api/v1/reddit/app/fetch_post_details_batch_large",
            self.base_url
        );

        if post_ids.is_empty() {
            return Err(TikHubError::InvalidParam(
                "post_ids cannot be empty".to_string(),
            ));
        }
        if post_ids.len() > 30 {
            return Err(TikHubError::InvalidParam(
                "Maximum 30 post IDs allowed".to_string(),
            ));
        }

        // Format: t3_xxx,t3_yyy,t3_zzz
        let ids_str = post_ids
            .iter()
            .map(|id| {
                if id.starts_with("t3_") {
                    id.clone()
                } else {
                    format!("t3_{}", id)
                }
            })
            .collect::<Vec<_>>()
            .join(",");

        info!(
            post_count = post_ids.len(),
            "Reddit: Fetching post details batch"
        );

        let query: Vec<(&str, &str)> = vec![("post_ids", &ids_str)];

        let data: RedditBatchPostsResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Reddit API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        info!("Reddit: Post details batch fetched");

        Ok(data)
    }

    /// Fetch Reddit post details in batch with retry
    pub async fn fetch_reddit_post_details_batch_with_retry(
        &self,
        post_ids: &[String],
    ) -> Result<RedditBatchPostsResponse, TikHubError> {
        let post_ids = post_ids.to_vec();
        self.with_retry("fetch_reddit_post_details_batch", || {
            let ids = post_ids.clone();
            async move { self.fetch_reddit_post_details_batch(&ids).await }
        })
        .await
    }

    // ============================================================
    // Twitter API Methods
    // ============================================================

    /// Search Twitter tweets
    ///
    /// Endpoint: `/api/v1/twitter/web/fetch_search_timeline`
    pub async fn search_twitter_tweets(
        &self,
        params: &TwitterSearchParams,
    ) -> Result<TwitterSearchResponse, TikHubError> {
        let url = format!("{}/api/v1/twitter/web/fetch_search_timeline", self.base_url);

        info!(
            keyword = %params.keyword,
            search_type = %params.search_type,
            "Twitter: Searching tweets"
        );

        let mut query: Vec<(&str, &str)> = vec![
            ("keyword", &params.keyword),
            ("search_type", &params.search_type),
        ];

        let cursor_str;
        if let Some(ref cursor) = params.cursor {
            cursor_str = cursor.clone();
            query.push(("cursor", &cursor_str));
        }

        let data: TwitterSearchResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Twitter API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let tweet_count = data
            .data
            .as_ref()
            .and_then(|d| d.timeline.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(keyword = %params.keyword, tweet_count = tweet_count, "Twitter: Search completed");

        Ok(data)
    }

    /// Search Twitter tweets with retry
    pub async fn search_twitter_tweets_with_retry(
        &self,
        params: &TwitterSearchParams,
    ) -> Result<TwitterSearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_twitter_tweets", || {
            let p = params.clone();
            async move { self.search_twitter_tweets(&p).await }
        })
        .await
    }

    /// Fetch Twitter user tweets
    ///
    /// Endpoint: `/api/v1/twitter/web/fetch_user_post_tweet`
    pub async fn fetch_twitter_user_tweets(
        &self,
        params: &TwitterUserTweetsParams,
    ) -> Result<TwitterUserTweetsResponse, TikHubError> {
        let url = format!("{}/api/v1/twitter/web/fetch_user_post_tweet", self.base_url);

        let user_str = params
            .screen_name
            .as_deref()
            .map(|u| format!("@{}", u))
            .or_else(|| params.rest_id.as_ref().map(|id| format!("rest_id={}", id)))
            .unwrap_or_else(|| "unknown".to_string());

        info!(user = %user_str, "Twitter: Fetching user tweets");

        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(ref rest_id) = params.rest_id {
            query.push(("rest_id", rest_id.clone()));
        } else if let Some(ref screen_name) = params.screen_name {
            query.push(("screen_name", screen_name.clone()));
        } else {
            return Err(TikHubError::InvalidParam(
                "Either rest_id or screen_name is required".to_string(),
            ));
        }

        if let Some(ref cursor) = params.cursor {
            query.push(("cursor", cursor.clone()));
        }

        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let data: TwitterUserTweetsResponse =
            self.get_with_status_handling(&url, &query_refs).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Twitter API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let tweet_count = data
            .data
            .as_ref()
            .and_then(|d| d.timeline.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        info!(user = %user_str, tweet_count = tweet_count, "Twitter: User tweets fetched");

        Ok(data)
    }

    /// Fetch Twitter user tweets with retry
    pub async fn fetch_twitter_user_tweets_with_retry(
        &self,
        params: &TwitterUserTweetsParams,
    ) -> Result<TwitterUserTweetsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_twitter_user_tweets", || {
            let p = params.clone();
            async move { self.fetch_twitter_user_tweets(&p).await }
        })
        .await
    }

    /// Fetch a single Twitter tweet by tweet ID.
    ///
    /// Endpoint: `/api/v1/twitter/web/fetch_tweet_detail`
    pub async fn fetch_twitter_tweet_detail(
        &self,
        tweet_id: &str,
    ) -> Result<TwitterTweetDetailResponse, TikHubError> {
        let url = format!("{}/api/v1/twitter/web/fetch_tweet_detail", self.base_url);

        info!(tweet_id = %tweet_id, "Twitter: Fetching tweet detail");

        let data: TwitterTweetDetailResponse = self
            .get_with_status_handling(&url, &[("tweet_id", tweet_id)])
            .await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Twitter API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let detail_found = extract_tweet_from_detail_response(&data).is_some();
        debug!(
            tweet_id = %tweet_id,
            detail_found = detail_found,
            "Twitter: Tweet detail fetched"
        );

        Ok(data)
    }

    /// Fetch a single Twitter tweet with retry.
    pub async fn fetch_twitter_tweet_detail_with_retry(
        &self,
        tweet_id: &str,
    ) -> Result<TwitterTweetDetailResponse, TikHubError> {
        let tweet_id = tweet_id.to_string();
        self.with_retry("fetch_twitter_tweet_detail", || {
            let id = tweet_id.clone();
            async move { self.fetch_twitter_tweet_detail(&id).await }
        })
        .await
    }

    /// Fetch Twitter tweet comments/replies
    ///
    /// Endpoint: `/api/v1/twitter/web/fetch_post_comments`
    pub async fn fetch_twitter_comments(
        &self,
        params: &TwitterCommentParams,
    ) -> Result<TwitterCommentsResponse, TikHubError> {
        let url = format!("{}/api/v1/twitter/web/fetch_post_comments", self.base_url);

        debug!(tweet_id = %params.tweet_id, "Twitter: Fetching tweet comments");

        let mut query: Vec<(&str, &str)> = vec![("tweet_id", &params.tweet_id)];

        let cursor_str;
        if let Some(ref cursor) = params.cursor {
            cursor_str = cursor.clone();
            query.push(("cursor", &cursor_str));
        }

        let data: TwitterCommentsResponse = self.get_with_status_handling(&url, &query).await?;

        if data.code != 200 {
            warn!(code = data.code, message = %data.message, "Twitter API error");
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let comment_count = data
            .data
            .as_ref()
            .and_then(|d| d.thread.as_ref())
            .map(|list| list.len())
            .unwrap_or(0);

        debug!(tweet_id = %params.tweet_id, comment_count = comment_count, "Twitter: Comments fetched");

        Ok(data)
    }

    /// Fetch Twitter tweet comments with retry
    pub async fn fetch_twitter_comments_with_retry(
        &self,
        params: &TwitterCommentParams,
    ) -> Result<TwitterCommentsResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_twitter_comments", || {
            let p = params.clone();
            async move { self.fetch_twitter_comments(&p).await }
        })
        .await
    }

    // ============================================================
    // TikTok Helper Methods
    // ============================================================

    /// Extract videos from search response
    pub fn extract_videos(response: &SearchResponse) -> Vec<&AwemeInfo> {
        response
            .data
            .as_ref()
            .and_then(|d| d.search_item_list.as_ref())
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.aweme_info.as_ref())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract comments from response
    pub fn extract_comments(response: &CommentsResponse) -> Vec<&TikTokComment> {
        response
            .data
            .as_ref()
            .and_then(|d| d.comments.as_ref())
            .map(|list| list.iter().collect())
            .unwrap_or_default()
    }

    /// Extract user videos from response
    pub fn extract_user_videos(response: &UserVideosResponse) -> Vec<&AwemeInfo> {
        response
            .data
            .as_ref()
            .and_then(|d| d.aweme_list.as_ref())
            .map(|list| list.iter().collect())
            .unwrap_or_default()
    }

    /// Extract Instagram posts from the V3 general search media grid.
    pub fn extract_instagram_general_posts(
        response: &GeneralSearchResponse,
    ) -> Vec<&InstagramPost> {
        response
            .data
            .as_ref()
            .and_then(|data| data.media_grid.as_ref())
            .and_then(|grid| grid.sections.as_ref())
            .map(|sections| {
                sections
                    .iter()
                    .filter_map(|section| section.layout_content.as_ref())
                    .filter_map(|content| content.medias.as_ref())
                    .flat_map(|medias| medias.iter())
                    .filter_map(|wrapper| wrapper.media.as_ref())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract Instagram posts from the V2 general search response.
    pub fn extract_instagram_general_v2_posts(
        response: &GeneralSearchV2Response,
    ) -> Vec<&InstagramPost> {
        response
            .data
            .as_ref()
            .and_then(|data| data.data.as_ref())
            .and_then(|data| data.items.as_ref())
            .map(|items| items.iter().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_params_builder() {
        let params = SearchParams::new("test")
            .with_region("GB")
            .with_count(15)
            .with_offset(10);

        assert_eq!(params.keyword, "test");
        assert_eq!(params.region, "GB");
        assert_eq!(params.count, 15);
        assert_eq!(params.offset, 10);
    }

    #[test]
    fn test_search_params_max_count() {
        let params = SearchParams::new("test").with_count(100);
        assert_eq!(params.count, 20); // Should be capped at 20
    }

    #[test]
    fn test_comment_params_max_count() {
        let params = CommentParams::new().with_count(500);
        assert_eq!(params.count, 100); // Should be capped at 100
    }
}
