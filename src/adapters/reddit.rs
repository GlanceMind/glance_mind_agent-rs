//! Reddit Adapter - Implements ContentGateway and CommentGateway for Reddit
//!
//! This adapter wraps the TikHubClient to implement the port interfaces for Reddit.
//! It provides automatic retry and proper error mapping.

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    CommentGateway, ContentGateway,
};
use crate::tikhub::{
    extract_comments_from_trees, extract_posts_from_search, RedditComment, RedditCommentParams,
    RedditPost, RedditSearchParams, RedditUserPostsParams, TikHubClient, TikHubError,
};

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
}

#[async_trait]
impl ContentGateway for RedditAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let params = RedditSearchParams::new(&options.query);

        let response = self
            .client
            .search_reddit_posts_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let posts = response
            .data
            .as_ref()
            .map(|d| extract_posts_from_search(d))
            .unwrap_or_default();

        Ok(posts
            .iter()
            .take(options.count as usize)
            .map(|p| Self::convert_content(p))
            .collect())
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                let params = RedditSearchParams::new(query);
                let response = self
                    .client
                    .search_reddit_posts_with_retry(&params)
                    .await
                    .map_err(Self::convert_error)?;

                let posts = response
                    .data
                    .as_ref()
                    .map(|d| extract_posts_from_search(d))
                    .unwrap_or_default();

                Ok(posts
                    .iter()
                    .take(options.count as usize)
                    .map(|p| Self::convert_content(p))
                    .collect())
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
