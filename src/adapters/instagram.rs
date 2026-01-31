//! Instagram Adapter - Implements ContentGateway and CommentGateway for Instagram
//!
//! This adapter wraps the TikHubClient to implement the port interfaces for Instagram.
//! It provides automatic retry and proper error mapping.

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    CommentGateway, ContentGateway,
};
use crate::tikhub::{
    InstagramComment, InstagramCommentParams, InstagramPost, InstagramV1Edge, InstagramV1Node,
    TikHubClient, TikHubError, UserPostsParams,
};

/// Instagram adapter implementing ContentGateway and CommentGateway
pub struct InstagramAdapter {
    client: TikHubClient,
}

impl InstagramAdapter {
    /// Create a new Instagram adapter with the given client
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
                GatewayError::AuthFailed(format!("Instagram auth failed: {}", message))
            }
            TikHubError::PaymentRequired { message } => {
                GatewayError::AuthFailed(format!("Instagram payment required: {}", message))
            }
            TikHubError::Forbidden { message } => {
                GatewayError::AuthFailed(format!("Instagram access forbidden: {}", message))
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

    /// Convert Instagram post to domain Content
    fn convert_content(post: &InstagramPost) -> Content {
        let post_id = post.post_id().unwrap_or("").to_string();

        Content {
            platform: "instagram".to_string(),
            content_id: post_id,
            author: post.author_username().unwrap_or("").to_string(),
            author_name: post.author_name().map(|s| s.to_string()),
            description: post.caption_text_str().to_string(),
            url: post
                .code
                .as_ref()
                .map(|c| format!("https://www.instagram.com/p/{}/", c)),
            engagement: Engagement {
                likes: post.likes(),
                comments: post.comments(),
                shares: 0, // Instagram doesn't expose share count via API
                views: post.views(),
            },
            created_at: post.created_at_timestamp(),
            raw_data: serde_json::to_value(post).ok(),
        }
    }

    /// Convert Instagram comment to domain Comment
    fn convert_comment(comment: &InstagramComment, content_id: &str) -> Comment {
        Comment {
            platform: "instagram".to_string(),
            comment_id: comment.id.clone().unwrap_or_default(),
            content_id: content_id.to_string(),
            parent_id: comment.parent_comment_id.clone(),
            author: comment.username().unwrap_or("").to_string(),
            author_name: comment.nickname().map(|s| s.to_string()),
            author_uid: comment.user_id().map(|s| s.to_string()),
            text: comment.content().to_string(),
            likes: comment.likes(),
            reply_count: comment.reply_count(),
            created_at: comment.created_at,
            language: None, // Instagram API doesn't provide language
            is_reply: comment.is_reply(),
            raw_data: serde_json::to_value(comment).ok(),
        }
    }

    /// Extract posts from API response
    fn extract_posts_from_response<T>(
        data: &Option<crate::tikhub::InstagramPaginatedData<T>>,
    ) -> Vec<&T> {
        data.as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| list.iter().collect())
            .unwrap_or_default()
    }

    /// Convert Instagram V1 node to domain Content
    fn convert_v1_node(node: &InstagramV1Node) -> Content {
        let post_id = node.id.clone().unwrap_or_default();
        let shortcode = node.shortcode.clone().unwrap_or_default();

        // Extract caption text from nested structure
        let caption = node
            .edge_media_to_caption
            .as_ref()
            .and_then(|e| e.edges.as_ref())
            .and_then(|edges| edges.first())
            .and_then(|edge| edge.node.as_ref())
            .and_then(|n| {
                // edge_media_to_caption edges contain V1Edge with InstagramV1Node
                // but the text is in extra field as raw JSON
                n.extra
                    .as_ref()
                    .and_then(|v| v.get("text"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        let owner = node.owner.as_ref();
        let author = owner.and_then(|o| o.username.clone()).unwrap_or_default();

        Content {
            platform: "instagram".to_string(),
            content_id: post_id,
            author,
            author_name: None,
            description: caption,
            url: if !shortcode.is_empty() {
                Some(format!("https://www.instagram.com/p/{}/", shortcode))
            } else {
                None
            },
            engagement: Engagement {
                likes: node
                    .edge_liked_by
                    .as_ref()
                    .and_then(|e| e.count)
                    .unwrap_or(0),
                comments: node
                    .edge_media_to_comment
                    .as_ref()
                    .and_then(|e| e.count)
                    .unwrap_or(0),
                shares: 0,
                views: node.video_view_count.unwrap_or(0),
            },
            created_at: node.taken_at_timestamp,
            raw_data: serde_json::to_value(node).ok(),
        }
    }

    /// Extract V1 edges from response and convert to Content
    fn extract_v1_contents(edges: &[InstagramV1Edge], count: usize) -> Vec<Content> {
        edges
            .iter()
            .filter_map(|edge| edge.node.as_ref())
            .take(count)
            .map(Self::convert_v1_node)
            .collect()
    }
}

#[async_trait]
impl ContentGateway for InstagramAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        // Use V1 API (more stable) with hashtag parameter
        // Remove # prefix if present
        let query = options.query.trim_start_matches('#');

        tracing::info!(hashtag = %query, "Instagram: Searching via V1 API");

        let response = self
            .client
            .search_hashtag_posts_v1_with_retry(query, None)
            .await
            .map_err(Self::convert_error)?;

        let edges = response
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.hashtag.as_ref())
            .and_then(|h| h.edge_hashtag_to_media.as_ref())
            .and_then(|e| e.edges.as_ref());

        match edges {
            Some(edges) => Ok(Self::extract_v1_contents(edges, options.count as usize)),
            None => Ok(vec![]),
        }
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                // Use V1 API (more stable) with hashtag parameter
                // Remove # prefix if present
                let query = query.trim_start_matches('#');

                tracing::info!(hashtag = %query, "Instagram: Searching via V1 API");

                let response = self
                    .client
                    .search_hashtag_posts_v1_with_retry(query, None)
                    .await
                    .map_err(Self::convert_error)?;

                let edges = response
                    .data
                    .as_ref()
                    .and_then(|d| d.data.as_ref())
                    .and_then(|d| d.hashtag.as_ref())
                    .and_then(|h| h.edge_hashtag_to_media.as_ref())
                    .and_then(|e| e.edges.as_ref());

                match edges {
                    Some(edges) => Ok(Self::extract_v1_contents(edges, options.count as usize)),
                    None => Ok(vec![]),
                }
            }
            KeywordType::UserId(username) => self.fetch_user_content(username, options.count).await,
            KeywordType::ContentId(content_id) => {
                // Instagram doesn't support direct post fetch via this API
                // Return empty for now
                tracing::warn!(
                    "Instagram direct post fetch not supported for ID: {}",
                    content_id
                );
                Ok(vec![])
            }
            KeywordType::SecUserId(user_id) => {
                // Fetch by user ID
                let params = UserPostsParams::by_user_id(user_id);
                let response = self
                    .client
                    .fetch_instagram_user_posts_with_retry(&params)
                    .await
                    .map_err(Self::convert_error)?;

                let posts: Vec<&InstagramPost> = Self::extract_posts_from_response(&response.data);
                Ok(posts
                    .iter()
                    .take(options.count as usize)
                    .map(|p| Self::convert_content(p))
                    .collect())
            }
        }
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        let params = UserPostsParams::by_username(user_id);

        let response = self
            .client
            .fetch_instagram_user_posts_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let posts: Vec<&InstagramPost> = Self::extract_posts_from_response(&response.data);
        Ok(posts
            .iter()
            .take(count as usize)
            .map(|p| Self::convert_content(p))
            .collect())
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        // Instagram V2 API doesn't support direct post fetch by ID
        Err(GatewayError::NotFound(format!(
            "Direct post fetch not supported for Instagram ID: {}",
            content_id
        )))
    }

    fn platform(&self) -> &str {
        "instagram"
    }
}

