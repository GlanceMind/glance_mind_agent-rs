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
use crate::tikhub::{
    TikHubClient, TikHubError, TwitterCommentParams, TwitterSearchParams, TwitterTweet,
    TwitterUserTweetsParams,
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

        Content {
            platform: "twitter".to_string(),
            content_id: tweet_id.clone(),
            author: screen_name.to_string(),
            author_name: tweet.author_name().map(|s| s.to_string()),
            description: tweet.content().to_string(),
            url: Some(format!(
                "https://twitter.com/{}/status/{}",
                screen_name, tweet_id
            )),
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
}

#[async_trait]
impl ContentGateway for TwitterAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let params = TwitterSearchParams::new(&options.query).with_search_type("Latest");

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
                    .filter(|t| t.tweet_type.as_deref() == Some("tweet"))
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
                let params = TwitterSearchParams::new(query).with_search_type("Latest");
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
                            .filter(|t| t.tweet_type.as_deref() == Some("tweet"))
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
            KeywordType::ContentId(tweet_id) => {
                // Twitter doesn't support direct tweet fetch via this API
                tracing::warn!(
                    "Twitter direct tweet fetch not supported for ID: {}",
                    tweet_id
                );
                Ok(vec![])
            }
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

        Ok(result.into_iter().take(count as usize).collect())
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        // Twitter API via TikHub doesn't support direct tweet fetch
        Err(GatewayError::NotFound(format!(
            "Direct tweet fetch not supported for Twitter ID: {}",
            content_id
        )))
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

    #[test]
    fn test_convert_content() {
        let tweet = TwitterTweet {
            tweet_id: Some("123456".to_string()),
            id: None,
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
}
