//! Real Facebook RapidAPI tests.
//!
//! These tests validate both:
//! 1. The live upstream RapidAPI contract for `facebook-scraper3`
//! 2. The `FacebookAdapter` mapping from that contract into domain entities
//!
//! Run with:
//! `FACEBOOK_RAPIDAPI_KEY=xxx cargo test --test facebook_real_api_test -- --nocapture`

use std::sync::OnceLock;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};

use glance_mind_agent_rs::{
    ports::comment_gateway::FetchCommentsOptions, CommentGateway, ContentGateway, FacebookAdapter,
    FacebookStrategy, KeywordType, PlatformStrategy, SearchOptions, TaskConfig,
};

const DEFAULT_HOST: &str = "facebook-scraper3.p.rapidapi.com";
const DEFAULT_BASE_URL: &str = "https://facebook-scraper3.p.rapidapi.com";
const POSTS_QUERY: &str = "china travel";
const POSTS_LOCATION: &str = "beijing,china";
const PAGE_QUERY_TEXT: &str = "National Geographic Museum";
const PAGE_QUERY_WITH_LOCATION: &str = "National Geographic Museum washington,usa";
const PAGE_ID: &str = "100064881934421";
const PLACE_QUERY: &str = "beijing china";
const PLACE_QUERY_WITH_LOCATION: &str = "beijing china beijing,china";
const POST_URL: &str = "https://www.facebook.com/NatGeoMuseum/posts/pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl";
const POST_LOOKUP_ID: &str =
    "pfbid02MmmxmHinoAbb2Aidf7TZHH1fSR4w8UmPYUXKT86HgHFAHryrD54bW5113ZPQ2gzYl";
const POST_ID: &str = "1431426125696772";
const COMMENT_RICH_POST_CANDIDATES: &[&str] = &[POST_ID, "859693417025820", "1134687898785891"];

struct RapidApiConfig {
    api_key: String,
    api_host: String,
    base_url: String,
}

fn live_test_mutex() -> &'static Mutex<()> {
    static LIVE_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    LIVE_TEST_MUTEX.get_or_init(|| Mutex::new(()))
}

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
    match std::env::var("FACEBOOK_RAPIDAPI_KEY") {
        Ok(v) if !v.trim().is_empty() => true,
        _ => false,
    }
}

fn require_rapidapi_config() -> RapidApiConfig {
    let _ = dotenvy::dotenv();

    let api_key = std::env::var("FACEBOOK_RAPIDAPI_KEY")
        .expect("FACEBOOK_RAPIDAPI_KEY must be set for real Facebook RapidAPI tests");
    let api_host =
        std::env::var("FACEBOOK_RAPIDAPI_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let base_url = std::env::var("FACEBOOK_RAPIDAPI_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

    RapidApiConfig {
        api_key,
        api_host,
        base_url,
    }
}

fn create_adapter() -> FacebookAdapter {
    FacebookAdapter::from_env()
        .expect("FacebookAdapter::from_env should succeed for real Facebook RapidAPI tests")
}

fn build_config<I, S>(extra_pairs: I) -> TaskConfig
where
    I: IntoIterator<Item = (S, Value)>,
    S: Into<String>,
{
    build_config_with_limits(1, 5, extra_pairs)
}

fn build_config_with_limits<I, S>(
    max_videos: i32,
    max_comments_per_video: i32,
    extra_pairs: I,
) -> TaskConfig
where
    I: IntoIterator<Item = (S, Value)>,
    S: Into<String>,
{
    let mut config = TaskConfig::new(1, "facebook")
        .with_region("US")
        .with_max_videos(max_videos)
        .with_max_comments_per_video(max_comments_per_video);

    for (key, value) in extra_pairs {
        config.extra.insert(key.into(), value);
    }

    config
}

fn build_options<I, S>(
    strategy: &FacebookStrategy,
    raw_keyword: &str,
    extra_pairs: I,
) -> (KeywordType, SearchOptions)
where
    I: IntoIterator<Item = (S, Value)>,
    S: Into<String>,
{
    let config = build_config(extra_pairs);
    let keyword = strategy.parse_keyword(raw_keyword);
    let options = strategy.build_search_options(&config, &keyword);
    (keyword, options)
}

fn build_options_with_limits<I, S>(
    strategy: &FacebookStrategy,
    raw_keyword: &str,
    max_videos: i32,
    max_comments_per_video: i32,
    extra_pairs: I,
) -> (KeywordType, SearchOptions)
where
    I: IntoIterator<Item = (S, Value)>,
    S: Into<String>,
{
    let config = build_config_with_limits(max_videos, max_comments_per_video, extra_pairs);
    let keyword = strategy.parse_keyword(raw_keyword);
    let options = strategy.build_search_options(&config, &keyword);
    (keyword, options)
}

fn json_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_u64().map(|v| v as i64)),
        Some(Value::String(number)) => number.parse().ok(),
        _ => None,
    }
}

