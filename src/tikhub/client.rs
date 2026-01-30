//! TikHub HTTP Client Implementation
//!
//! Provides robust TikHub API access with:
//! - Automatic retry with exponential backoff
//! - Proper error classification based on HTTP status codes
//! - Partial data recovery on pagination failures

use std::future::Future;
use std::time::Duration;
use reqwest::{Client, header};
use tracing::{info, warn, debug, error};

use super::types::*;
use super::error::{TikHubError, TikHubRetryConfig, PartialFetchResult};

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
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self, TikHubError> {
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
        let api_key = std::env::var("TIKHUB_API_KEY")
            .map_err(|_| TikHubError::MissingApiKey)?;
        
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
        let response = self.client
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
        
        // Parse JSON response
        let data: T = response.json().await?;
        Ok(data)
    }
    
    /// Execute a request with automatic retry based on error type
    async fn with_retry<T, F, Fut>(
        &self,
        operation: &str,
        f: F,
    ) -> Result<T, TikHubError>
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
    pub async fn search_videos(&self, params: &SearchParams) -> Result<SearchResponse, TikHubError> {
        let url = format!("{}/api/v1/tiktok/app/v3/fetch_video_search_result", self.base_url);
        
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

        let data: SearchResponse = self.get_with_status_handling(
            &url,
            &[
                ("keyword", params.keyword.as_str()),
                ("offset", &params.offset.to_string()),
                ("count", &params.count.to_string()),
                ("region", region),
                ("sort_type", &params.sort_type.to_string()),
                ("publish_time", &params.publish_time.to_string()),
            ],
        ).await?;

        // Check API-level response code
        if data.code != 200 {
            warn!(
                code = data.code,
                message = %data.message,
                "TikHub API error in response body"
            );
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let video_count = data.data
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
    pub async fn search_videos_with_retry(&self, params: &SearchParams) -> Result<SearchResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("search_videos", || {
            let p = params.clone();
            async move { self.search_videos(&p).await }
        }).await
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

        let data: CommentsResponse = self.get_with_status_handling(
            &url,
            &[
                ("aweme_id", aweme_id),
                ("cursor", &params.cursor),
                ("count", &params.count.to_string()),
                ("current_region", ""),
            ],
        ).await?;

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

        let comment_count = data.data
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
        }).await
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

                    cursor = data.cursor.map(|c| c.to_string()).unwrap_or_else(|| "0".to_string());

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
    pub async fn fetch_user_videos(&self, params: &UserVideoParams) -> Result<UserVideosResponse, TikHubError> {
        let url = format!("{}/api/v1/tiktok/app/v3/fetch_user_post_videos", self.base_url);

        let sec_user_id = params.sec_user_id.as_deref().unwrap_or("");
        let unique_id = params.unique_id.as_deref().unwrap_or("");

        if sec_user_id.is_empty() && unique_id.is_empty() {
            return Err(TikHubError::InvalidParam("Either sec_user_id or unique_id is required".to_string()));
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

        let data: UserVideosResponse = self.get_with_status_handling(
            &url,
            &[
                ("sec_user_id", sec_user_id),
                ("unique_id", unique_id),
                ("max_cursor", &params.max_cursor.to_string()),
                ("count", &params.count.to_string()),
                ("sort_type", &params.sort_type.to_string()),
            ],
        ).await?;

        // Check API-level response code
        if data.code != 200 {
            warn!(
                code = data.code,
                message = %data.message,
                "TikHub API error in response body"
            );
            return Err(TikHubError::from_api_code(data.code, &data.message));
        }

        let video_count = data.data
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
    pub async fn fetch_user_videos_with_retry(&self, params: &UserVideoParams) -> Result<UserVideosResponse, TikHubError> {
        let params = params.clone();
        self.with_retry("fetch_user_videos", || {
            let p = params.clone();
            async move { self.fetch_user_videos(&p).await }
        }).await
    }

    // ============================================================
    // Helper Methods
    // ============================================================

    /// Extract videos from search response
    pub fn extract_videos(response: &SearchResponse) -> Vec<&AwemeInfo> {
        response.data
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
        response.data
            .as_ref()
            .and_then(|d| d.comments.as_ref())
            .map(|list| list.iter().collect())
            .unwrap_or_default()
    }

    /// Extract user videos from response
    pub fn extract_user_videos(response: &UserVideosResponse) -> Vec<&AwemeInfo> {
        response.data
            .as_ref()
            .and_then(|d| d.aweme_list.as_ref())
            .map(|list| list.iter().collect())
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
