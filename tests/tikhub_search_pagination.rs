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

use glance_mind_agent_rs::domain::{KeywordType, SearchOptions};
use glance_mind_agent_rs::ports::content_gateway::FetchShortfall;
use glance_mind_agent_rs::ports::ContentGateway;
use glance_mind_agent_rs::strategies::extra_keys;
use glance_mind_agent_rs::tikhub::TikHubRetryConfig;
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
    let options = SearchOptions::new("travel")
        .with_count(40)
        .with_region("US");

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
// F3 (reconciled to D2 EmptyPageLimit): page1=20 content (has_more=1), then
// THREE consecutive empty pages (has_more=1, cursors advancing 40/60/80) ->
// 20 items. D2 freezes EmptyPageLimit at MAX_EMPTY_PAGES=3 consecutive
// empty-progress pages (pagination.rs:15), so a single empty page no longer
// terminates the loop; it now takes three.
//
// ASSERTION-CHANGE-JUSTIFIED: D2 EmptyPageLimit 冻结——旧语义由 PR #5 私有循环实现,迁移后不再适用
// Original f3 encoded the PR #5 private-loop "stop on FIRST empty page"
// semantics (single empty page2 -> stop). After the search path migrates to
// PaginationLoop, EmptyPageLimit only fires after MAX_EMPTY_PAGES (3)
// consecutive empty-progress pages, so the mock shape is updated to 3 empty
// pages. The observable assertion (result.len() == 20: only the first content
// page is delivered) is UNCHANGED; only the mock page shape is reconciled to
// the frozen D2 termination semantics. RED rationale: the current
// "首空页即停" loop stops after the FIRST empty page (offset=40) and never
// requests offset=60/80, so the offset-60/offset-80 mocks go unmatched and the
// loop terminates one page early relative to the D2 EmptyPageLimit shape this
// test now encodes. Green after the PaginationLoop migration.
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

    // Three consecutive empty pages with advancing cursors. EmptyPageLimit
    // (D2) fires only after the third empty-progress page; each empty page must
    // be requested exactly once. Under the current "首空页即停" loop only the
    // FIRST empty page (offset=20) is requested, so the offset=40/60 asserts
    // fail -> RED; green after the PaginationLoop migration.
    let e1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 40))
        .expect(1)
        .create_async()
        .await;
    let e2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "40".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 60))
        .expect(1)
        .create_async()
        .await;
    let e3 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "60".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 80))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("travel")
        .with_count(100)
        .with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 20);
    // All three empty pages were requested (EmptyPageLimit needs 3).
    e1.assert_async().await;
    e2.assert_async().await;
    e3.assert_async().await;
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
    let options = SearchOptions::new("travel")
        .with_count(40)
        .with_region("US");

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
    let options = SearchOptions::new("travel")
        .with_count(40)
        .with_region("US");

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
    let options = SearchOptions::new("travel")
        .with_count(10)
        .with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert_eq!(result.len(), 10);
    // Exactly one upstream request was made.
    m1.assert_async().await;
}

