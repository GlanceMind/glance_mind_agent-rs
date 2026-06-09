//! RED pagination tests for the TikHub USER/PAGE video path (T3).
//!
//! These exercise the REAL HTTP path: a mockito server scripts the TikHub
//! `/api/v1/tiktok/app/v3/fetch_user_post_videos` responses, and the test
//! drives the public `ContentGateway::fetch_by_keyword` API.
//!
//! TikHub's user-videos endpoint returns at most 20 items per request, so
//! satisfying a `SearchOptions` count of 40/100 REQUIRES the adapter to
//! paginate (a `max_cursor` loop). The current single-fetch implementation in
//! `fetch_user_content` / the `SecUserId` branch does NOT paginate, so these
//! tests are expected to FAIL until the pagination fix lands. That RED state
//! is the point of this file.
//!
//! Pagination here advances via `max_cursor` (NOT offset): page1's request
//! carries `max_cursor=0`; page2's request carries the `max_cursor` value
//! returned by page1's response body.

use glance_mind_agent_rs::domain::{KeywordType, SearchOptions};
use glance_mind_agent_rs::ports::ContentGateway;
use glance_mind_agent_rs::{TikHubAdapter, TikHubClient};

use mockito::Matcher;

const USER_VIDEOS_PATH: &str = "/api/v1/tiktok/app/v3/fetch_user_post_videos";

/// Build a TikHub user-videos response JSON body.
///
/// User videos put `AwemeInfo` DIRECTLY in `aweme_list` (NOT wrapped in
/// `{aweme_info: ...}` like the search endpoint). `ids` become the minimal
/// valid item shape `{ "aweme_id": id }`.
fn user_videos_body(ids: &[&str], has_more: i32, max_cursor: i64) -> String {
    let items: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| serde_json::json!({ "aweme_id": id }))
        .collect();

    serde_json::json!({
        "code": 200,
        "message": "success",
        "data": {
            "aweme_list": items,
            "has_more": has_more,
            "max_cursor": max_cursor,
        }
    })
    .to_string()
}

/// Generate `count` distinct ids with the given prefix, e.g. `ids("p1", 20)`.
fn ids(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{prefix}-{i}")).collect()
}

fn as_refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

fn adapter_for(server: &mockito::Server) -> TikHubAdapter {
    let client = TikHubClient::new("test-key", server.url()).expect("client builds");
    TikHubAdapter::new(client)
}

// ---------------------------------------------------------------------------
// U1: unique_id path paginates. page1 (max_cursor=0) -> 20, has_more=1,
// response max_cursor=100; page2 (max_cursor=100) -> 20, has_more=0.
// count=40 -> 40 items.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn u1_unique_id_paginates_two_pages_returns_40() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);

    let _m1 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::UrlEncoded("max_cursor".into(), "0".into()))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page1), 1, 100))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::UrlEncoded("max_cursor".into(), "100".into()))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page2), 0, 200))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("alice").with_count(40).with_region("US");

    let result = adapter
        .fetch_by_keyword(&KeywordType::UserId("alice".into()), &options)
        .await
        .expect("fetch succeeds");

    assert_eq!(result.len(), 40);
}

// ---------------------------------------------------------------------------
// U2: per-request count must be <= 20 (TikHub's cap). Same two-page script,
// but BOTH mocks require count=20 in addition to their max_cursor, each
// expecting exactly one hit. A request with count>20 would not match.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn u2_per_request_count_never_exceeds_20() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);

    let m1 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("max_cursor".into(), "0".into()),
            Matcher::UrlEncoded("count".into(), "20".into()),
        ]))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page1), 1, 100))
        .expect(1)
        .create_async()
        .await;

    let m2 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("max_cursor".into(), "100".into()),
            Matcher::UrlEncoded("count".into(), "20".into()),
        ]))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page2), 0, 200))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("alice").with_count(40).with_region("US");

    let result = adapter
        .fetch_by_keyword(&KeywordType::UserId("alice".into()), &options)
        .await
        .expect("fetch succeeds");

    assert_eq!(result.len(), 40);
    // Each page fetched exactly once, each with count=20.
    m1.assert_async().await;
    m2.assert_async().await;
}

// ---------------------------------------------------------------------------
// U3: sec_user_id path ALSO paginates. page1 (max_cursor=0) -> 20, has_more=1,
// response max_cursor=100; page2 (max_cursor=100) -> 20, has_more=0.
// count=40 -> 40 items. Proves the SecUserId branch is routed through
// pagination too.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn u3_sec_user_id_paginates_two_pages_returns_40() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);

    let _m1 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::UrlEncoded("max_cursor".into(), "0".into()))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page1), 1, 100))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::UrlEncoded("max_cursor".into(), "100".into()))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page2), 0, 200))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("x").with_count(40).with_region("US");

    let result = adapter
        .fetch_by_keyword(&KeywordType::SecUserId("MS4wLjABAAAA".into()), &options)
        .await
        .expect("fetch succeeds");

    assert_eq!(result.len(), 40);
}

// ---------------------------------------------------------------------------
// U4: an empty second page terminates pagination even when has_more claims
// otherwise. page1 (max_cursor=0) -> 20, has_more=1, max_cursor=100;
// page2 (max_cursor=100) -> empty aweme_list, has_more=1. count=100 -> 20.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn u4_empty_second_page_terminates_at_20() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    let _m1 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::UrlEncoded("max_cursor".into(), "0".into()))
        .with_status(200)
        .with_body(user_videos_body(&as_refs(&page1), 1, 100))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", USER_VIDEOS_PATH)
        .match_query(Matcher::UrlEncoded("max_cursor".into(), "100".into()))
        .with_status(200)
        .with_body(user_videos_body(&[], 1, 200))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("alice")
        .with_count(100)
        .with_region("US");

    let result = adapter
        .fetch_by_keyword(&KeywordType::UserId("alice".into()), &options)
        .await
        .expect("fetch succeeds");

    assert_eq!(result.len(), 20);
}
