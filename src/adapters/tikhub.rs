//! TikHub Adapter - Implements ContentGateway and CommentGateway for TikTok
//!
//! This adapter wraps the TikHubClient to implement the port interfaces.
//! It provides automatic retry and proper error mapping.

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    CommentGateway, ContentGateway,
};
use crate::tikhub::{
    AwemeInfo, CommentParams, SearchParams, TikHubClient, TikHubError, TikHubRetryConfig,
    UserVideoParams,
};

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

        if !page.has_more {
            break;
        }

        match page.next_cursor {
            Some(next) => {
                cursor = next;
                // Small delay before fetching the NEXT page only (never after
                // the terminal page) to avoid rate limiting.
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            // No cursor to advance with: stop rather than re-fetch the same page.
            None => break,
        }
    }

    collected.truncate(target);
    Ok(collected)
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
            .with_offset(cursor as u32);

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
        // paginate (offset/cursor loop) to reach the requested total.
        let fetcher = SearchPageFetcher {
            client: &self.client,
            keyword: options.query.clone(),
            region: options.region.clone(),
            sort_type: options.sort_type,
            publish_time: options.publish_time,
        };

        let videos = paginate_videos(options.count as usize, 20, &fetcher)
            .await
            .map_err(Self::convert_error)?;

        Ok(videos.iter().map(Self::convert_content).collect())
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
                let params = UserVideoParams::by_sec_user_id(sec_uid).with_count(options.count);

                // Use retry-enabled fetch
                let response = self
                    .client
                    .fetch_user_videos_with_retry(&params)
                    .await
                    .map_err(Self::convert_error)?;

                let videos = TikHubClient::extract_user_videos(&response);
                Ok(videos.iter().map(|v| Self::convert_content(v)).collect())
            }
            KeywordType::ContentId(content_id) => match self.fetch_by_id(content_id).await? {
                Some(content) => Ok(vec![content]),
                None => Ok(vec![]),
            },
        }
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        let params = UserVideoParams::by_unique_id(user_id).with_count(count);

        // Use retry-enabled fetch
        let response = self
            .client
            .fetch_user_videos_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let videos = TikHubClient::extract_user_videos(&response);
        Ok(videos.iter().map(|v| Self::convert_content(v)).collect())
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
}
