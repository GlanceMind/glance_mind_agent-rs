//! RED pagination tests for `TikHubAdapter::search` (T1).
//!
//! These exercise the REAL HTTP path: a mockito server scripts the TikHub
//! `/api/v1/tiktok/app/v3/fetch_video_search_result` responses, and the test
//! drives the public `ContentGateway::search` API.
//!
//! TikHub's search endpoint returns at most 20 items per request, so satisfying
//! a `SearchOptions` count of 40/100 REQUIRES the adapter to paginate
//! (offset/cursor loop). The current single-fetch implementation does NOT, so
//! these tests are expected to FAIL until the pagination fix lands. That RED
//! state is the point of this file.

use std::collections::HashSet;

use glance_mind_agent_rs::domain::SearchOptions;
use glance_mind_agent_rs::ports::ContentGateway;
use glance_mind_agent_rs::{TikHubAdapter, TikHubClient};

use mockito::Matcher;

const SEARCH_PATH: &str = "/api/v1/tiktok/app/v3/fetch_video_search_result";

/// Build a TikHub search-response JSON body.
///
/// `ids` become `aweme_info.aweme_id` values (the minimal valid item shape).
fn search_body(ids: &[&str], has_more: i32, cursor: i64) -> String {
    let items: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| serde_json::json!({ "aweme_info": { "aweme_id": id } }))
        .collect();

    serde_json::json!({
        "code": 200,
        "message": "success",
        "data": {
            "search_item_list": items,
            "has_more": has_more,
            "cursor": cursor,
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
// F1: two full pages of 20, count=40 -> 40 items.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f1_two_full_pages_returns_40() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page2), 0, 40))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel").with_count(40).with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 40);
}

// ---------------------------------------------------------------------------
// F2: page1=20, page2=15 (has_more=0), count=100 -> 35 items.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f2_partial_second_page_returns_35() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 15);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page2), 0, 35))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel")
        .with_count(100)
        .with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 35);
}

// ---------------------------------------------------------------------------
// F3: page1=20 (has_more=1), page2=0 items (empty) -> 20 items.
// An empty page terminates pagination even when has_more claims otherwise.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f3_empty_second_page_terminates_at_20() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 20))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel")
        .with_count(100)
        .with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 20);
}

// ---------------------------------------------------------------------------
// F4: page1 and page2 each 20 items but they SHARE one aweme_id ("dup1").
// count=40 -> returned content_ids must be unique (dedup across pages).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f4_overlapping_pages_dedup_unique_ids() {
    let mut server = mockito::Server::new_async().await;

    // page1: dup1 + 19 unique. page2: dup1 + 19 different unique.
    let mut page1: Vec<String> = vec!["dup1".to_string()];
    page1.extend(ids("p1", 19));
    let mut page2: Vec<String> = vec!["dup1".to_string()];
    page2.extend(ids("p2", 19));

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;

    let _m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page2), 0, 40))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel").with_count(40).with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    let unique: HashSet<&String> = result.iter().map(|c| &c.content_id).collect();
    assert_eq!(unique.len(), result.len());
}

// ---------------------------------------------------------------------------
// F5: every upstream request must use count<=20. We script two pages and
// assert that the per-request count param is exactly "20" (TikHub's cap).
// A request with count>20 would not match these mocks.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f5_per_request_count_never_exceeds_20() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);

    let m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("offset".into(), "0".into()),
            Matcher::UrlEncoded("count".into(), "20".into()),
        ]))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .expect(1)
        .create_async()
        .await;

    let m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("offset".into(), "20".into()),
            Matcher::UrlEncoded("count".into(), "20".into()),
        ]))
        .with_status(200)
        .with_body(search_body(&as_refs(&page2), 0, 40))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel").with_count(40).with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 40);
    // Each page fetched exactly once, each with count=20.
    m1.assert_async().await;
    m2.assert_async().await;
}

// ---------------------------------------------------------------------------
// F7: page1=10 (has_more=0), count=10 -> 10 items AND exactly ONE request.
// A second upstream request would fail the test (mock expects exactly 1 hit,
// and no offset=10/20 mock exists so it would 501).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn f7_single_page_single_request() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 10);

    let m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 0, 10))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel").with_count(10).with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 10);
    // Exactly one upstream request was made.
    m1.assert_async().await;
}
