//! Real Twitter/TikHub API tests.
//!
//! These tests validate both:
//! 1. The live upstream TikHub Twitter contract
//! 2. The `TwitterAdapter` mapping from that contract into domain entities
//!
//! Run with:
//! `TIKHUB_API_KEY=xxx cargo test --test twitter_real_api_test -- --nocapture`

#[path = "support/twitter_live.rs"]
mod twitter_live;

use serde_json::json;

use glance_mind_agent_rs::{
    tikhub::{TwitterCommentParams, TwitterSearchParams, TwitterTweet},
    ContentGateway,
};

use twitter_live::{
    assert_comment_matches_raw, assert_content_matches_raw, create_adapter, create_client,
    create_strategy, fetch_comments_for_content, fetch_contents_for_keyword,
    fetch_live_detail_tweet, fetch_live_user_timeline, fetch_live_user_timeline_by_rest_id,
    fetch_single_comment_page, live_test_mutex, pick_comment_seed, pick_rest_id_seed,
    pick_search_seed, retry_tikhub_rate_limit, COMMENT_TARGET_COUNT,
};

const SEARCH_TYPE: &str = "Top";

fn live_api_tests_enabled() -> bool {
    let _ = dotenvy::dotenv();
    // CI opt-in: on GitHub Actions the live suites only run when RUN_REAL_API_TESTS
    // is explicitly set (D-14; CI always has credentials, so the env-skip alone
    // would never trigger there).
    if std::env::var_os("GITHUB_ACTIONS").is_some()
        && std::env::var_os("RUN_REAL_API_TESTS").is_none()
    {
        return false;
    }
    // Credential unset or empty -> skip (empty string counts as unset).
    match std::env::var("TIKHUB_API_KEY") {
        Ok(v) if !v.trim().is_empty() => true,
        _ => false,
    }
}

fn assert_tweet_contract(tweet: &TwitterTweet, label: &str) {
    assert!(
        tweet.get_tweet_id().is_some(),
        "{label} should contain tweet_id/id"
    );
    assert!(
        !tweet.content().trim().is_empty(),
        "{label} should contain non-empty text"
    );
    assert!(
        tweet.created_at.is_some(),
        "{label} should contain created_at"
    );
    assert!(
        tweet.author_handle().is_some(),
        "{label} should contain author handle"
    );
}

#[tokio::test]
async fn test_twitter_search_real_contract_and_adapter_mapping() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_search_real_contract_and_adapter_mapping - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let seed = pick_search_seed().await;
    let client = create_client();

    let raw_response = retry_tikhub_rate_limit("twitter search contract", || async {
        let params = TwitterSearchParams::new(&seed.query).with_search_type(SEARCH_TYPE);
        client.search_twitter_tweets_with_retry(&params).await
    })
    .await;
    let raw_tweet = raw_response
        .data
        .as_ref()
        .and_then(|data| data.timeline.as_ref())
        .and_then(|timeline| timeline.iter().find(|tweet| tweet.get_tweet_id().is_some()))
        .expect("live Twitter search should return at least one tweet");
    assert_tweet_contract(raw_tweet, "twitter search result");

    let adapter = create_adapter();
    let strategy = create_strategy();
    let contents = fetch_contents_for_keyword(
        &adapter,
        &strategy,
        "twitter search adapter mapping",
        &seed.query,
        1,
        &[("search_type".to_string(), json!(SEARCH_TYPE))],
    )
    .await;

    assert_eq!(contents.len(), 1, "count=1 should return exactly one tweet");
    let content = &contents[0];
    let raw_from_adapter: TwitterTweet = serde_json::from_value(
        content
            .raw_data
            .clone()
            .expect("search content should retain raw_data"),
    )
    .expect("search content raw_data should deserialize back into TwitterTweet");
    assert_tweet_contract(&raw_from_adapter, "twitter search adapter raw_data");
    assert_content_matches_raw(content, &raw_from_adapter);
}

#[tokio::test]
async fn test_twitter_user_real_contract_and_adapter_mapping() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_user_real_contract_and_adapter_mapping - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let seed = pick_search_seed().await;
    let client = create_client();

    let raw_timeline = fetch_live_user_timeline(&client, &seed.screen_name).await;
    let raw_tweet = raw_timeline
        .iter()
        .find(|tweet| tweet.get_tweet_id().is_some() && !tweet.content().trim().is_empty())
        .expect("live Twitter user timeline should return at least one tweet");
    assert_tweet_contract(raw_tweet, "twitter user timeline result");

    let adapter = create_adapter();
    let strategy = create_strategy();
    let raw_keyword = format!("twitter_handle:{}", seed.screen_name);
    let contents = fetch_contents_for_keyword(
        &adapter,
        &strategy,
        "twitter handle adapter mapping",
        &raw_keyword,
        1,
        &[],
    )
    .await;

    assert_eq!(
        contents.len(),
        1,
        "twitter handle lookup should return one tweet"
    );
    let content = &contents[0];
    let raw_from_adapter: TwitterTweet = serde_json::from_value(
        content
            .raw_data
            .clone()
            .expect("user content should retain raw_data"),
    )
    .expect("user content raw_data should deserialize back into TwitterTweet");
    assert_tweet_contract(&raw_from_adapter, "twitter handle adapter raw_data");
    assert_content_matches_raw(content, &raw_from_adapter);
}