fn non_empty_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
}

fn assert_required_keys(value: &Value, keys: &[&str], label: &str) {
    for key in keys {
        assert!(
            value.get(*key).is_some(),
            "{label} should contain key `{key}`, got payload: {value}"
        );
    }
}

fn assert_list_results<'a>(body: &'a Value, endpoint: &str) -> &'a Vec<Value> {
    let results = body
        .get("results")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{endpoint} should return a `results` array: {body}"));
    assert!(
        !results.is_empty(),
        "{endpoint} should return at least one result: {body}"
    );
    assert!(
        body.get("cursor").is_some(),
        "{endpoint} should return a `cursor` field, even if it is null: {body}"
    );
    results
}

fn assert_object_result<'a>(body: &'a Value, endpoint: &str) -> &'a Value {
    body.get("results")
        .unwrap_or_else(|| panic!("{endpoint} should return a `results` object: {body}"))
}

fn assert_live_counter_close(label: &str, actual: i64, expected: i64) {
    let tolerance = 5.max(expected.abs() / 100);
    let delta = (actual - expected).abs();
    assert!(
        delta <= tolerance,
        "{label} drifted too far between live API calls: actual={actual}, expected={expected}, tolerance={tolerance}"
    );
}

fn assert_content_matches_raw(content: &glance_mind_agent_rs::Content, raw: &Value) {
    let raw_author = raw.get("author");
    let expected_author = raw_author
        .and_then(|author| author.get("id"))
        .and_then(Value::as_str)
        .or_else(|| {
            raw_author
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();

    assert_eq!(content.platform, "facebook");
    assert_eq!(
        content.content_id,
        raw.get("post_id")
            .and_then(Value::as_str)
            .expect("raw post should contain post_id")
    );
    assert_eq!(content.author, expected_author);
    assert_eq!(
        content.author_name.as_deref(),
        raw_author
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
    );
    assert_eq!(
        content.description,
        non_empty_string(raw, "message")
            .or_else(|| non_empty_string(raw, "message_rich"))
            .unwrap_or_default()
    );
    assert_eq!(
        content.url.as_deref(),
        raw.get("url").and_then(Value::as_str)
    );
    assert_eq!(content.created_at, json_i64(raw.get("timestamp")));

    let raw_data = content
        .raw_data
        .as_ref()
        .expect("content.raw_data should retain the upstream Facebook payload");
    let adapter_likes = json_i64(raw_data.get("reactions_count")).unwrap_or_default();
    let adapter_comments = json_i64(raw_data.get("comments_count")).unwrap_or_default();
    let adapter_shares = json_i64(raw_data.get("reshare_count")).unwrap_or_default();
    assert_eq!(content.engagement.likes, adapter_likes);
    assert_eq!(content.engagement.comments, adapter_comments);
    assert_eq!(content.engagement.shares, adapter_shares);
    assert_live_counter_close(
        "reactions_count",
        adapter_likes,
        json_i64(raw.get("reactions_count")).unwrap_or_default(),
    );
    assert_live_counter_close(
        "comments_count",
        adapter_comments,
        json_i64(raw.get("comments_count")).unwrap_or_default(),
    );
    assert_live_counter_close(
        "reshare_count",
        adapter_shares,
        json_i64(raw.get("reshare_count")).unwrap_or_default(),
    );
    assert_eq!(raw_data.get("post_id"), raw.get("post_id"));
    assert_eq!(raw_data.get("url"), raw.get("url"));
    assert_eq!(raw_data.get("type"), raw.get("type"));
    assert_eq!(raw_data.get("message"), raw.get("message"));
    assert_eq!(raw_data.get("message_rich"), raw.get("message_rich"));
    assert_eq!(raw_data.get("timestamp"), raw.get("timestamp"));
    assert_eq!(
        raw_data
            .get("author")
            .and_then(|author| author.get("id"))
            .and_then(Value::as_str),
        raw.get("author")
            .and_then(|author| author.get("id"))
            .and_then(Value::as_str)
    );
    assert_eq!(
        raw_data
            .get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str),
        raw.get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
    );
}

fn assert_comment_matches_raw(comment: &glance_mind_agent_rs::Comment, raw: &Value, post_id: &str) {
    let raw_author = raw.get("author");
    let expected_comment_id = raw
        .get("legacy_comment_id")
        .and_then(Value::as_str)
        .or_else(|| raw.get("comment_id").and_then(Value::as_str))
        .expect("raw comment should have legacy_comment_id or comment_id");
    let expected_author = raw_author
        .and_then(|author| author.get("id"))
        .and_then(Value::as_str)
        .or_else(|| {
            raw_author
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();

    assert_eq!(comment.platform, "facebook");
    assert_eq!(comment.comment_id, expected_comment_id);
    assert_eq!(comment.content_id, post_id);
    assert_eq!(
        comment.parent_id.as_deref(),
        raw.get("parent_comment_id").and_then(Value::as_str)
    );
    assert_eq!(
        comment.text,
        raw.get("message")
            .and_then(Value::as_str)
            .or_else(|| {
                raw.get("sticker")
                    .and_then(|sticker| sticker.get("label"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("[facebook comment without text]")
    );
    assert_eq!(comment.author, expected_author);
    assert_eq!(
        comment.author_uid.as_deref(),
        raw_author
            .and_then(|author| author.get("id"))
            .and_then(Value::as_str)
    );
    assert_eq!(
        comment.author_name.as_deref(),
        raw_author
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
    );
    assert_eq!(
        comment.likes,
        json_i64(raw.get("reactions_count")).unwrap_or_default()
    );
    assert_eq!(
        comment.reply_count,
        json_i64(raw.get("replies_count")).unwrap_or_default() as i32
    );
    assert_eq!(comment.created_at, json_i64(raw.get("created_time")));
    assert_eq!(
        comment.is_reply,
        json_i64(raw.get("depth")).unwrap_or_default() > 0
    );

    let raw_data = comment
        .raw_data
        .as_ref()
        .expect("comment.raw_data should retain the upstream Facebook payload");
    assert_eq!(raw_data.get("comment_id"), raw.get("comment_id"));
    assert_eq!(
        raw_data.get("legacy_comment_id"),
        raw.get("legacy_comment_id")
    );
    assert_eq!(raw_data.get("message"), raw.get("message"));
    assert_eq!(raw_data.get("created_time"), raw.get("created_time"));
    assert_eq!(raw_data.get("depth"), raw.get("depth"));
    assert_eq!(
        raw_data.get("parent_comment_id"),
        raw.get("parent_comment_id")
    );
    assert_eq!(raw_data.get("reactions_count"), raw.get("reactions_count"));
    assert_eq!(raw_data.get("replies_count"), raw.get("replies_count"));
    assert_eq!(
        raw_data
            .get("author")
            .and_then(|author| author.get("id"))
            .and_then(Value::as_str),
        raw.get("author")
            .and_then(|author| author.get("id"))
            .and_then(Value::as_str)
    );
    assert_eq!(
        raw_data
            .get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str),
        raw.get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
    );
}

fn surrounding_date_window(timestamp: i64) -> (String, String) {
    let date = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .expect("timestamp should be valid")
        .date_naive();
    let start = date
        .checked_sub_signed(ChronoDuration::days(7))
        .unwrap_or(date);
    let end = date
        .checked_add_signed(ChronoDuration::days(7))
        .unwrap_or(date);
    (
        start.format("%Y-%m-%d").to_string(),
        end.format("%Y-%m-%d").to_string(),
    )
}

fn parse_date_boundary(date: &str, end_of_day: bool) -> i64 {
    let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .expect("date should be formatted as YYYY-MM-DD");
    let naive = if end_of_day {
        date.and_hms_opt(23, 59, 59)
    } else {
        date.and_hms_opt(0, 0, 0)
    }
    .expect("date boundary should be valid");
    Utc.from_utc_datetime(&naive).timestamp()
}

fn assert_contents_sorted_by_timestamp_desc(contents: &[glance_mind_agent_rs::Content]) {
    let timestamps = contents
        .iter()
        .map(|content| {
            content
                .created_at
                .expect("all filtered Facebook contents should have timestamps")
        })
        .collect::<Vec<_>>();
    let mut sorted = timestamps.clone();
    sorted.sort_by(|left, right| right.cmp(left));
    assert_eq!(
        timestamps, sorted,
        "facebook contents should be sorted by newest timestamp first"
    );
}

fn assert_contents_within_date_range(
    contents: &[glance_mind_agent_rs::Content],
    start_date: &str,
    end_date: &str,
) {
    let start_ts = parse_date_boundary(start_date, false);
    let end_ts = parse_date_boundary(end_date, true);
    for content in contents {
        let timestamp = content
            .created_at
            .expect("filtered Facebook contents should have timestamps");
        assert!(
            timestamp >= start_ts && timestamp <= end_ts,
            "content {} timestamp {} should be within [{}, {}]",
            content.content_id,
            timestamp,
            start_ts,
            end_ts
        );
    }
}

async fn fetch_raw_json(path: &str, params: &[(&str, &str)]) -> Value {
    let config = require_rapidapi_config();
    let client = Client::new();
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), path);

    for fallback_delay_secs in [2_u64, 5, 10] {
        let response = client
            .get(&url)
            .query(params)
            .header("Content-Type", "application/json")
            .header("x-rapidapi-host", &config.api_host)
            .header("x-rapidapi-key", &config.api_key)
            .send()
            .await
            .expect("live Facebook RapidAPI request should succeed");

        let status = response.status();
        let retry_after_secs = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let body_text = response
            .text()
            .await
            .expect("live Facebook RapidAPI response body should be readable");
        let body: Value = serde_json::from_str(&body_text)
            .unwrap_or_else(|err| panic!("response should be valid JSON: {err}; body={body_text}"));

        if status == StatusCode::TOO_MANY_REQUESTS {
            sleep(Duration::from_secs(
                retry_after_secs.unwrap_or(fallback_delay_secs),
            ))
            .await;
            continue;
        }

        assert_eq!(
            status,
            StatusCode::OK,
            "live Facebook RapidAPI request to {path} failed: {body}"
        );

        return body;
    }

    let response = client
        .get(url)
        .query(params)
        .header("Content-Type", "application/json")
        .header("x-rapidapi-host", &config.api_host)
        .header("x-rapidapi-key", &config.api_key)
        .send()
        .await
        .expect("live Facebook RapidAPI retry request should succeed");
    let status = response.status();
    let body_text = response
        .text()
        .await
        .expect("live Facebook RapidAPI retry response body should be readable");
    let body: Value = serde_json::from_str(&body_text)
        .unwrap_or_else(|err| panic!("response should be valid JSON: {err}; body={body_text}"));
    assert_eq!(
        status,
        StatusCode::OK,
        "live Facebook RapidAPI request to {path} failed after retries: {body}"
    );
    body
}

fn raw_comment_id(raw: &Value) -> Option<&str> {
    raw.get("legacy_comment_id")
        .and_then(Value::as_str)
        .or_else(|| raw.get("comment_id").and_then(Value::as_str))
}

async fn fetch_raw_post_object(post_id: &str) -> Value {
    let raw_post_body = fetch_raw_json("/post", &[("post_id", post_id)]).await;
    assert_object_result(&raw_post_body, "/post").clone()
}

async fn fetch_raw_comments_paginated(post_id: &str, max_count: usize) -> Vec<Value> {
    let mut comments = Vec::new();
    let mut seen_comment_ids = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = std::collections::HashSet::new();
    let mut empty_hops = 0;

    while comments.len() < max_count {
        let mut params = vec![("post_id", post_id)];
        if let Some(ref cursor_value) = cursor {
            params.push(("cursor", cursor_value.as_str()));
        }
        let body = fetch_raw_json("/post/comments", &params).await;
        let page_comments = body
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if page_comments.is_empty() {
            empty_hops += 1;
            if empty_hops >= 2 {
                break;
            }
        } else {
            empty_hops = 0;
        }

        for raw_comment in page_comments {
            if let Some(comment_id) = raw_comment_id(&raw_comment) {
                if seen_comment_ids.insert(comment_id.to_string()) {
                    comments.push(raw_comment);
                }
            }
            if comments.len() >= max_count {
                break;
            }
        }

        let Some(next_cursor) = body.get("cursor").and_then(Value::as_str) else {
            break;
        };
        if !seen_cursors.insert(next_cursor.to_string()) {
            break;
        }
        cursor = Some(next_cursor.to_string());
    }

    comments
}

async fn pick_live_post_with_comments() -> (Value, Vec<Value>) {
    for post_id in COMMENT_RICH_POST_CANDIDATES {
        let raw_post = fetch_raw_post_object(post_id).await;
        let raw_comments = fetch_raw_comments_paginated(post_id, 5).await;
        if raw_comments.len() >= 5 {
            return (raw_post, raw_comments);
        }
    }

    panic!("expected at least one live Facebook post candidate to expose five comments");
}

#[tokio::test]
async fn test_facebook_keyword_posts_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_keyword_posts_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_body = fetch_raw_json("/search/posts", &[("query", POSTS_QUERY)]).await;
    let raw_posts = assert_list_results(&raw_body, "/search/posts");
    let first_raw_post = &raw_posts[0];
    assert_required_keys(
        first_raw_post,
        &[
            "post_id",
            "author",
            "message",
            "message_rich",
            "timestamp",
            "comments_count",
            "reactions_count",
            "reshare_count",
            "type",
            "url",
        ],
        "/search/posts first result",
    );
    assert_required_keys(
        first_raw_post.get("author").expect("author should exist"),
        &["id", "name", "url"],
        "/search/posts first result author",
    );

    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options(
        &strategy,
        POSTS_QUERY,
        vec![("search_type", json!("posts"))],
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook keyword/posts search should succeed");

    assert_eq!(contents.len(), 1, "count=1 should return exactly one post");
    let raw_post = contents[0]
        .raw_data
        .as_ref()
        .expect("facebook keyword/posts result should retain raw payload");
    assert_content_matches_raw(&contents[0], raw_post);
}

#[tokio::test]
async fn test_facebook_keyword_posts_real_paginates_search_results() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_keyword_posts_real_paginates_search_results - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options_with_limits(
        &strategy,
        POSTS_QUERY,
        6,
        5,
        vec![("search_type", json!("posts"))],
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook keyword/posts pagination should succeed");

    assert_eq!(
        contents.len(),
        6,
        "search/posts should paginate until six posts are collected"
    );
    let unique_ids = contents
        .iter()
        .map(|content| content.content_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_ids.len(),
        6,
        "paginated search/posts should deduplicate posts"
    );
}

#[tokio::test]
async fn test_facebook_keyword_posts_real_covers_all_search_params() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_keyword_posts_real_covers_all_search_params - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_body =
        fetch_raw_json("/search/posts", &[("query", "china travel beijing,china")]).await;
    let raw_posts = assert_list_results(&raw_body, "/search/posts");
    let target_timestamp = raw_posts
        .iter()
        .find_map(|post| json_i64(post.get("timestamp")))
        .expect("live /search/posts should return a timestamped post for parameter coverage");
    let (start_date, end_date) = surrounding_date_window(target_timestamp);

    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options_with_limits(
        &strategy,
        POSTS_QUERY,
        3,
        5,
        vec![
            ("search_type", json!("posts")),
            ("location", json!(POSTS_LOCATION)),
            ("recent_posts", json!(true)),
            ("start_date", json!(start_date.clone())),
            ("end_date", json!(end_date.clone())),
        ],
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook keyword/posts search with all search params should succeed");

    assert!(
        !contents.is_empty(),
        "facebook keyword/posts search with all params should return at least one content"
    );
    assert_contents_sorted_by_timestamp_desc(&contents);
    assert_contents_within_date_range(&contents, &start_date, &end_date);

    let raw_post = contents[0]
        .raw_data
        .as_ref()
        .expect("facebook keyword/posts search should retain raw payload");
    assert_content_matches_raw(&contents[0], raw_post);
}

#[tokio::test]
async fn test_facebook_keyword_pages_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_keyword_pages_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_pages_body =
        fetch_raw_json("/search/pages", &[("query", PAGE_QUERY_WITH_LOCATION)]).await;
    let raw_pages = assert_list_results(&raw_pages_body, "/search/pages");
    let first_page = &raw_pages[0];
    assert_required_keys(
        first_page,
        &["facebook_id", "name", "profile_url", "url", "type"],
        "/search/pages first result",
    );
    assert_eq!(
        first_page.get("facebook_id").and_then(Value::as_str),
        Some(PAGE_ID),
        "the exact page query should resolve to the expected NatGeoMuseum page"
    );

    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options(
        &strategy,
        PAGE_QUERY_TEXT,
        vec![
            ("search_type", json!("pages")),
            ("recent_posts", json!(true)),
            ("location", json!("washington,usa")),
        ],
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook keyword/pages search should succeed");

    assert_eq!(contents.len(), 1, "count=1 should return exactly one post");
    let raw_post = contents[0]
        .raw_data
        .as_ref()
        .expect("facebook keyword/pages search should retain raw payload");
    assert_content_matches_raw(&contents[0], raw_post);
}

#[tokio::test]
async fn test_facebook_keyword_pages_real_paginates_page_posts() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_keyword_pages_real_paginates_page_posts - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options_with_limits(
        &strategy,
        PAGE_QUERY_TEXT,
        4,
        5,
        vec![
            ("search_type", json!("pages")),
            ("recent_posts", json!(true)),
            ("location", json!("washington,usa")),
        ],
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook keyword/pages pagination should succeed");

    assert_eq!(
        contents.len(),
        4,
        "pages input should paginate page/posts until four posts are collected"
    );
    let unique_ids = contents
        .iter()
        .map(|content| content.content_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_ids.len(),
        4,
        "pages input pagination should return four unique posts"
    );
}

#[tokio::test]
async fn test_facebook_keyword_places_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_keyword_places_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_places_body =
        fetch_raw_json("/search/places", &[("query", PLACE_QUERY_WITH_LOCATION)]).await;
    let raw_places = assert_list_results(&raw_places_body, "/search/places");
    assert_required_keys(
        &raw_places[0],
        &["facebook_id", "name", "profile_url", "url", "type"],
        "/search/places first result",
    );

    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options(
        &strategy,
        PLACE_QUERY,
        vec![
            ("search_type", json!("places")),
            ("location", json!("beijing,china")),
        ],
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook keyword/places search should succeed");

    assert_eq!(contents.len(), 1, "count=1 should return exactly one post");
    let raw_post = contents[0]
        .raw_data
        .as_ref()
        .expect("facebook keyword/places search should retain raw payload");
    assert_content_matches_raw(&contents[0], raw_post);
}

#[tokio::test]
async fn test_facebook_page_real_text_and_numeric() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_page_real_text_and_numeric - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let page_posts_body = fetch_raw_json("/page/posts", &[("page_id", PAGE_ID)]).await;
    let page_posts = assert_list_results(&page_posts_body, "/page/posts");
    let expected_post = &page_posts[0];
    assert_required_keys(
        expected_post,
        &[
            "post_id",
            "author",
            "message",
            "timestamp",
            "comments_count",
            "reactions_count",
            "reshare_count",
            "url",
        ],
        "/page/posts first result",
    );

    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();

    let (text_keyword, text_options) = build_options(
        &strategy,
        "facebook_page:National Geographic Museum",
        Vec::<(&str, Value)>::new(),
    );
    let text_contents = adapter
        .fetch_by_keyword(&text_keyword, &text_options)
        .await
        .expect("facebook page text lookup should succeed");
    assert_eq!(text_contents.len(), 1);
    assert_content_matches_raw(&text_contents[0], expected_post);

    let (numeric_keyword, numeric_options) = build_options(
        &strategy,
        &format!("facebook_page:{PAGE_ID}"),
        Vec::<(&str, Value)>::new(),
    );
    let numeric_contents = adapter
        .fetch_by_keyword(&numeric_keyword, &numeric_options)
        .await
        .expect("facebook page numeric lookup should succeed");
    assert_eq!(numeric_contents.len(), 1);
    assert_content_matches_raw(&numeric_contents[0], expected_post);
    assert_eq!(
        text_contents[0].content_id, numeric_contents[0].content_id,
        "page text and numeric lookups should resolve to the same first page post"
    );
}

#[tokio::test]
async fn test_facebook_post_url_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_post_url_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_post_body = fetch_raw_json("/post", &[("post_id", POST_LOOKUP_ID)]).await;
    let raw_post = assert_object_result(&raw_post_body, "/post");
    assert_required_keys(
        raw_post,
        &[
            "post_id",
            "author",
            "message",
            "message_rich",
            "timestamp",
            "comments_count",
            "reactions_count",
            "reshare_count",
            "url",
        ],
        "/post result",
    );
    assert_eq!(
        raw_post.get("post_id").and_then(Value::as_str),
        Some(POST_ID)
    );
    assert!(
        raw_post
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| url.starts_with("https://www.facebook.com/NatGeoMuseum/posts/")),
        "post lookup should return a NatGeoMuseum Facebook post URL: {raw_post}"
    );

    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let (keyword, options) = build_options(
        &strategy,
        &format!("facebook_post_url:{POST_URL}"),
        Vec::<(&str, Value)>::new(),
    );

    assert_eq!(
        options.query, POST_LOOKUP_ID,
        "post URL should be converted to the RapidAPI lookup token"
    );

    let contents = adapter
        .fetch_by_keyword(&keyword, &options)
        .await
        .expect("facebook post URL lookup should succeed");

    assert_eq!(contents.len(), 1);
    assert_content_matches_raw(&contents[0], raw_post);
}

#[tokio::test]
async fn test_facebook_post_fetch_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_post_fetch_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_post_body = fetch_raw_json("/post", &[("post_id", POST_ID)]).await;
    let raw_post = assert_object_result(&raw_post_body, "/post");

    let adapter = create_adapter();
    let content = adapter
        .fetch_by_id(POST_ID)
        .await
        .expect("facebook direct post fetch should succeed")
        .expect("facebook direct post fetch should return a post");

    assert_content_matches_raw(&content, raw_post);
    assert_eq!(content.content_id, POST_ID);
    assert_eq!(
        content.url.as_deref(),
        raw_post.get("url").and_then(Value::as_str)
    );
}

