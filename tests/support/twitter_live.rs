#![allow(dead_code)]

use std::{collections::HashSet, future::Future, sync::OnceLock};

use serde_json::Value;
use tokio::{
    sync::{Mutex, OnceCell},
    time::{sleep, Duration},
};

use glance_mind_agent_rs::{
    domain::errors::{GatewayError, GatewayResult},
    ports::comment_gateway::FetchCommentsOptions,
    tikhub::{
        extract_tweet_from_detail_response, TikHubClient, TikHubError, TwitterCommentParams,
        TwitterSearchParams, TwitterTweet, TwitterUserTweetsParams,
    },
    Comment, CommentGateway, Content, ContentGateway, KeywordType, PlatformStrategy, SearchOptions,
    TaskConfig, TwitterAdapter, TwitterStrategy,
};

pub const COMMENT_TARGET_COUNT: u32 = 5;
const SEARCH_QUERIES: &[&str] = &["OpenAI", "rustlang", "langchain"];
const SEARCH_TYPE: &str = "Top";

#[derive(Debug, Clone)]
pub struct LiveSearchSeed {
    pub query: String,
    pub tweet_id: String,
    pub screen_name: String,
    pub tweet: TwitterTweet,
}

#[derive(Debug, Clone)]
pub struct LiveCommentSeed {
    pub query: String,
    pub tweet_id: String,
    pub screen_name: String,
    pub tweet: TwitterTweet,
    pub replies: Vec<TwitterTweet>,
}

pub fn live_test_mutex() -> &'static Mutex<()> {
    static LIVE_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    LIVE_TEST_MUTEX.get_or_init(|| Mutex::new(()))
}

pub fn create_client() -> TikHubClient {
    let _ = dotenvy::dotenv();
    TikHubClient::from_env().expect("TIKHUB_API_KEY must be set for real Twitter API tests")
}

pub fn create_adapter() -> TwitterAdapter {
    let _ = dotenvy::dotenv();
    TwitterAdapter::from_env()
        .expect("TwitterAdapter::from_env should succeed for real Twitter tests")
}

pub fn create_strategy() -> TwitterStrategy {
    TwitterStrategy::new()
}