// ---------------------------------------------------------------------------
// R6a (reconciled to D2 EmptyPageLimit): an all-empty stream whose pages claim
// has_more=1 with advancing cursors (0 -> 20 -> 40) still yields an empty Vec.
// This guards that the orchestrator's "zero results -> end campaign" path
// (src/orchestrator.rs) keeps firing after the pagination change.
//
// ASSERTION-CHANGE-JUSTIFIED: D2 EmptyPageLimit 冻结——旧语义由 PR #5 私有循环实现,迁移后不再适用
// Original r6a used a single empty first page with has_more=0 (cursor=0),
// which encoded the PR #5 private-loop "stop on the FIRST empty page"
// semantics. After migration to PaginationLoop, an empty page that claims
// has_more=1 no longer stops immediately: the loop advances cursors and only
// terminates via EmptyPageLimit after MAX_EMPTY_PAGES (3) consecutive
// empty-progress pages (pagination.rs:15). The observable assertion
// (result.is_empty(): empty stream -> empty Vec, preserving the
// zero-results->end-campaign trigger) is UNCHANGED; only the mock page shape
// is reconciled to the frozen D2 termination semantics. RED rationale: the
// current "首空页即停" loop stops after the FIRST empty page (offset=0) and
// never requests offset=20/40, so those mocks go unmatched and the loop
// terminates two pages early relative to the D2 EmptyPageLimit shape this test
// now encodes. Green after the PaginationLoop migration.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn r6a_empty_first_page_returns_empty_vec() {
    let mut server = mockito::Server::new_async().await;

    // Three consecutive empty pages with advancing cursors. EmptyPageLimit
    // (D2) fires only after the third empty-progress page; the stream yields
    // nothing throughout. Each empty page must be requested exactly once. Under
    // the current "首空页即停" loop only the FIRST empty page (offset=0) is
    // requested, so the offset=20/40 asserts fail -> RED; green after the
    // PaginationLoop migration.
    let e1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 20))
        .expect(1)
        .create_async()
        .await;
    let e2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 40))
        .expect(1)
        .create_async()
        .await;
    let e3 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "40".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 60))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let options = SearchOptions::new("nomatch")
        .with_count(100)
        .with_region("US");

    let result = adapter.search(&options).await.expect("search succeeds");

    assert!(result.is_empty());
    assert_eq!(result.len(), 0);
    // All three empty pages were requested (EmptyPageLimit needs 3).
    e1.assert_async().await;
    e2.assert_async().await;
    e3.assert_async().await;
}

// ===========================================================================
// M3-T2 T-011: tiktok search -> PaginationLoop migration via
// `fetch_by_keyword_with_outcome` (m3-tiktok-p1.md §4 M3-T2, tests 1~11).
// Assertions = plan contract text. These drive the D1 outcome path
// (`fetch_by_keyword_with_outcome`), which TikHub does NOT yet override, so the
// default trait method returns `shortfall: None` over the existing private
// `paginate_videos` loop. Most assertions therefore RED until the migration
// (PaginationLoop + override + PAGE_SIZE) lands. Per-test RED/GREEN annotations
// follow the plan's §4 "允许先绿" list and the M3-T2 RED 预期 block.
//
// DR-18 tiktok adaptation: TikHub's `cursor` is a JSON number deserialized to
// i64 (types.rs / search_body), so there is NO "non-numeric cursor" wire shape
// to exercise. The DR-18 nail (test 10) is therefore re-cast as the i64-native
// equivalent: has_more=1 but the `cursor` field is ABSENT -> normalized to
// next=None -> Exhausted. Justified gap: tiktok cursor=i64, no non-numeric
// form; the parse-failure branch DR-18 guards on string-cursor platforms is
// not reachable here, so the absent-cursor normalization stands in.
// ===========================================================================

const SEARCH_KEYWORD: &str = "travel";

fn search_kw() -> KeywordType {
    KeywordType::Search(SEARCH_KEYWORD.to_string())
}

fn outcome_options(count: u32) -> SearchOptions {
    SearchOptions::new(SEARCH_KEYWORD)
        .with_count(count)
        .with_region("US")
}

/// Adapter with a retry config that keeps RateLimited's max_retries=3 but
/// zeroes the retry delay (max_delay_ms=0 caps the 60s rate-limit delay to 0),
/// so a 429 page issues max_retries+1 = 4 HTTP requests with NO real sleep
/// (DR-19: "调用数 = 请求数", stay within the test budget without honoring a
/// Retry-After header the current client does not read). Used only by the
/// rate-limit tests (6/7) where the request count is asserted.
fn fast_retry_adapter(server: &mockito::Server) -> TikHubAdapter {
    let cfg = TikHubRetryConfig {
        max_retries: 3,
        initial_delay_ms: 0,
        max_delay_ms: 0,
        backoff_multiplier: 2.0,
    };
    TikHubAdapter::with_retry_config("test-key", Some(server.url()), cfg)
        .expect("adapter builds with retry config")
}