#[tokio::test]
async fn test_facebook_post_comments_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_post_comments_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let raw_comments_body = fetch_raw_json("/post/comments", &[("post_id", POST_ID)]).await;
    let raw_comments = assert_list_results(&raw_comments_body, "/post/comments");
    let first_raw_comment = &raw_comments[0];
    assert_required_keys(
        first_raw_comment,
        &[
            "comment_id",
            "legacy_comment_id",
            "author",
            "created_time",
            "depth",
            "parent_comment_id",
            "reactions_count",
            "replies_count",
            "type",
        ],
        "/post/comments first result",
    );
    assert_required_keys(
        first_raw_comment
            .get("author")
            .expect("comment author should exist"),
        &["id", "name", "url", "profile_image"],
        "/post/comments first result author",
    );

    let adapter = create_adapter();
    let result = adapter
        .fetch_comments(POST_ID, &FetchCommentsOptions::new(1))
        .await
        .expect("facebook post/comments should succeed");

    assert_eq!(
        result.comments.len(),
        1,
        "count=1 should return exactly one Facebook comment"
    );
    assert!(
        result.next_cursor.is_some(),
        "post/comments should expose a cursor so the adapter can paginate comments"
    );
    assert_comment_matches_raw(&result.comments[0], first_raw_comment, POST_ID);
}

#[tokio::test]
async fn test_facebook_post_comments_real_fetch_all_comments() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_facebook_post_comments_real_fetch_all_comments - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let (raw_post, raw_comments) = pick_live_post_with_comments().await;
    let post_id = raw_post
        .get("post_id")
        .and_then(Value::as_str)
        .expect("selected live raw post should expose post_id");

    let adapter = create_adapter();
    let comments = adapter
        .fetch_all_comments(post_id, 5)
        .await
        .expect("facebook fetch_all_comments should succeed");

    assert_eq!(
        comments.len(),
        5,
        "fetch_all_comments should return five live Facebook comments"
    );
    let unique_ids = comments
        .iter()
        .map(|comment| comment.comment_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_ids.len(),
        5,
        "fetch_all_comments should deduplicate live comments"
    );

    for comment in &comments {
        let raw = raw_comments
            .iter()
            .find(|raw| {
                raw.get("legacy_comment_id")
                    .and_then(Value::as_str)
                    .or_else(|| raw.get("comment_id").and_then(Value::as_str))
                    == Some(comment.comment_id.as_str())
            })
            .expect("every adapter comment should map back to a raw /post/comments entry");
        assert_comment_matches_raw(comment, raw, post_id);
    }
}