#[async_trait]
impl CommentGateway for InstagramAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        let mut params = InstagramCommentParams::new(content_id).with_sort_by("recent");

        if let Some(ref cursor) = options.cursor {
            params = params.with_pagination_token(cursor);
        }

        let response = self
            .client
            .fetch_instagram_comments_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let comments: Vec<Comment> = response
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| {
                list.iter()
                    .take(options.count as usize)
                    .map(|c| Self::convert_comment(c, content_id))
                    .collect()
            })
            .unwrap_or_default();

        let next_cursor = response
            .data
            .as_ref()
            .and_then(|d| d.pagination_token.clone());
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
        content_id: &str,
        comment_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        // Use the comment replies endpoint
        let mut params = crate::tikhub::CommentRepliesParams::new(content_id, comment_id);

        if let Some(ref cursor) = options.cursor {
            params = params.with_pagination_token(cursor);
        }

        let response = self
            .client
            .fetch_instagram_comment_replies_with_retry(&params)
            .await
            .map_err(Self::convert_error)?;

        let replies: Vec<Comment> = response
            .data
            .as_ref()
            .and_then(|d| d.data.as_ref())
            .and_then(|d| d.items.as_ref())
            .map(|list| {
                list.iter()
                    .take(options.count as usize)
                    .map(|c| Self::convert_comment(c, content_id))
                    .collect()
            })
            .unwrap_or_default();

        Ok(replies)
    }

    fn platform(&self) -> &str {
        "instagram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_content() {
        let post = InstagramPost {
            code: Some("ABC123".to_string()),
            id: Some("12345".to_string()),
            product_type: Some("clips".to_string()),
            media_type: Some(2),
            caption_text: Some("Test caption".to_string()),
            caption: None,
            user: Some(crate::tikhub::InstagramUser {
                id: Some("u123".to_string()),
                username: Some("testuser".to_string()),
                full_name: Some("Test User".to_string()),
                profile_pic_url: None,
                is_verified: Some(false),
            }),
            like_count: Some(100),
            comment_count: Some(10),
            play_count: Some(1000),
            thumbnail_url: None,
            image_versions: None,
            video_versions: None,
            is_video: Some(true),
            taken_at_ts: Some(1234567890),
            taken_at: None,
        };

        let content = InstagramAdapter::convert_content(&post);
        assert_eq!(content.platform, "instagram");
        assert_eq!(content.content_id, "12345");
        assert_eq!(content.author, "testuser");
        assert_eq!(content.engagement.likes, 100);
        assert_eq!(content.engagement.views, 1000);
    }

    #[test]
    fn test_convert_comment() {
        let comment = InstagramComment {
            id: Some("c123".to_string()),
            text: Some("Great post!".to_string()),
            user: Some(crate::tikhub::InstagramUser {
                id: Some("u456".to_string()),
                username: Some("commenter".to_string()),
                full_name: Some("Commenter Name".to_string()),
                profile_pic_url: None,
                is_verified: None,
            }),
            like_count: Some(50),
            child_comment_count: Some(5),
            created_at: Some(1234567890),
            parent_comment_id: None,
        };

        let domain_comment = InstagramAdapter::convert_comment(&comment, "post123");
        assert_eq!(domain_comment.platform, "instagram");
        assert_eq!(domain_comment.comment_id, "c123");
        assert_eq!(domain_comment.content_id, "post123");
        assert_eq!(domain_comment.text, "Great post!");
        assert_eq!(domain_comment.author, "commenter");
        assert_eq!(domain_comment.likes, 50);
        assert!(!domain_comment.is_reply);
    }
}
