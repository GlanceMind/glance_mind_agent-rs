//! Twitter Adapter - Implements ContentGateway and CommentGateway for Twitter
//!
//! This adapter wraps the TikHubClient to implement the port interfaces for Twitter.
//! It provides automatic retry and proper error mapping.

use async_trait::async_trait;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, Engagement, KeywordType, SearchOptions};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
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
}