// ---------------------------------------------------------------------------
// Test 1 `paginates_offsets_until_count` (T-011 main assertion):
// 3 pages (20+20+10, has_more=1/1/0, cursor=20/40/—), count=50, page_size=20
// -> 50 contents, shortfall=None; captured request offsets 0,20,40 and count=20.
//
// 允许先绿 + AG-006 金丝雀 for the "request count 3 / offset progression"
// sub-assertion (RED 前提已被 PR #5 消解). The "per-request count=fixed 20"
// sub-assertion differs from main's min(remaining,20) (3rd request main sends
// count=10), so the offset=40&count=20 mock will not match under the current
// impl -> RED there, per plan's "待执行期复核 / 以 D2/D4 契约为准".
// ---------------------------------------------------------------------------
#[tokio::test]
async fn paginates_offsets_until_count() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);
    let page3 = ids("p3", 10);

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
        .with_body(search_body(&as_refs(&page2), 1, 40))
        .expect(1)
        .create_async()
        .await;
    let m3 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("offset".into(), "40".into()),
            Matcher::UrlEncoded("count".into(), "20".into()),
        ]))
        .with_status(200)
        .with_body(search_body(&as_refs(&page3), 0, 60))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("fetch succeeds");

    assert_eq!(outcome.contents.len(), 50);
    assert!(outcome.shortfall.is_none());

    // Offset sequence 0,20,40 each requested exactly once with count=20.
    m1.assert_async().await;
    m2.assert_async().await;
    m3.assert_async().await;
}

// ---------------------------------------------------------------------------
// Test 2 `exhausted_when_has_more_zero`: 2 pages (20+10, page2 has_more=0),
// count=50 -> 30 contents, Some(Exhausted). RED: main has no shortfall ->
// left: None, right: Some(Exhausted).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn exhausted_when_has_more_zero() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 10);

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
        .with_body(search_body(&as_refs(&page2), 0, 30))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("fetch succeeds");

    assert_eq!(outcome.contents.len(), 30);
    assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
}

// ---------------------------------------------------------------------------
// Test 3 `cursor_missing_normalized_exhausted`: has_more=1 but the cursor
// field is ABSENT -> next=None -> Some(Exhausted) (normalization). RED: main
// has no shortfall.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn cursor_missing_normalized_exhausted() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    // has_more=1 but no `cursor` key in `data`.
    let body = serde_json::json!({
        "code": 200,
        "message": "success",
        "data": {
            "search_item_list": page1
                .iter()
                .map(|id| serde_json::json!({ "aweme_info": { "aweme_id": id } }))
                .collect::<Vec<_>>(),
            "has_more": 1,
        }
    })
    .to_string();

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("fetch succeeds");

    assert_eq!(outcome.contents.len(), 20);
    assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
}

// ---------------------------------------------------------------------------
// Test 4 `repeated_cursor_stops` (F-004): page2 returns the SAME cursor as
// page1 -> terminate, Some(Exhausted), and NO third request. RED: main has no
// repeated-cursor detection (it would issue a 3rd request, bounded by
// max_pages) and reports no shortfall.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn repeated_cursor_stops() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 20);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        // cursor=20 -> next offset 20
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;
    let m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        // repeated cursor=20 -> CursorLoop
        .with_body(search_body(&as_refs(&page2), 1, 20))
        .expect(1)
        .create_async()
        .await;
    // A third request (offset=20 again) must NOT happen; assert page2 hit once.

    let adapter = adapter_for(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("fetch succeeds");

    assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    // page2 fetched exactly once; the repeated cursor must not trigger a refetch.
    m2.assert_async().await;
}

// ---------------------------------------------------------------------------
// Test 5 `empty_pages_stop_at_limit` (F-005): 3 consecutive empty pages
// (has_more=1, advancing cursors) after a content page -> terminate,
// Some(Exhausted). RED: main stops on the FIRST empty page (首空页即停),
// never reaching the 3rd empty page, and reports no shortfall.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn empty_pages_stop_at_limit() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;
    let _e1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 40))
        .create_async()
        .await;
    let _e2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "40".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 60))
        .create_async()
        .await;
    let _e3 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "60".into()))
        .with_status(200)
        .with_body(search_body(&[], 1, 80))
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(100))
        .await
        .expect("fetch succeeds");

    assert_eq!(outcome.contents.len(), 20);
    assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
}