#[tokio::test]
async fn test_twitter_detail_real_contract_and_adapter_mapping() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_detail_real_contract_and_adapter_mapping - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let seed = pick_search_seed().await;
    let client = create_client();

    let raw_detail = fetch_live_detail_tweet(&client, &seed.tweet_id).await;
    assert_tweet_contract(&raw_detail, "twitter detail result");
    assert_eq!(raw_detail.get_tweet_id(), Some(seed.tweet_id.as_str()));

    let adapter = create_adapter();
    let content = adapter
        .fetch_by_id(&seed.tweet_id)
        .await
        .expect("twitter detail adapter fetch should succeed")
        .expect("twitter detail adapter fetch should return a tweet");

    let raw_from_adapter: TwitterTweet = serde_json::from_value(
        content
            .raw_data
            .clone()
            .expect("detail content should retain raw_data"),
    )
    .expect("detail content raw_data should deserialize back into TwitterTweet");
    assert_tweet_contract(&raw_from_adapter, "twitter detail adapter raw_data");
    assert_eq!(
        raw_from_adapter.get_tweet_id(),
        Some(seed.tweet_id.as_str())
    );
    assert_content_matches_raw(&content, &raw_from_adapter);
}

#[tokio::test]
async fn test_twitter_comments_real_contract_and_adapter_mapping() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_comments_real_contract_and_adapter_mapping - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let seed = pick_comment_seed().await;
    let client = create_client();

    let raw_response = retry_tikhub_rate_limit("twitter comments contract", || async {
        let params = TwitterCommentParams::new(&seed.tweet_id);
        client.fetch_twitter_comments_with_retry(&params).await
    })
    .await;
    let raw_thread = raw_response
        .data
        .as_ref()
        .and_then(|data| data.thread.as_ref())
        .expect("twitter comments response should contain a thread");
    let first_reply = raw_thread
        .iter()
        .find(|tweet| tweet.get_tweet_id() != Some(seed.tweet_id.as_str()))
        .expect("twitter comments response should contain at least one reply");
    assert_tweet_contract(first_reply, "twitter comment reply");

    let adapter = create_adapter();
    let page =
        fetch_single_comment_page(&adapter, "twitter single comment page", &seed.tweet_id, 1).await;

    assert_eq!(
        page.comments.len(),
        1,
        "count=1 should return exactly one mapped Twitter reply"
    );
    assert_eq!(
        page.next_cursor,
        raw_response
            .data
            .as_ref()
            .and_then(|data| data.next_cursor.clone()),
        "single-page adapter call should preserve next_cursor"
    );
    assert_comment_matches_raw(&page.comments[0], first_reply, &seed.tweet_id);
}

#[tokio::test]
async fn test_twitter_comments_real_fetch_all_comments_maps_unique_replies() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_comments_real_fetch_all_comments_maps_unique_replies - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let seed = pick_comment_seed().await;
    let adapter = create_adapter();
    let comments = fetch_comments_for_content(
        &adapter,
        "twitter fetch_all_comments mapping",
        &seed.tweet_id,
        COMMENT_TARGET_COUNT,
    )
    .await;

    assert_eq!(
        comments.len(),
        COMMENT_TARGET_COUNT as usize,
        "fetch_all_comments should return {COMMENT_TARGET_COUNT} replies"
    );
    let unique_ids = comments
        .iter()
        .map(|comment| comment.comment_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_ids.len(),
        COMMENT_TARGET_COUNT as usize,
        "fetch_all_comments should deduplicate reply ids"
    );

    for comment in &comments {
        let raw: TwitterTweet = serde_json::from_value(
            comment
                .raw_data
                .clone()
                .expect("fetch_all_comments should retain raw_data"),
        )
        .expect("fetch_all comment raw_data should deserialize back into TwitterTweet");
        assert_comment_matches_raw(comment, &raw, &seed.tweet_id);
    }
}

#[tokio::test]
async fn test_twitter_rest_id_real_contract_and_adapter_mapping() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_rest_id_real_contract_and_adapter_mapping - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let seed = pick_rest_id_seed().await;
    let rest_id = seed
        .tweet
        .user_id()
        .expect("rest_id seed should have user_id")
        .to_string();
    let client = create_client();

    let raw_timeline = fetch_live_user_timeline_by_rest_id(&client, &rest_id).await;
    assert!(
        !raw_timeline.is_empty(),
        "live user timeline by rest_id should return at least one tweet"
    );
    let raw_tweet = raw_timeline
        .iter()
        .find(|t| t.get_tweet_id().is_some() && !t.content().trim().is_empty())
        .expect("timeline by rest_id should contain at least one valid tweet");
    assert_tweet_contract(raw_tweet, "twitter rest_id timeline result");

    let adapter = create_adapter();
    let strategy = create_strategy();
    let raw_keyword = format!("twitter_rest_id:{rest_id}");
    let contents = fetch_contents_for_keyword(
        &adapter,
        &strategy,
        "twitter rest_id adapter mapping",
        &raw_keyword,
        1,
        &[],
    )
    .await;

    assert_eq!(
        contents.len(),
        1,
        "twitter rest_id lookup should return one tweet"
    );
    let content = &contents[0];
    let raw_from_adapter: TwitterTweet = serde_json::from_value(
        content
            .raw_data
            .clone()
            .expect("rest_id content should retain raw_data"),
    )
    .expect("rest_id content raw_data should deserialize back into TwitterTweet");
    assert_tweet_contract(&raw_from_adapter, "twitter rest_id adapter raw_data");
    assert_content_matches_raw(content, &raw_from_adapter);
}
