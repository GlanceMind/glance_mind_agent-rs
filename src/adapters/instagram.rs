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
        let media_id = post.post_id().unwrap_or("").to_string();
        let shortcode = post.code.clone().unwrap_or_default();

        // IMPORTANT: Use shortcode as content_id, NOT media_id!
        // TikHub Instagram comment APIs require shortcode (e.g., "CxYZaBcDeF")
        // not the numeric media_id (e.g., "3824454208267080788").
        // The shortcode is what appears in Instagram URLs: instagram.com/p/{shortcode}/
        let content_id = if !shortcode.is_empty() {
            shortcode.clone()
        } else {
            // Fallback to media_id if shortcode is missing (shouldn't happen normally)
            tracing::warn!(
                media_id = %media_id,
                "Instagram post missing shortcode (code field), falling back to media_id (comments may fail)"
            );
            media_id.clone()
        };

        Content {
            platform: "instagram".to_string(),
            content_id,
            author: post.author_username().unwrap_or("").to_string(),
            author_name: post.author_name().map(|s| s.to_string()),
            description: post.caption_text_str().to_string(),
            url: if !shortcode.is_empty() {
                Some(format!("https://www.instagram.com/p/{}/", shortcode))
            } else {
                None
            },
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

    fn normalize_v3_query(query: &str) -> String {
        let query = query.trim();
        if query.starts_with('#') {
            query.to_string()
        } else {
            format!("#{}", query)
        }
    }

    async fn fetch_keyword_posts_with_fallback(
        &self,
        raw_query: &str,
    ) -> GatewayResult<Vec<InstagramPost>> {
        let v3_query = Self::normalize_v3_query(raw_query);

        tracing::info!(query = %v3_query, "Instagram: Searching via V3 general_search");

        match self.client.search_instagram_general(&v3_query).await {
            Ok(response) => {
                let posts = TikHubClient::extract_instagram_general_posts(&response)
                    .into_iter()
                    .cloned()
                    .collect();
                Ok(posts)
            }
            Err(TikHubError::BadRequest { message }) => {
                tracing::warn!(
                    query = %v3_query,
                    error = %message,
                    "Instagram V3 general_search returned bad request; falling back to V2 general_search"
                );

                let response = self
                    .client
                    .search_instagram_general_v2_with_retry(raw_query)
                    .await
                    .map_err(Self::convert_error)?;
                let posts = TikHubClient::extract_instagram_general_v2_posts(&response)
                    .into_iter()
                    .cloned()
                    .collect();
                Ok(posts)
            }
            Err(err) => Err(Self::convert_error(err)),
        }
    }

    /// Convert Instagram V1 node to domain Content
    #[allow(dead_code)]
    fn convert_v1_node(node: &InstagramV1Node) -> Content {
        let media_id = node.id.clone().unwrap_or_default();
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

        // IMPORTANT: Use shortcode as content_id, NOT media_id!
        // TikHub Instagram V2 fetch_post_comments API requires shortcode (e.g., "CxYZaBcDeF")
        // not the numeric media_id (e.g., "3824454208267080788").
        // The shortcode is what appears in Instagram URLs: instagram.com/p/{shortcode}/
        let content_id = if !shortcode.is_empty() {
            shortcode.clone()
        } else {
            // Fallback to media_id if shortcode is missing (shouldn't happen normally)
            tracing::warn!(
                media_id = %media_id,
                "Instagram post missing shortcode, falling back to media_id (comments may fail)"
            );
            media_id.clone()
        };

        Content {
            platform: "instagram".to_string(),
            content_id,
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
    #[allow(dead_code)]
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
        let posts = self
            .fetch_keyword_posts_with_fallback(&options.query)
            .await?;
        Ok(posts
            .iter()
            .take(options.count as usize)
            .map(Self::convert_content)
            .collect())
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                let posts = self.fetch_keyword_posts_with_fallback(query).await?;
                Ok(posts
                    .iter()
                    .take(options.count as usize)
                    .map(Self::convert_content)
                    .collect())
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
        // TikHub Instagram V2 API requires full URL format for fetch_post_comments
        // Convert shortcode to full URL: https://www.instagram.com/p/{shortcode}/
        let code_or_url = if content_id.starts_with("http") {
            content_id.to_string()
        } else {
            format!("https://www.instagram.com/p/{}/", content_id)
        };

        tracing::debug!(
            shortcode = %content_id,
            url = %code_or_url,
            "Instagram: Fetching comments with full URL"
        );

        let mut params = InstagramCommentParams::new(&code_or_url).with_sort_by("recent");

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
        // TikHub Instagram V2 API requires full URL format
        let code_or_url = if content_id.starts_with("http") {
            content_id.to_string()
        } else {
            format!("https://www.instagram.com/p/{}/", content_id)
        };

        // Use the comment replies endpoint
        let mut params = crate::tikhub::CommentRepliesParams::new(&code_or_url, comment_id);

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
                    let mut buffer = vec![0_u8; 8192];
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
                        400 => "Bad Request",
                        401 => "Unauthorized",
                        429 => "Too Many Requests",
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

    use crate::ports::content_gateway::FetchShortfall;

    /// Build one V3 `general_search` media item (`media_grid.sections[].layout_content.medias[].media`).
    /// Shape sourced from `instagram_types.rs` serde defs (DR-20):
    /// InstagramGeneralSearchData → InstagramMediaGrid → InstagramMediaGridSection
    /// → InstagramLayoutContent → InstagramMediaWrapper → InstagramPost.
    fn instagram_v3_media_item(code: &str, username: &str) -> serde_json::Value {
        json!({
            "media": {
                "code": code,
                "pk": format!("pk-{code}"),
                "product_type": "clips",
                "media_type": 2,
                "caption": {"text": format!("v3 post {code}")},
                "user": {
                    "id": format!("user-{username}"),
                    "pk": format!("user-{username}"),
                    "username": username,
                    "full_name": username,
                    "profile_pic_url": "https://example.test/avatar.jpg",
                    "is_verified": false
                },
                "like_count": 5,
                "comment_count": 1,
                "play_count": 42,
                "taken_at": 1778338738
            }
        })
    }

    /// Build a V3 `general_search` 200 response carrying `count` posts in a single page.
    /// (Branch B context: upstream returns a single page — `has_more=false`, no next token.)
    fn instagram_v3_single_page(count: usize) -> serde_json::Value {
        let medias: Vec<serde_json::Value> = (0..count)
            .map(|i| instagram_v3_media_item(&format!("V3CODE{i:03}"), &format!("creator{i}")))
            .collect();
        json!({
            "code": 200,
            "message": "ok",
            "data": {
                "rank_token": "rank-abc",
                "media_grid": {
                    "sections": [
                        {
                            "layout_type": "media_grid",
                            "feed_type": "media",
                            "layout_content": { "medias": medias }
                        }
                    ],
                    "rank_token": "rank-abc",
                    "next_max_id": null,
                    "has_more": false
                }
            }
        })
    }

    fn instagram_v2_response(code: &str, username: &str, url: &str) -> serde_json::Value {
        json!({
            "code": 200,
            "message": "ok",
            "data": {
                "data": {
                    "items": [
                        {
                            "code": code,
                            "pk": format!("pk-{code}"),
                            "product_type": "clips",
                            "media_type": 2,
                            "caption": {"text": "fallback result"},
                            "user": {
                                "id": format!("user-{username}"),
                                "pk": format!("user-{username}"),
                                "username": username,
                                "full_name": username,
                                "profile_pic_url": "https://example.test/avatar.jpg",
                                "is_verified": false
                            },
                            "like_count": 7,
                            "comment_count": 2,
                            "play_count": 99,
                            "image_versions2": {
                                "candidates": [
                                    {"url": url, "width": 640, "height": 640}
                                ]
                            },
                            "taken_at": 1778338738
                        }
                    ]
                },
                "pagination_token": null
            }
        })
    }

    #[test]
    fn test_convert_content() {
        let post = InstagramPost {
            code: Some("ABC123".to_string()),
            id: Some("12345".to_string()),
            pk: None,
            product_type: Some("clips".to_string()),
            media_type: Some(2),
            caption_text: Some("Test caption".to_string()),
            caption: None,
            user: Some(crate::tikhub::InstagramUser {
                id: Some("u123".to_string()),
                pk: None,
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
            image_versions2: None,
            video_versions: None,
            is_video: Some(true),
            taken_at_ts: Some(1234567890),
            taken_at: None,
        };

        let content = InstagramAdapter::convert_content(&post);
        assert_eq!(content.platform, "instagram");
        // content_id should be shortcode (code), not media_id (id)
        // This is required for TikHub comment API which expects shortcode
        assert_eq!(content.content_id, "ABC123");
        assert_eq!(content.author, "testuser");
        assert_eq!(content.engagement.likes, 100);
        assert_eq!(content.engagement.views, 1000);
    }

    #[test]
    fn test_convert_comment() {
        let comment = InstagramComment {
            id: Some("c123".to_string()),
            pk: None,
            text: Some("Great post!".to_string()),
            user: Some(crate::tikhub::InstagramUser {
                id: Some("u456".to_string()),
                pk: None,
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

    #[tokio::test]
    async fn test_fetch_by_keyword_falls_back_to_v2_when_v3_returns_bad_request() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                400,
                json!({
                    "detail": {
                        "code": 400,
                        "router": "/api/v1/instagram/v3/general_search",
                        "params": {
                            "query": "#fitness",
                            "enable_metadata": "true"
                        }
                    }
                }),
            ),
            MockHttpResponse::json(
                200,
                instagram_v2_response("FALLBACK1", "creator", "https://example.test/thumb.jpg"),
            ),
        ])
        .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let content = adapter
            .fetch_by_keyword(
                &KeywordType::Hashtag("fitness".to_string()),
                &SearchOptions::new("fitness")
                    .with_platform("instagram")
                    .with_count(5),
            )
            .await
            .expect("V3 bad request should fall back to V2");

        assert_eq!(content.len(), 1);
        assert_eq!(content[0].content_id, "FALLBACK1");
        assert_eq!(content[0].author, "creator");
        assert_eq!(
            content[0].url.as_deref(),
            Some("https://www.instagram.com/p/FALLBACK1/")
        );

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].starts_with(
                "GET /api/v1/instagram/v3/general_search?query=%23fitness&enable_metadata=true "
            ),
            "unexpected request line: {}",
            requests[0]
        );
        assert!(
            requests[1].starts_with("GET /api/v1/instagram/v2/general_search?keyword=fitness "),
            "unexpected request line: {}",
            requests[1]
        );
    }

    #[tokio::test]
    async fn test_fetch_by_keyword_does_not_fallback_on_unauthorized() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
                401,
                json!({"message": "unauthorized"}),
            )])
            .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let result = adapter
            .fetch_by_keyword(
                &KeywordType::Hashtag("fitness".to_string()),
                &SearchOptions::new("fitness")
                    .with_platform("instagram")
                    .with_count(5),
            )
            .await;

        assert!(matches!(result, Err(GatewayError::AuthFailed(_))));

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn test_fetch_by_keyword_does_not_fallback_on_rate_limit() {
        let (base_url, requests) =
            spawn_mock_http_server_with_capture(vec![MockHttpResponse::json(
                429,
                json!({"message": "rate limited"}),
            )])
            .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let result = adapter
            .fetch_by_keyword(
                &KeywordType::Hashtag("fitness".to_string()),
                &SearchOptions::new("fitness")
                    .with_platform("instagram")
                    .with_count(5),
            )
            .await;

        assert!(matches!(result, Err(GatewayError::RateLimited { .. })));

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn test_search_uses_same_v3_to_v2_fallback() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(
                400,
                json!({
                    "detail": {
                        "code": 400,
                        "router": "/api/v1/instagram/v3/general_search",
                        "params": {
                            "query": "#travel",
                            "enable_metadata": "true"
                        }
                    }
                }),
            ),
            MockHttpResponse::json(
                200,
                instagram_v2_response("SEARCH1", "traveler", "https://example.test/travel.jpg"),
            ),
        ])
        .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let content = adapter
            .search(
                &SearchOptions::new("travel")
                    .with_platform("instagram")
                    .with_count(1),
            )
            .await
            .expect("search should use the same V3 to V2 fallback");

        assert_eq!(content.len(), 1);
        assert_eq!(content[0].content_id, "SEARCH1");
        assert_eq!(content[0].author, "traveler");

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
    }

    // ===== M5-T3-B 分支 B:单页 + 如实上报 override(m5-instagram-p2.md §3 M5-T3-B) =====
    // 裁决:V1=分支 B(用户裁决 2026-06-11,保守解读)。T3-A 标 NOT-TAKEN。
    // 全部经可观测 `fetch_by_keyword_with_outcome`(contents / shortfall / 捕获请求数)断言。
    // mock 形状取自 instagram_types.rs serde 定义(DR-20:instagram_v3_single_page helper)。
    // 反作弊声明:不得修改断言;欠量上报 Exhausted 是 R-006 的规格本体,
    // 不得改为 None/COMPLETED 语义。

    fn keyword_search_options(count: u32) -> SearchOptions {
        SearchOptions::new("fitness")
            .with_platform("instagram")
            .with_count(count)
    }

    fn search_keyword() -> KeywordType {
        KeywordType::Hashtag("fitness".to_string())
    }

    /// M5-T3-B 测试 1(T-014-B 主断言 / R-006 红线):
    /// 单页 20 条、count=50 → contents.len()==20、shortfall == Some(Exhausted)。
    /// 「上游单页即枯竭,欠量绝不静默 COMPLETED」(事故反模式的 instagram 钉子)。
    /// 预期 RED(默认方法无 override,shortfall 默认 None):
    ///   left: None, right: Some(Exhausted)。
    #[tokio::test]
    async fn single_page_underdelivery_reports_exhausted() {
        let (base_url, _requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(200, instagram_v3_single_page(20)),
        ])
        .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .expect("single-page underdelivery should be Ok with Exhausted shortfall, not Err");

        assert_eq!(
            outcome.contents.len(),
            20,
            "single page delivers exactly its 20 items; got {}",
            outcome.contents.len()
        );
        assert_eq!(
            outcome.shortfall,
            Some(FetchShortfall::Exhausted),
            "underdelivery (delivered 20 < count 50) MUST report Some(Exhausted), \
             never silent None/COMPLETED (R-006 red line); got {:?}",
            outcome.shortfall
        );
    }

    /// M5-T3-B 测试 2:单页 ≥ count → 截取 count 条、shortfall == None。
    /// 预期 RED(默认方法不截取/可能交付全部 + 无 None 语义保证;实现者 override 后达量 None)。
    #[tokio::test]
    async fn single_page_reaching_count_no_shortfall() {
        let (base_url, _requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(200, instagram_v3_single_page(60)),
        ])
        .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let outcome = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await
            .expect("reaching count should be Ok");

        assert_eq!(
            outcome.contents.len(),
            50,
            "single page with >=count items must be truncated to count (50); got {}",
            outcome.contents.len()
        );
        assert!(
            outcome.shortfall.is_none(),
            "reaching count (delivered 50 == count 50) must report None; got {:?}",
            outcome.shortfall
        );
    }

    /// M5-T3-B 测试 3(F-001;允许先绿 AG-006):首调即错 → Err。
    /// 分支 B 的 V3 路径为单次调用(非翻页),429 即 RateLimited Err,零进展。
    /// (DR-19 的「429 = 4 次 HTTP 请求」属翻页重试语境;branch-B V3 单次调用不重试,
    /// 故请求数 = 1;核心契约 = 零进展 → Err。)
    #[tokio::test]
    async fn zero_progress_error_is_err() {
        let (base_url, _requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(429, json!({"message": "rate limited"})),
        ])
        .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let result = adapter
            .fetch_by_keyword_with_outcome(&search_keyword(), &keyword_search_options(50))
            .await;

        assert!(
            result.is_err(),
            "zero-progress first-call error must be Err (F-001); got {:?}",
            result.map(|o| (o.contents.len(), o.shortfall))
        );
    }

    /// M5-T3-B 测试 4(回归;允许先绿 AG-006):既有 `search()` 路径不受 override 影响。
    /// (legacy `search()` 仍走 V3→V2 fallback,行为不变。)
    #[tokio::test]
    async fn legacy_search_unchanged() {
        let (base_url, requests) = spawn_mock_http_server_with_capture(vec![
            MockHttpResponse::json(200, instagram_v3_single_page(3)),
        ])
        .await;
        let client = TikHubClient::new("test-key", base_url).unwrap();
        let adapter = InstagramAdapter::new(client);

        let content = adapter
            .search(&keyword_search_options(5))
            .await
            .expect("legacy search should remain functional");

        // single V3 page of 3, count=5 → take(5) yields all 3; search() returns Vec<Content>
        // (no shortfall surface on legacy path).
        assert_eq!(content.len(), 3, "legacy search returns the 3 available posts");
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "legacy search makes a single V3 request");
    }
}