// ---------------------------------------------------------------------------
// Test 6 `partial_failure_with_progress` (F-002): page1=20 + page2 HTTP 429
// -> 20 contents, Some(PartialFailure{..}) with message containing "rate"
// (lowercase). DR-19: 429 carries Retry-After: 0 and is queued per TikHub
// retry semantics; RateLimited max_retries=3 -> page2 = 4 HTTP requests total.
// RED: main maps any in-loop error straight to Err -> expected
// Ok(PartialFailure), got Err.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn partial_failure_with_progress() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;
    // page2 at offset=20 returns 429 on every attempt; with max_retries=3 the
    // adapter issues 4 requests (DR-19). Retry-After: 0 declares intent even
    // though the fast retry config zeroes the real delay.
    let m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(429)
        .with_header("Retry-After", "0")
        .with_body(serde_json::json!({"message": "rate limited"}).to_string())
        .expect(4)
        .create_async()
        .await;

    let adapter = fast_retry_adapter(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("fetch returns partial outcome (not Err) when there is progress");

    assert_eq!(outcome.contents.len(), 20);
    match outcome.shortfall {
        Some(FetchShortfall::PartialFailure { ref message }) => {
            assert!(
                message.contains("rate"),
                "PartialFailure message must contain lowercase \"rate\", got: {message}"
            );
        }
        other => panic!("expected Some(PartialFailure), got {other:?}"),
    }
    m2.assert_async().await;
}

// ---------------------------------------------------------------------------
// Test 7 `zero_progress_error_is_err` (F-001): page1 immediately 429 ->
// Err(..). 允许先绿: error passthrough is the current behavior; AG-006 covered
// by AG-012. DR-19 same as test 6: Retry-After: 0 + retry queue -> 4 requests.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn zero_progress_error_is_err() {
    let mut server = mockito::Server::new_async().await;

    let m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(429)
        .with_header("Retry-After", "0")
        .with_body(serde_json::json!({"message": "rate limited"}).to_string())
        .expect(4)
        .create_async()
        .await;

    let adapter = fast_retry_adapter(&server);
    let result = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await;

    assert!(
        result.is_err(),
        "zero-progress 429 must be Err, got {result:?}"
    );
    m1.assert_async().await;
}

// ---------------------------------------------------------------------------
// Test 8 `page_size_extra_consumed`: extra page_size=7 -> captured request
// count=7. The total count (50) is deliberately > the page_size hint (7) so
// that the current impl's `min(remaining,20)` would send count=20, NOT 7 ->
// the migrated reader of the PAGE_SIZE extra is what makes the request use
// count=7. RED: main ignores the extra and sends count=min(50,20)=20 -> the
// first-page mock (offset=0&count=7) goes unmatched (501) -> the fetch errors
// out instead of returning Ok (left: would-be count 20, right: 7).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn page_size_extra_consumed() {
    let mut server = mockito::Server::new_async().await;

    // First page of 7 (cursor advances by the page size); terminate after it so
    // the request-count assertion on the first page is the load-bearing check.
    let page1 = ids("p1", 7);

    let m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("offset".into(), "0".into()),
            Matcher::UrlEncoded("count".into(), "7".into()),
        ]))
        .with_status(200)
        // has_more=0 so the loop stops after one page; the point of this test is
        // the per-request count, not multi-page accumulation.
        .with_body(search_body(&as_refs(&page1), 0, 7))
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    // Total count 50 >> page_size 7: only a PAGE_SIZE-aware impl sends count=7.
    let options =
        outcome_options(50).with_extra_value(extra_keys::PAGE_SIZE, serde_json::json!(7));

    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &options)
        .await
        .expect("fetch succeeds");

    assert_eq!(outcome.contents.len(), 7);
    // The first request used count=7 (the page_size extra), not the default 20.
    m1.assert_async().await;
}