pub async fn retry_tikhub_rate_limit<T, F, Fut>(label: &str, mut operation: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, TikHubError>>,
{
    let fallback_delays_secs = [2_u64, 5, 10];

    for (attempt, fallback_delay) in fallback_delays_secs.iter().enumerate() {
        match operation().await {
            Ok(value) => return value,
            Err(TikHubError::RateLimited { retry_after_secs }) => {
                let delay_secs = retry_after_secs.unwrap_or(*fallback_delay);
                eprintln!(
                    "twitter live test hit rate limit during {label}; retry {} in {}s",
                    attempt + 1,
                    delay_secs
                );
                sleep(Duration::from_secs(delay_secs)).await;
            }
            Err(err) => panic!("{label} should succeed: {err:?}"),
        }
    }

    operation()
        .await
        .unwrap_or_else(|err| panic!("{label} should succeed after retries: {err:?}"))
}

pub async fn retry_gateway_rate_limit<T, F, Fut>(label: &str, mut operation: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = GatewayResult<T>>,
{
    let fallback_delays_secs = [2_u64, 5, 10];

    for (attempt, fallback_delay) in fallback_delays_secs.iter().enumerate() {
        match operation().await {
            Ok(value) => return value,
            Err(GatewayError::RateLimited { retry_after_secs }) => {
                let delay_secs = retry_after_secs.unwrap_or(*fallback_delay);
                eprintln!(
                    "twitter live adapter test hit rate limit during {label}; retry {} in {}s",
                    attempt + 1,
                    delay_secs
                );
                sleep(Duration::from_secs(delay_secs)).await;
            }
            Err(err) => panic!("{label} should succeed: {err:?}"),
        }
    }

    operation()
        .await
        .unwrap_or_else(|err| panic!("{label} should succeed after retries: {err:?}"))
}

pub fn build_options(
    strategy: &TwitterStrategy,
    raw_keyword: &str,
    max_videos: i32,
    extra_pairs: &[(String, Value)],
) -> (KeywordType, SearchOptions) {
    let mut config = TaskConfig::new(1, "twitter")
        .with_region("GLOBAL")
        .with_max_videos(max_videos)
        .with_max_comments_per_video(COMMENT_TARGET_COUNT as i32);

    for (key, value) in extra_pairs {
        config.extra.insert(key.clone(), value.clone());
    }

    let keyword = strategy.parse_keyword(raw_keyword);
    let options = strategy.build_search_options(&config, &keyword);
    (keyword, options)
}

pub async fn fetch_contents_for_keyword(
    adapter: &TwitterAdapter,
    strategy: &TwitterStrategy,
    label: &str,
    raw_keyword: &str,
    max_videos: i32,
    extra_pairs: &[(String, Value)],
) -> Vec<Content> {
    let (keyword, options) = build_options(strategy, raw_keyword, max_videos, extra_pairs);
    retry_gateway_rate_limit(label, || adapter.fetch_by_keyword(&keyword, &options)).await
}

pub async fn fetch_first_content_for_keyword(
    adapter: &TwitterAdapter,
    strategy: &TwitterStrategy,
    label: &str,
    raw_keyword: &str,
    max_videos: i32,
    extra_pairs: &[(String, Value)],
) -> Option<Content> {
    fetch_contents_for_keyword(
        adapter,
        strategy,
        label,
        raw_keyword,
        max_videos,
        extra_pairs,
    )
    .await
    .into_iter()
    .next()
}

pub async fn fetch_comments_for_content(
    adapter: &TwitterAdapter,
    label: &str,
    content_id: &str,
    max_count: u32,
) -> Vec<Comment> {
    retry_gateway_rate_limit(label, || adapter.fetch_all_comments(content_id, max_count)).await
}

pub async fn fetch_single_comment_page(
    adapter: &TwitterAdapter,
    label: &str,
    content_id: &str,
    count: u32,
) -> glance_mind_agent_rs::ports::comment_gateway::FetchCommentsResult {
    retry_gateway_rate_limit(label, || async {
        let options = FetchCommentsOptions::new(count);
        adapter.fetch_comments(content_id, &options).await
    })
    .await
}

pub async fn pick_search_seed() -> LiveSearchSeed {
    static SEARCH_SEED: OnceLock<OnceCell<LiveSearchSeed>> = OnceLock::new();
    SEARCH_SEED
        .get_or_init(OnceCell::const_new)
        .get_or_init(|| async {
            let client = create_client();
            for query in SEARCH_QUERIES {
                let response = retry_tikhub_rate_limit("twitter search seed", || async {
                    let params = TwitterSearchParams::new(*query).with_search_type(SEARCH_TYPE);
                    client.search_twitter_tweets_with_retry(&params).await
                })
                .await;

                if let Some(tweet) = response
                    .data
                    .as_ref()
                    .and_then(|data| data.timeline.as_ref())
                    .and_then(|timeline| timeline.iter().find(|tweet| is_valid_seed_tweet(tweet)))
                    .cloned()
                {
                    return LiveSearchSeed {
                        query: (*query).to_string(),
                        tweet_id: tweet
                            .get_tweet_id()
                            .expect("validated tweet should have an id")
                            .to_string(),
                        screen_name: tweet
                            .author_handle()
                            .expect("validated tweet should have a handle")
                            .to_string(),
                        tweet,
                    };
                }
            }

            panic!(
                "expected at least one live Twitter search result with tweet id, text, and author"
            )
        })
        .await
        .clone()
}

pub async fn pick_comment_seed() -> LiveCommentSeed {
    static COMMENT_SEED: OnceLock<OnceCell<LiveCommentSeed>> = OnceLock::new();
    COMMENT_SEED
        .get_or_init(OnceCell::const_new)
        .get_or_init(|| async {
            let client = create_client();
            for query in SEARCH_QUERIES {
                let response = retry_tikhub_rate_limit("twitter comment seed search", || async {
                    let params = TwitterSearchParams::new(*query).with_search_type(SEARCH_TYPE);
                    client.search_twitter_tweets_with_retry(&params).await
                })
                .await;

                let timeline = response
                    .data
                    .as_ref()
                    .and_then(|data| data.timeline.as_ref())
                    .cloned()
                    .unwrap_or_default();

                for tweet in timeline.into_iter().filter(is_valid_seed_tweet) {
                    let tweet_id = tweet
                        .get_tweet_id()
                        .expect("validated tweet should have an id")
                        .to_string();
                    let replies = fetch_raw_comments_paginated(
                        &client,
                        &tweet_id,
                        COMMENT_TARGET_COUNT as usize,
                    )
                    .await;
                    if replies.len() >= COMMENT_TARGET_COUNT as usize {
                        return LiveCommentSeed {
                            query: (*query).to_string(),
                            tweet_id,
                            screen_name: tweet
                                .author_handle()
                                .expect("validated tweet should have a handle")
                                .to_string(),
                            tweet,
                            replies,
                        };
                    }
                }
            }

            panic!(
                "expected at least one live Twitter tweet with {COMMENT_TARGET_COUNT} unique replies"
            )
        })
        .await
        .clone()
}

pub async fn fetch_raw_comments_paginated(
    client: &TikHubClient,
    tweet_id: &str,
    max_count: usize,
) -> Vec<TwitterTweet> {
    let mut replies = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_ids = HashSet::new();
    let mut seen_cursors = HashSet::new();

    while replies.len() < max_count {
        let response = retry_tikhub_rate_limit("twitter raw comments pagination", || async {
            let mut params = TwitterCommentParams::new(tweet_id);
            if let Some(cursor_value) = cursor.clone() {
                params = params.with_cursor(cursor_value);
            }
            client.fetch_twitter_comments_with_retry(&params).await
        })
        .await;

        if let Some(thread) = response.data.as_ref().and_then(|data| data.thread.as_ref()) {
            for reply in thread {
                let Some(reply_id) = reply.get_tweet_id() else {
                    continue;
                };
                if reply_id == tweet_id {
                    continue;
                }
                if seen_ids.insert(reply_id.to_string()) {
                    replies.push(reply.clone());
                }
                if replies.len() >= max_count {
                    break;
                }
            }
        }

        if replies.len() >= max_count {
            break;
        }

        let Some(next_cursor) = response
            .data
            .as_ref()
            .and_then(|data| data.next_cursor.clone())
        else {
            break;
        };
        if !seen_cursors.insert(next_cursor.clone()) {
            break;
        }
        cursor = Some(next_cursor);
    }

    replies
}

pub fn query_seed_from_text(text: &str) -> Option<String> {
    let words = text
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|word| word.len() >= 4)
        .take(6)
        .collect::<Vec<_>>();
    if words.len() >= 3 {
        Some(words.join(" "))
    } else {
        None
    }
}

pub fn candidate_exact_queries(tweet: &TwitterTweet) -> Vec<String> {
    let mut queries = Vec::new();
    if let Some(seed) = query_seed_from_text(tweet.content()) {
        push_unique_query(&mut queries, seed.clone());
        if let Some(handle) = tweet.author_handle() {
            push_unique_query(&mut queries, format!("{seed} from:{handle}"));
            push_unique_query(&mut queries, format!("from:{handle} {seed}"));
        }
    }
    queries
}

pub fn push_unique_query(queries: &mut Vec<String>, query: String) {
    let query = query.trim().to_string();
    if !query.is_empty() && !queries.iter().any(|existing| existing == &query) {
        queries.push(query);
    }
}

pub fn assert_content_matches_raw(content: &Content, raw: &TwitterTweet) {
    let expected_content_id = raw
        .get_tweet_id()
        .expect("raw tweet should contain a tweet id");
    let expected_url = expected_tweet_url(raw);

    assert_eq!(content.platform, "twitter");
    assert_eq!(content.content_id, expected_content_id);
    assert_eq!(content.author, raw.author_handle().unwrap_or_default());
    assert_eq!(content.author_name.as_deref(), raw.author_name());
    assert_eq!(content.description, raw.content());
    assert_eq!(content.url.as_deref(), Some(expected_url.as_str()));
    assert_eq!(content.engagement.likes, raw.like_count());
    assert_eq!(content.engagement.comments, raw.reply_count());
    assert_eq!(content.engagement.shares, raw.retweet_count());
    assert_eq!(content.engagement.views, raw.view_count());
    assert_eq!(content.created_at, raw.created_at_timestamp());

    let raw_data = content
        .raw_data
        .as_ref()
        .expect("content.raw_data should retain the upstream Twitter payload");
    let roundtrip: TwitterTweet = serde_json::from_value(raw_data.clone())
        .expect("content.raw_data should deserialize back into TwitterTweet");
    assert_eq!(roundtrip.get_tweet_id(), raw.get_tweet_id());
    assert_eq!(roundtrip.content(), raw.content());
    assert_eq!(roundtrip.author_handle(), raw.author_handle());
    assert_eq!(roundtrip.like_count(), raw.like_count());
    assert_eq!(roundtrip.reply_count(), raw.reply_count());
    assert_eq!(roundtrip.retweet_count(), raw.retweet_count());
    assert_eq!(roundtrip.view_count(), raw.view_count());
}

pub fn assert_comment_matches_raw(comment: &Comment, raw: &TwitterTweet, tweet_id: &str) {
    let expected_comment_id = raw
        .get_tweet_id()
        .expect("raw reply should contain a tweet id");

    assert_eq!(comment.platform, "twitter");
    assert_eq!(comment.comment_id, expected_comment_id);
    assert_eq!(comment.content_id, tweet_id);
    assert_eq!(
        comment.parent_id.as_deref(),
        raw.in_reply_to_status_id_str.as_deref()
    );
    assert_eq!(comment.author, raw.author_handle().unwrap_or_default());
    assert_eq!(comment.author_name.as_deref(), raw.author_name());
    assert_eq!(comment.author_uid.as_deref(), raw.user_id());
    assert_eq!(comment.text, raw.content());
    assert_eq!(comment.likes, raw.like_count());
    assert_eq!(comment.reply_count, raw.reply_count() as i32);
    assert_eq!(comment.created_at, raw.created_at_timestamp());
    assert_eq!(comment.language.as_deref(), raw.lang.as_deref());
    assert_eq!(comment.is_reply, raw.is_reply());

    let raw_data = comment
        .raw_data
        .as_ref()
        .expect("comment.raw_data should retain the upstream Twitter payload");
    let roundtrip: TwitterTweet = serde_json::from_value(raw_data.clone())
        .expect("comment.raw_data should deserialize back into TwitterTweet");
    assert_eq!(roundtrip.get_tweet_id(), raw.get_tweet_id());
    assert_eq!(roundtrip.content(), raw.content());
    assert_eq!(roundtrip.author_handle(), raw.author_handle());
    assert_eq!(
        roundtrip.in_reply_to_status_id_str,
        raw.in_reply_to_status_id_str
    );
    assert_eq!(roundtrip.like_count(), raw.like_count());
    assert_eq!(roundtrip.reply_count(), raw.reply_count());
}

pub async fn fetch_live_detail_tweet(client: &TikHubClient, tweet_id: &str) -> TwitterTweet {
    let response = retry_tikhub_rate_limit("twitter detail fetch", || async {
        client.fetch_twitter_tweet_detail_with_retry(tweet_id).await
    })
    .await;

    extract_tweet_from_detail_response(&response)
        .unwrap_or_else(|| panic!("twitter detail response should contain tweet {tweet_id}"))
}

pub async fn fetch_live_user_timeline(
    client: &TikHubClient,
    screen_name: &str,
) -> Vec<TwitterTweet> {
    let response = retry_tikhub_rate_limit("twitter user timeline fetch", || async {
        let params = TwitterUserTweetsParams::by_screen_name(screen_name);
        client.fetch_twitter_user_tweets_with_retry(&params).await
    })
    .await;

    response
        .data
        .as_ref()
        .and_then(|data| data.timeline.as_ref())
        .cloned()
        .unwrap_or_default()
}

pub async fn pick_rest_id_seed() -> LiveSearchSeed {
    let seed = pick_search_seed().await;
    let client = create_client();

    let timeline = fetch_live_user_timeline(&client, &seed.screen_name).await;
    let tweet = timeline
        .into_iter()
        .find(|t| is_valid_seed_tweet(t) && t.user_id().is_some())
        .expect("live user timeline should contain at least one tweet with user_id");
    assert!(
        tweet.user_id().is_some(),
        "seed tweet should have a user rest_id"
    );

    LiveSearchSeed {
        query: seed.query,
        tweet_id: tweet
            .get_tweet_id()
            .expect("seed tweet should have an id")
            .to_string(),
        screen_name: tweet
            .author_handle()
            .expect("seed tweet should have a handle")
            .to_string(),
        tweet,
    }
}

pub async fn fetch_live_user_timeline_by_rest_id(
    client: &TikHubClient,
    rest_id: &str,
) -> Vec<TwitterTweet> {
    let response = retry_tikhub_rate_limit("twitter user timeline by rest_id", || async {
        let params = TwitterUserTweetsParams::by_rest_id(rest_id);
        client.fetch_twitter_user_tweets_with_retry(&params).await
    })
    .await;

    response
        .data
        .as_ref()
        .and_then(|data| data.timeline.as_ref())
        .cloned()
        .unwrap_or_default()
}

fn is_valid_seed_tweet(tweet: &TwitterTweet) -> bool {
    tweet.get_tweet_id().is_some()
        && !tweet.content().trim().is_empty()
        && tweet.author_handle().is_some()
}

fn expected_tweet_url(raw: &TwitterTweet) -> String {
    let tweet_id = raw
        .get_tweet_id()
        .expect("raw tweet should contain a tweet id");
    if let Some(handle) = raw.author_handle() {
        format!("https://twitter.com/{handle}/status/{tweet_id}")
    } else {
        format!("https://twitter.com/i/web/status/{tweet_id}")
    }
}