// ---------------------------------------------------------------------------
// Test 9 `legacy_search_unchanged`: the existing `fetch_by_keyword` (no
// outcome) returns the same contents as `fetch_by_keyword_with_outcome` under
// the same mock (regression). 允许先绿, AG-006.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn legacy_search_unchanged() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);
    let page2 = ids("p2", 10);

    // Two pages so both the legacy and outcome paths can be driven twice.
    for _ in 0..2 {
        server
            .mock("GET", SEARCH_PATH)
            .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
            .with_status(200)
            .with_body(search_body(&as_refs(&page1), 1, 20))
            .create_async()
            .await;
        server
            .mock("GET", SEARCH_PATH)
            .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
            .with_status(200)
            .with_body(search_body(&as_refs(&page2), 0, 30))
            .create_async()
            .await;
    }

    let adapter = adapter_for(&server);

    let legacy = adapter
        .fetch_by_keyword(&search_kw(), &outcome_options(50))
        .await
        .expect("legacy fetch succeeds");
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("outcome fetch succeeds");

    assert_eq!(legacy.len(), outcome.contents.len());
    let legacy_ids: Vec<&String> = legacy.iter().map(|c| &c.content_id).collect();
    let outcome_ids: Vec<&String> = outcome.contents.iter().map(|c| &c.content_id).collect();
    assert_eq!(legacy_ids, outcome_ids);
}

// ---------------------------------------------------------------------------
// Test 10 `non_numeric_cursor_normalized_exhausted` (DR-18, tiktok-adapted):
// TikHub cursor is i64, so there is no non-numeric wire form. Re-cast as the
// i64-native equivalent: has_more=1 but the cursor field is ABSENT (so the
// adapter cannot advance) -> normalized to next=None -> Some(Exhausted), no
// panic, no refetch. RED: main's cursor is i64 with no shortfall -> left: None,
// right: Some(Exhausted).
//
// Justified gap: tiktok cursor=i64, no non-numeric form; the DR-18
// parse-failure branch (string-cursor platforms) is not reachable here. This
// test pins the equivalent normalization (absent cursor -> Exhausted) and is
// distinct from test 3 by additionally asserting exactly ONE request (no
// refetch / no panic on the missing cursor).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn non_numeric_cursor_normalized_exhausted() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    // has_more=1 but `cursor` absent: no way to advance -> Exhausted.
    let body = serde_json::json!({
        "code": 200,
        "message": "success",
        "data": {
            "search_item_list": page1
                .iter()
                .map(|id| serde_json::json!({ "aweme_info": { "aweme_id": id } }))
                .collect::<Vec<_>>(),
            "has_more": 1,
        }
    })
    .to_string();

    let m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(body)
        .expect(1)
        .create_async()
        .await;

    let adapter = adapter_for(&server);
    let outcome = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await
        .expect("fetch succeeds (must not panic on absent cursor)");

    assert_eq!(outcome.shortfall, Some(FetchShortfall::Exhausted));
    // No refetch: the missing cursor terminates after exactly one request.
    m1.assert_async().await;
}

// ---------------------------------------------------------------------------
// Test 11 `hard_error_with_progress_is_err` (DR-10): page1=20 (progress) +
// page2 HTTP 500 (hard error) -> overall Err. The PartialFailure trigger set is
// RateLimited-only (M1 D2 frozen); hard errors must NOT be downgraded to
// Partial. 允许先绿 + AG-006 金丝雀: PR #5 already maps any error to Err, so
// this is green now; the canary (temporarily adding 500 to the Partial trigger
// set) must turn it red. Guards against an over-broad migration trigger set
// (expected Err, got Ok(PartialFailure)).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn hard_error_with_progress_is_err() {
    let mut server = mockito::Server::new_async().await;

    let page1 = ids("p1", 20);

    let _m1 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "0".into()))
        .with_status(200)
        .with_body(search_body(&as_refs(&page1), 1, 20))
        .create_async()
        .await;
    // page2: HTTP 500 hard error (ServerError). 500 is retryable in the client,
    // so use the fast retry adapter to avoid real backoff sleeps; after retries
    // are exhausted the loop must propagate Err (not PartialFailure).
    let _m2 = server
        .mock("GET", SEARCH_PATH)
        .match_query(Matcher::UrlEncoded("offset".into(), "20".into()))
        .with_status(500)
        .with_body(serde_json::json!({"message": "server error"}).to_string())
        .create_async()
        .await;

    let adapter = fast_retry_adapter(&server);
    let result = adapter
        .fetch_by_keyword_with_outcome(&search_kw(), &outcome_options(50))
        .await;

    assert!(
        result.is_err(),
        "hard error (HTTP 500) with progress must be Err, not Ok(PartialFailure); got {result:?}"
    );
}
