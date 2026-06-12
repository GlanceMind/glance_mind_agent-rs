//! Real API Tests - Test actual TikHub API calls for all platforms
//!
//! These tests require valid TIKHUB_API_KEY environment variable.
//! Run with: TIKHUB_API_KEY=xxx cargo test --test real_api_test -- --nocapture

use glance_mind_agent_rs::tikhub::{
    extract_comments_from_trees,
    // Helpers
    extract_posts_from_search,
    CommentParams,
    // Instagram
    HashtagSearchParams,
    InstagramCommentParams,
    RedditCommentParams,
    // Reddit
    RedditSearchParams,
    // TikTok
    SearchParams,
    TikHubClient,
    TikHubError,
    TikHubRetryConfig,
    TwitterCommentParams,
    // Twitter
    TwitterSearchParams,
};
use glance_mind_agent_rs::{
    ContentGateway, InstagramAdapter, KeywordType, SearchOptions, TikHubAdapter,
};

/// Verified on 2026-05-09 with the current local TikHub key:
/// V2 general_search returns HTTP 200, business code=200, 8 items,
/// and the first item includes a shortcode.
const INSTAGRAM_LIVE_SMOKE_KEYWORD: &str = "cat";

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

/// Helper to create client from env
fn create_client() -> Option<TikHubClient> {
    match TikHubClient::from_env() {
        Ok(client) => Some(client),
        Err(e) => {
            eprintln!("⚠️  Skipping test - TikHub client error: {:?}", e);
            None
        }
    }
}

// ============================================================
// TikTok API Tests
// ============================================================

#[tokio::test]
async fn test_tiktok_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_tiktok_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing TikTok Search API...");

    let params = SearchParams::new("fitness").with_count(5).with_region("US");

    match client.search_videos_with_retry(&params).await {
        Ok(response) => {
            let videos = TikHubClient::extract_videos(&response);
            let video_count = videos.len();
            println!("✅ TikTok Search: Found {} videos", video_count);

            if video_count > 0 {
                let first = videos[0];
                println!(
                    "   First video: id={}, author={}",
                    &first.aweme_id,
                    first.author_name().unwrap_or("?")
                );
            }

            assert!(video_count > 0, "Should find at least 1 video");
        }
        Err(e) => {
            println!("❌ TikTok Search Error: {:?}", e);
            panic!("TikTok search failed: {:?}", e);
        }
    }
}

/// Live end-to-end validation of the pagination fix: drives the
/// `ContentGateway::search` adapter (not the low-level client) with a desired
/// total of 40, which TikHub can only satisfy by paginating across multiple
/// ≤20-item pages. Asserts the result crossed the single-page cap of 20,
/// proving the offset/cursor loop works against the real API. Auto-skips
/// without a TikHub key, like the other live tests here.
#[tokio::test]
async fn test_tiktok_adapter_paginates_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_tiktok_adapter_paginates_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    let adapter = TikHubAdapter::new(client);
    let options = SearchOptions::new("fitness")
        .with_count(40)
        .with_region("US");

    println!("\n🔍 Testing TikTok adapter pagination (target=40)...");

    let result = adapter
        .search(&options)
        .await
        .expect("real TikHub adapter search should succeed");

    println!(
        "✅ Adapter pagination returned {} videos (target 40)",
        result.len()
    );

    assert!(
        result.len() > 20,
        "pagination must exceed the single-page cap of 20; got {} (pre-fix this was always ≤20)",
        result.len()
    );
}

// ============================================================
// Instagram API Tests
// ============================================================

#[tokio::test]
async fn test_instagram_hashtag_search_v1_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_hashtag_search_v1_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram V1 API (Hashtag Search)...");

    // Use V1 API which is more stable and uses 'hashtag' parameter
    match client
        .search_hashtag_posts_v1_with_retry("fitness", None)
        .await
    {
        Ok(response) => {
            let post_count = response
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.hashtag.as_ref())
                .and_then(|h| h.edge_hashtag_to_media.as_ref())
                .and_then(|e| e.edges.as_ref())
                .map(|edges| edges.len())
                .unwrap_or(0);

            println!("✅ Instagram V1 Hashtag Search: Found {} posts", post_count);

            if post_count > 0 {
                let edges = response
                    .data
                    .unwrap()
                    .data
                    .unwrap()
                    .hashtag
                    .unwrap()
                    .edge_hashtag_to_media
                    .unwrap()
                    .edges
                    .unwrap();
                let first = &edges[0];
                if let Some(ref node) = first.node {
                    println!(
                        "   First post: shortcode={}, owner={}",
                        node.shortcode.as_deref().unwrap_or("?"),
                        node.owner
                            .as_ref()
                            .and_then(|o| o.username.as_deref())
                            .unwrap_or("?")
                    );
                }
            }

            assert!(post_count > 0, "Should find at least 1 post");
        }
        Err(e) => {
            println!("❌ Instagram V1 Hashtag Search Error: {:?}", e);
            panic!("Instagram V1 hashtag search failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_instagram_web_api_deprecated() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_web_api_deprecated - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram Web API (Deprecated)...");

    // NOTE: web_app API is deprecated/not working, this test just verifies it fails gracefully
    let params = HashtagSearchParams::new("fitness").with_feed_type("clips");

    match client.search_hashtag_posts_web_with_retry(&params).await {
        Ok(response) => {
            let post_count = response
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.items.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);

            println!(
                "✅ Instagram Web API (deprecated): Found {} posts",
                post_count
            );
        }
        Err(e) => {
            // Expected - web_app API is deprecated
            println!(
                "⚠️  Instagram Web API Error (expected - API deprecated): {:?}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_instagram_v3_general_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_v3_general_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram V3 API (General Search)...");

    match client
        .search_instagram_general_with_retry("#muhameds")
        .await
    {
        Ok(response) => {
            let posts = TikHubClient::extract_instagram_general_posts(&response);
            let post_count = posts.len();

            println!("✅ Instagram V3 General Search: Found {} posts", post_count);

            assert_eq!(response.code, 200, "TikHub should return success code");
            assert!(
                post_count > 0,
                "V3 general_search should return media posts"
            );

            let first = posts[0];
            assert!(
                !first.code.as_deref().unwrap_or("").is_empty(),
                "V3 media should include shortcode for downstream comment fetches"
            );
            assert!(
                first.created_at_timestamp().is_some(),
                "V3 numeric taken_at should parse into a timestamp"
            );
            assert!(
                first.thumbnail().is_some(),
                "V3 image_versions2 should expose a thumbnail URL"
            );
        }
        Err(TikHubError::BadRequest { message }) => {
            println!(
                "⚠️  Instagram V3 general_search returned HTTP 400; fallback path should cover this. Body: {}",
                message
            );
        }
        Err(e) => {
            println!("❌ Instagram V3 General Search Error: {:?}", e);
            panic!(
                "Instagram V3 general_search failed with non-fallback error: {:?}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_instagram_v2_general_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_v2_general_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram V2 API (General Search fallback)...");

    match client
        .search_instagram_general_v2_with_retry(INSTAGRAM_LIVE_SMOKE_KEYWORD)
        .await
    {
        Ok(response) => {
            let posts = TikHubClient::extract_instagram_general_v2_posts(&response);
            println!(
                "✅ Instagram V2 General Search: Found {} posts",
                posts.len()
            );

            assert_eq!(response.code, 200, "TikHub V2 should return success code");
            assert!(
                !posts.is_empty(),
                "V2 general_search should return media posts"
            );
            assert!(
                !posts[0].code.as_deref().unwrap_or("").is_empty(),
                "V2 media should include shortcode for downstream comment fetches"
            );
        }
        Err(e) => {
            println!("❌ Instagram V2 General Search Error: {:?}", e);
            panic!("Instagram V2 general_search failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_instagram_adapter_uses_fallback_capable_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_adapter_uses_fallback_capable_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let _ = dotenvy::dotenv();
    let Ok(adapter) = InstagramAdapter::from_env() else {
        eprintln!("⚠️  Skipping test - InstagramAdapter::from_env failed");
        return;
    };

    println!("\n🔍 Testing Instagram adapter via fallback-capable search...");

    let contents = adapter
        .fetch_by_keyword(
            &KeywordType::Hashtag(INSTAGRAM_LIVE_SMOKE_KEYWORD.to_string()),
            &SearchOptions::new(INSTAGRAM_LIVE_SMOKE_KEYWORD)
                .with_platform("instagram")
                .with_count(3),
        )
        .await
        .expect("Instagram adapter should fetch content via V3 or V2 fallback");

    println!(
        "✅ Instagram Adapter fallback-capable search: Found {} posts",
        contents.len()
    );

    assert!(
        !contents.is_empty(),
        "adapter should return media content via V3 or V2 fallback"
    );
    assert_eq!(contents[0].platform, "instagram");
    assert!(
        contents[0]
            .url
            .as_deref()
            .unwrap_or("")
            .contains("instagram.com/p/"),
        "adapter should preserve shortcode URL for downstream comment fetch"
    );
}

#[tokio::test]
async fn test_instagram_v2_hashtag_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_v2_hashtag_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram V2 API (Hashtag Posts)...");

    // Test the V2 API (may return 400)
    let params = HashtagSearchParams::new("fitness").with_feed_type("top");

    match client.search_hashtag_posts_with_retry(&params).await {
        Ok(response) => {
            let post_count = response
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.items.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);

            println!("✅ Instagram V2 Hashtag Search: Found {} posts", post_count);
        }
        Err(e) => {
            println!(
                "⚠️  Instagram V2 Hashtag Search Error (may be expected): {:?}",
                e
            );
            // V2 API may fail, that's why we use web_app API
        }
    }
}

#[tokio::test]
async fn test_instagram_v2_reels_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_v2_reels_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram V2 API (Reels Search)...");

    // Test the V2 API (may return 400)
    let params = glance_mind_agent_rs::tikhub::ReelsSearchParams::new("fitness");

    match client.search_instagram_reels_with_retry(&params).await {
        Ok(response) => {
            let reel_count = response
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.items.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);

            println!("✅ Instagram V2 Reels Search: Found {} reels", reel_count);
        }
        Err(e) => {
            println!(
                "⚠️  Instagram V2 Reels Search Error (may be expected): {:?}",
                e
            );
            // V2 API may fail, that's why we use web_app API
        }
    }
}

// ============================================================
// Reddit API Tests
// ============================================================

#[tokio::test]
async fn test_reddit_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_reddit_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Reddit Search API...");

    let params = RedditSearchParams::new("rust programming");

    match client.search_reddit_posts_with_retry(&params).await {
        Ok(response) => {
            let post_count = response
                .data
                .as_ref()
                .map(|d| extract_posts_from_search(d).len())
                .unwrap_or(0);

            println!("✅ Reddit Search: Found {} posts", post_count);

            if post_count > 0 {
                let posts = extract_posts_from_search(response.data.as_ref().unwrap());
                let first = posts[0];
                println!(
                    "   First post: subreddit={}, title={:.50}...",
                    first.subreddit_name().unwrap_or("?"),
                    first.title_str()
                );
            }

            assert!(post_count > 0, "Should find at least 1 post");
        }
        Err(e) => {
            println!("❌ Reddit Search Error: {:?}", e);
            panic!("Reddit search failed: {:?}", e);
        }
    }
}

// ============================================================
// Twitter API Tests
// ============================================================

#[tokio::test]
async fn test_twitter_search_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_search_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Twitter Search API...");

    let params = TwitterSearchParams::new("rustlang").with_search_type("Latest");

    match client.search_twitter_tweets_with_retry(&params).await {
        Ok(response) => {
            let tweet_count = response
                .data
                .as_ref()
                .and_then(|d| d.timeline.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);

            println!("✅ Twitter Search: Found {} tweets", tweet_count);

            if tweet_count > 0 {
                let timeline = response.data.unwrap().timeline.unwrap();
                let first = &timeline[0];
                println!(
                    "   First tweet: id={}, text={:.50}...",
                    first.get_tweet_id().unwrap_or("?"),
                    first.content()
                );
            }

            assert!(tweet_count > 0, "Should find at least 1 tweet");
        }
        Err(e) => {
            println!("❌ Twitter Search Error: {:?}", e);
            panic!("Twitter search failed: {:?}", e);
        }
    }
}

// ============================================================
// All Platforms Summary Test
// ============================================================

#[tokio::test]
async fn test_all_platforms_summary() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_all_platforms_summary - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n{}", "=".repeat(60));
    println!("🧪 Testing All Platforms API Calls");
    println!("{}", "=".repeat(60));

    let mut results: Vec<(&str, bool, String)> = Vec::new();

    // TikTok
    print!("\n📱 TikTok... ");
    let tiktok_result = client
        .search_videos_with_retry(&SearchParams::new("fitness").with_count(3))
        .await;
    match tiktok_result {
        Ok(r) => {
            let count = TikHubClient::extract_videos(&r).len();
            println!("✅ {} videos", count);
            results.push(("TikTok", true, format!("{} videos", count)));
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("TikTok", false, format!("{:?}", e)));
        }
    }

    // Instagram (V1 API)
    print!("📷 Instagram (V1)... ");
    let ig_result = client
        .search_hashtag_posts_v1_with_retry("fitness", None)
        .await;
    match ig_result {
        Ok(r) => {
            let count = r
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.hashtag.as_ref())
                .and_then(|h| h.edge_hashtag_to_media.as_ref())
                .and_then(|e| e.edges.as_ref())
                .map(|edges| edges.len())
                .unwrap_or(0);
            println!("✅ {} posts", count);
            results.push(("Instagram", true, format!("{} posts", count)));
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("Instagram", false, format!("{:?}", e)));
        }
    }

    // Reddit
    print!("🤖 Reddit... ");
    let reddit_result = client
        .search_reddit_posts_with_retry(&RedditSearchParams::new("rust"))
        .await;
    match reddit_result {
        Ok(r) => {
            let count = r
                .data
                .as_ref()
                .map(|d| extract_posts_from_search(d).len())
                .unwrap_or(0);
            println!("✅ {} posts", count);
            results.push(("Reddit", true, format!("{} posts", count)));
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("Reddit", false, format!("{:?}", e)));
        }
    }

    // Twitter
    print!("🐦 Twitter... ");
    let twitter_result = client
        .search_twitter_tweets_with_retry(&TwitterSearchParams::new("rust"))
        .await;
    match twitter_result {
        Ok(r) => {
            let count = r
                .data
                .as_ref()
                .and_then(|d| d.timeline.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);
            println!("✅ {} tweets", count);
            results.push(("Twitter", true, format!("{} tweets", count)));
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("Twitter", false, format!("{:?}", e)));
        }
    }

    // Summary
    println!("\n{}", "=".repeat(60));
    println!("📊 Summary:");
    println!("{}", "-".repeat(60));

    let success_count = results.iter().filter(|(_, ok, _)| *ok).count();
    let total = results.len();

    for (platform, ok, detail) in &results {
        let status = if *ok { "✅" } else { "❌" };
        println!("  {} {}: {}", status, platform, detail);
    }

    println!("{}", "-".repeat(60));
    println!("  Total: {}/{} platforms succeeded", success_count, total);
    println!("{}", "=".repeat(60));

    // At least TikTok should work
    assert!(
        results.iter().any(|(p, ok, _)| *p == "TikTok" && *ok),
        "TikTok API should work"
    );
}

// ============================================================
// Comment API Tests
// ============================================================

#[tokio::test]
async fn test_tiktok_comments_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_tiktok_comments_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing TikTok Comments API...");

    // First search for a video to get a valid video ID
    let search_params = SearchParams::new("fitness").with_count(1);
    let search_result = client.search_videos_with_retry(&search_params).await;

    let video_id = match search_result {
        Ok(response) => {
            let videos = TikHubClient::extract_videos(&response);
            if videos.is_empty() {
                println!("⚠️  No videos found to test comments");
                return;
            }
            videos[0].aweme_id.clone()
        }
        Err(e) => {
            println!("⚠️  Could not search videos: {:?}", e);
            return;
        }
    };

    println!("   Testing with video ID: {}", video_id);

    let params = CommentParams::default();

    match client.fetch_comments_with_retry(&video_id, &params).await {
        Ok(response) => {
            let comments = TikHubClient::extract_comments(&response);
            println!("✅ TikTok Comments: Found {} comments", comments.len());

            if !comments.is_empty() {
                let first = comments[0];
                println!(
                    "   First comment: author={}, text={:.30}...",
                    first
                        .user
                        .as_ref()
                        .and_then(|u| u.unique_id.as_deref())
                        .unwrap_or("?"),
                    first.text.as_deref().unwrap_or("?")
                );
            }
        }
        Err(e) => {
            println!("⚠️  TikTok Comments Error: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_instagram_comments_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_instagram_comments_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Instagram Comments API...");

    // First search for a post to get a valid post code
    let search_result = client
        .search_hashtag_posts_v1_with_retry("fitness", None)
        .await;

    let post_code = match search_result {
        Ok(response) => {
            let edges = response
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.hashtag.as_ref())
                .and_then(|h| h.edge_hashtag_to_media.as_ref())
                .and_then(|e| e.edges.as_ref());

            match edges.and_then(|e| e.first()) {
                Some(edge) => edge
                    .node
                    .as_ref()
                    .and_then(|n| n.shortcode.clone())
                    .unwrap_or_default(),
                None => {
                    println!("⚠️  No posts found to test comments");
                    return;
                }
            }
        }
        Err(e) => {
            println!("⚠️  Could not search posts: {:?}", e);
            return;
        }
    };

    if post_code.is_empty() {
        println!("⚠️  No valid post code found");
        return;
    }

    println!("   Testing with post code: {}", post_code);

    let params = InstagramCommentParams::new(&post_code);

    match client.fetch_instagram_comments_with_retry(&params).await {
        Ok(response) => {
            let comment_count = response
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.items.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);

            println!("✅ Instagram Comments: Found {} comments", comment_count);
        }
        Err(e) => {
            println!("⚠️  Instagram Comments Error (V2 API may fail): {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_reddit_comments_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_reddit_comments_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Reddit Comments API...");

    // First search for a post to get a valid post ID
    let search_params = RedditSearchParams::new("rust programming");
    let search_result = client.search_reddit_posts_with_retry(&search_params).await;

    let post_id = match search_result {
        Ok(response) => {
            let posts = response
                .data
                .as_ref()
                .map(|d| extract_posts_from_search(d))
                .unwrap_or_default();

            if posts.is_empty() {
                println!("⚠️  No posts found to test comments");
                return;
            }

            posts[0].post_id().unwrap_or_default()
        }
        Err(e) => {
            println!("⚠️  Could not search posts: {:?}", e);
            return;
        }
    };

    if post_id.is_empty() {
        println!("⚠️  No valid post ID found");
        return;
    }

    // Ensure t3_ prefix
    let post_id = if post_id.starts_with("t3_") {
        post_id
    } else {
        format!("t3_{}", post_id)
    };
    println!("   Testing with post ID: {}", post_id);

    let params = RedditCommentParams::new(&post_id).with_limit(10);

    match client.fetch_reddit_comments_with_retry(&params).await {
        Ok(response) => {
            let comment_count = response
                .data
                .as_ref()
                .and_then(|d| d.post_info_by_id.as_ref())
                .and_then(|p| p.comment_forest.as_ref())
                .and_then(|f| f.trees.as_ref())
                .map(|trees| extract_comments_from_trees(trees, None).len())
                .unwrap_or(0);

            println!("✅ Reddit Comments: Found {} comments", comment_count);
        }
        Err(e) => {
            println!("⚠️  Reddit Comments Error: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_twitter_comments_real() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_twitter_comments_real - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔍 Testing Twitter Comments API...");

    // First search for a tweet to get a valid tweet ID
    let search_params = TwitterSearchParams::new("rust").with_search_type("Latest");
    let search_result = client
        .search_twitter_tweets_with_retry(&search_params)
        .await;

    let tweet_id = match search_result {
        Ok(response) => {
            match response
                .data
                .as_ref()
                .and_then(|d| d.timeline.as_ref())
                .and_then(|t| t.first())
            {
                Some(tweet) => tweet.get_tweet_id().unwrap_or("").to_string(),
                None => {
                    println!("⚠️  No tweets found to test comments");
                    return;
                }
            }
        }
        Err(e) => {
            println!("⚠️  Could not search tweets: {:?}", e);
            return;
        }
    };

    if tweet_id.is_empty() {
        println!("⚠️  No valid tweet ID found");
        return;
    }

    println!("   Testing with tweet ID: {}", tweet_id);

    let params = TwitterCommentParams::new(&tweet_id);

    match client.fetch_twitter_comments_with_retry(&params).await {
        Ok(response) => {
            let comment_count = response
                .data
                .as_ref()
                .and_then(|d| d.thread.as_ref())
                .map(|list| list.len())
                .unwrap_or(0);

            println!(
                "✅ Twitter Comments: Found {} comments/replies",
                comment_count
            );
        }
        Err(e) => {
            println!("⚠️  Twitter Comments Error: {:?}", e);
        }
    }
}

// ============================================================
// M4-T4 Real Gate Probes: Reddit/Twitter Second-Page Pagination
// AG-008 criteria:
//   (1) TIKHUB_API_KEY gate — skip without credentials (DR-19 zero-retry budget)
//   (2) DR-16 branch-assert (twitter): cursor equality branch is a plan contract,
//       not a runtime guard bypass — both paths are explicitly specified.
//   (3) DR-20 fixture rehydration obligation: after live run, compare each field
//       in the dumped JSON against M4-T2/T3 mock fixture shapes (reddit_types.rs /
//       twitter_types.rs serde definitions). Divergence → stop, report, mock
//       revision requires ASSERTION-CHANGE-JUSTIFIED + root notification.
// ============================================================

/// T-052 · AG-008 · Reddit second-page real-API probe
///
/// Gate: TIKHUB_API_KEY (skip when absent — no credentials on this machine).
///
/// DR-19 zero-retry: uses `search_reddit_posts` (no retry wrapper) to keep
/// HTTP request count ≤ 3 (page-1 + page-2 = 2 requests, budget = 3).
///
/// Flow:
///   1. Fetch page 1 → extract end_cursor from pageInfo.
///   2. Fetch page 2 with `after=<end_cursor>`.
///   3. Assert page-2 response is not an error.
///   4. Assert id-sets are not identical (content advanced).
///
/// DR-20 fixture rehydration obligation:
///   Raw responses are serialised to tests/fixtures/reddit/ on live run.
///   After rehydration, compare every field against the mock shapes used in
///   M4-T2 (reddit_types.rs · RedditSearchData / RedditMainComponent /
///   RedditPageInfo). Divergence must be reported; mock revision requires
///   ASSERTION-CHANGE-JUSTIFIED + root notification.
#[tokio::test]
async fn real_reddit_search_second_page_after() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping real_reddit_search_second_page_after - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    eprintln!("\n[T-052/reddit] Fetching page 1 (no retry, DR-19)...");

    // --- Page 1: DR-19 no retry wrapper, request count = 1 ---
    let p1_params = RedditSearchParams::new("rust programming");
    let page1_resp = client
        .search_reddit_posts(&p1_params)
        .await
        .expect("page-1 reddit search must succeed");

    assert_eq!(page1_resp.code, 200, "page-1 code must be 200");

    // Collect page-1 post IDs.
    let p1_posts = page1_resp
        .data
        .as_ref()
        .map(|d| extract_posts_from_search(d))
        .unwrap_or_default();
    assert!(
        !p1_posts.is_empty(),
        "page-1 must return at least one post to obtain end_cursor"
    );
    let p1_ids: std::collections::HashSet<String> = p1_posts
        .iter()
        .filter_map(|p| p.post_id())
        .collect();

    eprintln!("[T-052/reddit] page-1 posts={}", p1_posts.len());

    // Extract end_cursor from pageInfo (RedditMainComponent).
    let end_cursor: Option<String> = page1_resp
        .data
        .as_ref()
        .and_then(|d| d.search.as_ref())
        .and_then(|s| s.dynamic.as_ref())
        .and_then(|dy| dy.components.as_ref())
        .and_then(|c| c.main.as_ref())
        .and_then(|m| m.page_info.as_ref())
        .and_then(|pi| pi.end_cursor.clone());

    // DR-20: serialise page-1 fixture for rehydration audit.
    {
        let fixture_dir = std::path::Path::new("tests/fixtures/reddit");
        if fixture_dir.exists() {
            if let Ok(json) = serde_json::to_string_pretty(&page1_resp) {
                let path = fixture_dir.join("search_rust_programming_page1.json");
                if let Err(e) = std::fs::write(&path, &json) {
                    eprintln!("[DR-20/reddit] could not write page-1 fixture: {e}");
                } else {
                    eprintln!("[DR-20/reddit] page-1 fixture written: {}", path.display());
                }
            }
        }
    }

    let Some(cursor) = end_cursor else {
        // No cursor → single-page result set; cannot test pagination advance.
        // Document and pass: the adapter exhausted the result on page 1.
        eprintln!(
            "[T-052/reddit] end_cursor absent after page-1 — single-page result; \
             second-page advance cannot be verified. PASS (exhausted)"
        );
        return;
    };

    eprintln!("[T-052/reddit] end_cursor={cursor:.40}...; fetching page 2...");

    // --- Page 2: DR-19 request count = 2 ≤ budget of 3 ---
    let p2_params = RedditSearchParams::new("rust programming").with_after(&cursor);
    let page2_resp = client
        .search_reddit_posts(&p2_params)
        .await
        .expect("page-2 reddit search must succeed (non-error)");

    assert_eq!(page2_resp.code, 200, "page-2 code must be 200");

    // DR-20: serialise page-2 fixture.
    {
        let fixture_dir = std::path::Path::new("tests/fixtures/reddit");
        if fixture_dir.exists() {
            if let Ok(json) = serde_json::to_string_pretty(&page2_resp) {
                let path = fixture_dir.join("search_rust_programming_page2.json");
                if let Err(e) = std::fs::write(&path, &json) {
                    eprintln!("[DR-20/reddit] could not write page-2 fixture: {e}");
                } else {
                    eprintln!("[DR-20/reddit] page-2 fixture written: {}", path.display());
                }
            }
        }
    }

    // Collect page-2 post IDs.
    let p2_posts = page2_resp
        .data
        .as_ref()
        .map(|d| extract_posts_from_search(d))
        .unwrap_or_default();
    let p2_ids: std::collections::HashSet<String> = p2_posts
        .iter()
        .filter_map(|p| p.post_id())
        .collect();

    eprintln!("[T-052/reddit] page-2 posts={}; asserting content advance...", p2_posts.len());

    // Assert content advanced: id-sets must not be identical.
    assert_ne!(
        p1_ids, p2_ids,
        "page-2 id-set must differ from page-1 (content advanced via after={cursor:.20})"
    );

    eprintln!("[T-052/reddit] PASS — second-page content advance confirmed.");
}

/// T-052 · AG-008 · DR-16 · Twitter second-page real-API probe (branch-assert)
///
/// Gate: TIKHUB_API_KEY (skip when absent — no credentials on this machine).
///
/// DR-19 zero-retry: uses `search_twitter_tweets` (no retry wrapper) for
/// HTTP request count ≤ 3 (page-1 + page-2 = 2 requests, budget = 3).
///
/// DR-16 branch-assert (execution-time zero assertion change):
///   Both branches are pre-written plan contracts; which branch executes
///   depends on live API behaviour — neither branch is altered at runtime.
///
///   if second_cursor == first_cursor {
///       // F-004: Twitter known to return identical cursor on some searches.
///       // Record evidence (first/second cursor values) for DR-16 audit.
///       // Assert only that the response is non-error. PASS.
///   } else {
///       // Cursor advanced → assert id-sets not identical.
///   }
///
/// DR-20 fixture rehydration obligation:
///   Raw responses serialised to tests/fixtures/twitter/ on live run.
///   After rehydration, compare fields against M4-T3 mock shapes
///   (twitter_types.rs · TwitterTimelineData / TwitterTweet / next_cursor).
///   Divergence → stop, report; mock revision requires
///   ASSERTION-CHANGE-JUSTIFIED + root notification.
#[tokio::test]
async fn real_twitter_search_second_page_cursor() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping real_twitter_search_second_page_cursor - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    eprintln!("\n[T-052/twitter] Fetching page 1 (no retry, DR-19)...");

    // --- Page 1: DR-19 no retry wrapper, request count = 1 ---
    let p1_params = TwitterSearchParams::new("rustlang").with_search_type("Latest");
    let page1_resp = client
        .search_twitter_tweets(&p1_params)
        .await
        .expect("page-1 twitter search must succeed");

    assert_eq!(page1_resp.code, 200, "page-1 code must be 200");

    let p1_data = page1_resp
        .data
        .as_ref()
        .expect("page-1 data must be present");

    let p1_tweets = p1_data.timeline.as_deref().unwrap_or_default();
    assert!(
        !p1_tweets.is_empty(),
        "page-1 must return at least one tweet to obtain next_cursor"
    );

    let p1_ids: std::collections::HashSet<String> = p1_tweets
        .iter()
        .filter_map(|t| t.get_tweet_id().map(|s| s.to_string()))
        .collect();

    let first_cursor: Option<String> = p1_data.next_cursor.clone();

    eprintln!(
        "[T-052/twitter] page-1 tweets={}; next_cursor={:?}",
        p1_tweets.len(),
        first_cursor.as_deref().map(|c| &c[..c.len().min(40)])
    );

    // DR-20: serialise page-1 fixture.
    {
        let fixture_dir = std::path::Path::new("tests/fixtures/twitter");
        if fixture_dir.exists() {
            if let Ok(json) = serde_json::to_string_pretty(&page1_resp) {
                let path = fixture_dir.join("search_rustlang_page1.json");
                if let Err(e) = std::fs::write(&path, &json) {
                    eprintln!("[DR-20/twitter] could not write page-1 fixture: {e}");
                } else {
                    eprintln!("[DR-20/twitter] page-1 fixture written: {}", path.display());
                }
            }
        }
    }

    let Some(ref cursor) = first_cursor else {
        eprintln!(
            "[T-052/twitter] next_cursor absent after page-1 — single-page result; \
             second-page advance cannot be verified. PASS (exhausted)"
        );
        return;
    };

    eprintln!("[T-052/twitter] first_cursor={cursor:.40}...; fetching page 2...");

    // --- Page 2: DR-19 request count = 2 ≤ budget of 3 ---
    let p2_params = TwitterSearchParams::new("rustlang")
        .with_search_type("Latest")
        .with_cursor(cursor);
    let page2_resp = client
        .search_twitter_tweets(&p2_params)
        .await
        .expect("page-2 twitter search must succeed (non-error)");

    assert_eq!(page2_resp.code, 200, "page-2 code must be 200");

    // DR-20: serialise page-2 fixture.
    {
        let fixture_dir = std::path::Path::new("tests/fixtures/twitter");
        if fixture_dir.exists() {
            if let Ok(json) = serde_json::to_string_pretty(&page2_resp) {
                let path = fixture_dir.join("search_rustlang_page2.json");
                if let Err(e) = std::fs::write(&path, &json) {
                    eprintln!("[DR-20/twitter] could not write page-2 fixture: {e}");
                } else {
                    eprintln!("[DR-20/twitter] page-2 fixture written: {}", path.display());
                }
            }
        }
    }

    let second_cursor = page2_resp
        .data
        .as_ref()
        .and_then(|d| d.next_cursor.clone());

    let p2_tweets = page2_resp
        .data
        .as_ref()
        .and_then(|d| d.timeline.as_ref())
        .map(|t| t.as_slice())
        .unwrap_or_default();

    // DR-16 branch-assert: both branches are pre-written plan contracts.
    // Execution picks the branch based on live API behaviour — zero assertion
    // changes are allowed at runtime.
    if second_cursor.as_deref() == Some(cursor.as_str()) {
        // F-004 evidence: Twitter returned the same cursor for page 2.
        // This is known behaviour documented in DR-16.
        // Branch contract: assert only that the response is non-error. PASS.
        eprintln!(
            "[T-052/twitter][DR-16/F-004] second_cursor == first_cursor ({:.40}…); \
             Twitter returned identical cursor — known behaviour. \
             Asserting response non-error only. PASS.",
            cursor
        );
        // Response non-error already asserted above (code == 200). Explicit:
        assert_eq!(
            page2_resp.code, 200,
            "page-2 must be non-error even when cursor does not advance (DR-16 branch)"
        );
    } else {
        // Cursor advanced → assert content also advanced.
        eprintln!(
            "[T-052/twitter] cursor advanced: {:?} → {:?}; asserting id-set divergence...",
            first_cursor.as_deref().map(|c| &c[..c.len().min(20)]),
            second_cursor.as_deref().map(|c| &c[..c.len().min(20)])
        );
        let p2_ids: std::collections::HashSet<String> = p2_tweets
            .iter()
            .filter_map(|t| t.get_tweet_id().map(|s| s.to_string()))
            .collect();
        assert_ne!(
            p1_ids, p2_ids,
            "page-2 id-set must differ from page-1 when cursor advanced"
        );
        eprintln!("[T-052/twitter] PASS — second-page content advance confirmed.");
    }
}

// ============================================================
// M3-T3 · T-051 · AG-008: TikTok second-page offset probe
// ============================================================

/// Live probe for second-page offset pagination semantics (T-051 / AG-008).
///
/// ## AG-008 判据
/// 1. **HTTP 请求数 ≤ 3**（DR-19 零重试：max_retries=0，每次调用=1 个 HTTP 请求）；
///    本测试共发出 2 次 `search_videos` 调用 → 2 个 HTTP 请求 ≤ 3。
/// 2. **offset=0 首页 → 记录 has_more/cursor 实测值**；
///    **offset=cursor 第二页 → 断言非空、aweme_id 集合不全同**。
/// 3. **cursor 推进语义对账（M3-T3 核心）**：
///    - API 返回的 `cursor` 字段（i64）被 M3-T2 适配层直接用作下一次
///      `offset`（u32 截断：`cursor.max(0) as u32`）；
///    - 若 `cursor` 超出 u32 范围（> 4 294 967 295），offset 会截断溢出，
///      与 M3-T2 实现假设冲突；该测试通过 `assert!` 验证 cursor 在 u32 范围内；
///    - 实跑结论（待凭据）须写入
///      `tests/fixtures/tiktok/search_fitness_us_page2.json` 旁注注释。
///
/// ## 回灌义务
/// 实跑通过时须将第二页响应落盘至：
///   `tests/fixtures/tiktok/search_fitness_us_page2.json`
/// 并在文件顶部注释记录：
///   - 实跑时间戳
///   - 首页 cursor 实测值
///   - cursor 是否在 u32 范围内（offset=cursor.parse 假设是否成立）
///   - 第二页 aweme_id 集合与首页重叠数
///
/// ## 若与 M3-T2 假设冲突
/// 若 cursor > u32::MAX，**停下上报，走 ASSERTION-CHANGE-JUSTIFIED + root 知会，
/// 不静默改 mock**。
#[tokio::test]
async fn real_tiktok_search_second_page_offset() {
    // --- Gate: skip without credentials / CI opt-in (M1-T0 pattern) ---
    if !live_api_tests_enabled() {
        eprintln!(
            "Skipping real_tiktok_search_second_page_offset - \
             credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent"
        );
        return;
    }

    // DR-19: zero-retry client — each search_videos call = exactly 1 HTTP request.
    // Budget: 2 calls (page1 + page2) ≤ 3 HTTP requests.
    let zero_retry_config = TikHubRetryConfig {
        max_retries: 0,
        initial_delay_ms: 0,
        max_delay_ms: 0,
        backoff_multiplier: 1.0,
    };
    let api_key = match std::env::var("TIKHUB_API_KEY") {
        Ok(k) if !k.trim().is_empty() => k,
        _ => {
            eprintln!("Skipping real_tiktok_search_second_page_offset - TIKHUB_API_KEY empty");
            return;
        }
    };
    let base_url = std::env::var("TIKHUB_BASE_URL")
        .unwrap_or_else(|_| "https://api.tikhub.io".to_string());

    let client = TikHubClient::with_retry_config(api_key, base_url, zero_retry_config)
        .expect("DR-19 zero-retry client creation must succeed");

    // ---- HTTP call #1: Page 1 (offset=0) ----
    let page1_params = SearchParams::new("fitness")
        .with_count(10)
        .with_region("US")
        .with_offset(0);

    let page1_resp = client
        .search_videos(&page1_params)
        .await
        .expect("T-051: page-1 search_videos must succeed");

    let page1_data = page1_resp
        .data
        .as_ref()
        .expect("T-051: page-1 response must contain data field");

    let page1_has_more = page1_data.has_more;
    let page1_cursor = page1_data.cursor;

    let page1_videos = TikHubClient::extract_videos(&page1_resp);
    assert!(
        !page1_videos.is_empty(),
        "T-051: page-1 must return at least 1 video (got 0)"
    );

    let page1_ids: std::collections::HashSet<String> =
        page1_videos.iter().map(|v| v.aweme_id.clone()).collect();

    eprintln!(
        "[AG-008] Page-1: has_more={:?}, cursor={:?}, video_count={}",
        page1_has_more,
        page1_cursor,
        page1_videos.len()
    );

    // ---- cursor 推进语义对账 ----
    // M3-T2 assumes: next_offset = cursor (i64 cast to u32 via cursor.max(0) as u32).
    // Validate cursor is in u32 range to confirm the assumption holds.
    let cursor_val = page1_cursor.unwrap_or(0);
    assert!(
        cursor_val >= 0,
        "T-051: cursor must be non-negative (got {}); M3-T2 offset=cursor assumption violated",
        cursor_val
    );
    // ASSERTION-CHANGE-JUSTIFIED guard: if cursor > u32::MAX, stop and report.
    // We do NOT silently continue — this assertion expresses the M3-T2 contract.
    assert!(
        cursor_val <= i64::from(u32::MAX),
        "T-051 CONFLICT: cursor={} exceeds u32::MAX={}; M3-T2 offset=cursor.max(0) as u32 \
         assumption violated. STOP — report to root, walk ASSERTION-CHANGE-JUSTIFIED process, \
         do NOT silently change mock.",
        cursor_val,
        u32::MAX
    );

    let next_offset = cursor_val.max(0) as u32;

    // Only attempt page 2 if page 1 indicated there is more data.
    if page1_has_more != Some(1) {
        eprintln!(
            "[AG-008] Page-1 has_more={:?} — no second page available for keyword 'fitness' \
             at this moment; test passes vacuously (cursor semantics verified above).",
            page1_has_more
        );
        return;
    }

    // ---- HTTP call #2: Page 2 (offset=cursor from page 1) ----
    let page2_params = SearchParams::new("fitness")
        .with_count(10)
        .with_region("US")
        .with_offset(next_offset);

    let page2_resp = client
        .search_videos(&page2_params)
        .await
        .expect("T-051: page-2 search_videos must succeed");

    let page2_videos = TikHubClient::extract_videos(&page2_resp);

    eprintln!(
        "[AG-008] Page-2: video_count={}, offset_used={}",
        page2_videos.len(),
        next_offset
    );

    // Assertion 1: second page must be non-empty.
    assert!(
        !page2_videos.is_empty(),
        "T-051: page-2 (offset={}) must return at least 1 video",
        next_offset
    );

    let page2_ids: std::collections::HashSet<String> =
        page2_videos.iter().map(|v| v.aweme_id.clone()).collect();

    // Assertion 2: aweme_id sets must not be identical (partial overlap allowed).
    // This validates that offset/cursor actually advances the page, not re-fetching page 1.
    assert_ne!(
        page1_ids, page2_ids,
        "T-051: page-2 aweme_id set must differ from page-1 \
         (full identity means offset/cursor did NOT advance pagination)"
    );

    let overlap: std::collections::HashSet<_> = page1_ids.intersection(&page2_ids).collect();
    eprintln!(
        "[AG-008] Overlap between page-1 and page-2: {}/{} ids (partial overlap is OK)",
        overlap.len(),
        page2_ids.len()
    );

    // Summary: cursor semantics confirmed — offset=cursor (i64→u32) advances the page.
    eprintln!(
        "[AG-008] PASSED: T-051 second-page probe complete. \
         cursor_val={}, next_offset={}, page1_count={}, page2_count={}, overlap={}. \
         Fixture backfill obligation: tests/fixtures/tiktok/search_fitness_us_page2.json",
        cursor_val,
        next_offset,
        page1_ids.len(),
        page2_ids.len(),
        overlap.len()
    );
}

// ============================================================
// All APIs Summary Test (Search + Comments)
// ============================================================

#[tokio::test]
async fn test_all_apis_comprehensive() {
    if !live_api_tests_enabled() {
        eprintln!("Skipping test_all_apis_comprehensive - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent");
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n{}", "=".repeat(70));
    println!("🧪 Comprehensive API Test (Search + Comments)");
    println!("{}", "=".repeat(70));

    let mut results: Vec<(&str, &str, bool, String)> = Vec::new();

    // TikTok Search
    print!("\n📱 TikTok Search... ");
    let tiktok_search = client
        .search_videos_with_retry(&SearchParams::new("fitness").with_count(3))
        .await;
    let tiktok_video_id = match &tiktok_search {
        Ok(r) => {
            let videos = TikHubClient::extract_videos(r);
            let count = videos.len();
            println!("✅ {} videos", count);
            results.push(("TikTok", "Search", true, format!("{} videos", count)));
            videos.first().map(|v| v.aweme_id.clone())
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("TikTok", "Search", false, format!("{:?}", e)));
            None
        }
    };

    // TikTok Comments
    print!("📱 TikTok Comments... ");
    if let Some(ref video_id) = tiktok_video_id {
        match client
            .fetch_comments_with_retry(video_id, &CommentParams::default())
            .await
        {
            Ok(r) => {
                let count = TikHubClient::extract_comments(&r).len();
                println!("✅ {} comments", count);
                results.push(("TikTok", "Comments", true, format!("{} comments", count)));
            }
            Err(e) => {
                println!("❌ {:?}", e);
                results.push(("TikTok", "Comments", false, format!("{:?}", e)));
            }
        }
    } else {
        println!("⏭️  Skipped (no video)");
        results.push(("TikTok", "Comments", false, "Skipped".to_string()));
    }

    // Instagram Search (V1)
    print!("📷 Instagram Search... ");
    let ig_search = client
        .search_hashtag_posts_v1_with_retry("fitness", None)
        .await;
    let ig_post_code = match &ig_search {
        Ok(r) => {
            let count = r
                .data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.hashtag.as_ref())
                .and_then(|h| h.edge_hashtag_to_media.as_ref())
                .and_then(|e| e.edges.as_ref())
                .map(|e| e.len())
                .unwrap_or(0);
            println!("✅ {} posts", count);
            results.push(("Instagram", "Search", true, format!("{} posts", count)));
            r.data
                .as_ref()
                .and_then(|d| d.data.as_ref())
                .and_then(|d| d.hashtag.as_ref())
                .and_then(|h| h.edge_hashtag_to_media.as_ref())
                .and_then(|e| e.edges.as_ref())
                .and_then(|e| e.first())
                .and_then(|e| e.node.as_ref())
                .and_then(|n| n.shortcode.clone())
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("Instagram", "Search", false, format!("{:?}", e)));
            None
        }
    };

    // Instagram Comments (V2 - may fail)
    print!("📷 Instagram Comments... ");
    if let Some(ref code) = ig_post_code {
        match client
            .fetch_instagram_comments_with_retry(&InstagramCommentParams::new(code))
            .await
        {
            Ok(r) => {
                let count = r
                    .data
                    .as_ref()
                    .and_then(|d| d.data.as_ref())
                    .and_then(|d| d.items.as_ref())
                    .map(|l| l.len())
                    .unwrap_or(0);
                println!("✅ {} comments", count);
                results.push(("Instagram", "Comments", true, format!("{} comments", count)));
            }
            Err(e) => {
                println!("⚠️  {:?}", e);
                results.push((
                    "Instagram",
                    "Comments",
                    false,
                    "API error (expected)".to_string(),
                ));
            }
        }
    } else {
        println!("⏭️  Skipped (no post)");
        results.push(("Instagram", "Comments", false, "Skipped".to_string()));
    }

    // Reddit Search
    print!("🤖 Reddit Search... ");
    let reddit_search = client
        .search_reddit_posts_with_retry(&RedditSearchParams::new("rust"))
        .await;
    let reddit_post_id = match &reddit_search {
        Ok(r) => {
            let posts = r
                .data
                .as_ref()
                .map(|d| extract_posts_from_search(d))
                .unwrap_or_default();
            let count = posts.len();
            println!("✅ {} posts", count);
            results.push(("Reddit", "Search", true, format!("{} posts", count)));
            posts.first().and_then(|p| p.post_id())
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("Reddit", "Search", false, format!("{:?}", e)));
            None
        }
    };

    // Reddit Comments
    print!("🤖 Reddit Comments... ");
    if let Some(post_id) = reddit_post_id {
        let full_id = if post_id.starts_with("t3_") {
            post_id
        } else {
            format!("t3_{}", post_id)
        };
        match client
            .fetch_reddit_comments_with_retry(&RedditCommentParams::new(&full_id))
            .await
        {
            Ok(r) => {
                let count = r
                    .data
                    .as_ref()
                    .and_then(|d| d.post_info_by_id.as_ref())
                    .and_then(|p| p.comment_forest.as_ref())
                    .and_then(|f| f.trees.as_ref())
                    .map(|t| extract_comments_from_trees(t, None).len())
                    .unwrap_or(0);
                println!("✅ {} comments", count);
                results.push(("Reddit", "Comments", true, format!("{} comments", count)));
            }
            Err(e) => {
                println!("❌ {:?}", e);
                results.push(("Reddit", "Comments", false, format!("{:?}", e)));
            }
        }
    } else {
        println!("⏭️  Skipped (no post)");
        results.push(("Reddit", "Comments", false, "Skipped".to_string()));
    }

    // Twitter Search
    print!("🐦 Twitter Search... ");
    let twitter_search = client
        .search_twitter_tweets_with_retry(&TwitterSearchParams::new("rust"))
        .await;
    let twitter_tweet_id = match &twitter_search {
        Ok(r) => {
            let count = r
                .data
                .as_ref()
                .and_then(|d| d.timeline.as_ref())
                .map(|l| l.len())
                .unwrap_or(0);
            println!("✅ {} tweets", count);
            results.push(("Twitter", "Search", true, format!("{} tweets", count)));
            r.data
                .as_ref()
                .and_then(|d| d.timeline.as_ref())
                .and_then(|t| t.first())
                .and_then(|t| t.get_tweet_id().map(|s| s.to_string()))
        }
        Err(e) => {
            println!("❌ {:?}", e);
            results.push(("Twitter", "Search", false, format!("{:?}", e)));
            None
        }
    };

    // Twitter Comments
    print!("🐦 Twitter Comments... ");
    if let Some(ref tweet_id) = twitter_tweet_id {
        match client
            .fetch_twitter_comments_with_retry(&TwitterCommentParams::new(tweet_id))
            .await
        {
            Ok(r) => {
                let count = r
                    .data
                    .as_ref()
                    .and_then(|d| d.thread.as_ref())
                    .map(|l| l.len())
                    .unwrap_or(0);
                println!("✅ {} replies", count);
                results.push(("Twitter", "Comments", true, format!("{} replies", count)));
            }
            Err(e) => {
                println!("❌ {:?}", e);
                results.push(("Twitter", "Comments", false, format!("{:?}", e)));
            }
        }
    } else {
        println!("⏭️  Skipped (no tweet)");
        results.push(("Twitter", "Comments", false, "Skipped".to_string()));
    }

    // Summary
    println!("\n{}", "=".repeat(70));
    println!("📊 Comprehensive Summary:");
    println!("{}", "-".repeat(70));

    let search_success = results
        .iter()
        .filter(|(_, t, ok, _)| *t == "Search" && *ok)
        .count();
    let comment_success = results
        .iter()
        .filter(|(_, t, ok, _)| *t == "Comments" && *ok)
        .count();

    for (platform, api_type, ok, detail) in &results {
        let status = if *ok { "✅" } else { "❌" };
        println!("  {} {} {}: {}", status, platform, api_type, detail);
    }

    println!("{}", "-".repeat(70));
    println!("  Search APIs: {}/4 succeeded", search_success);
    println!("  Comment APIs: {}/4 succeeded", comment_success);
    println!("{}", "=".repeat(70));
}

// ============================================================
// M5-T1 V1 probe: Instagram general_search pagination-token capability
// (m5-instagram-p2.md §3 M5-T1; covers V1 / N-001 / T-053 / P-004 / C-005 / PV-005)
// ============================================================

/// V1 探测(gated):TikHub Instagram `general_search` V3 是否接受分页 token(`next_max_id`/
/// `rank_token`)。**判定标准(§2.1,原文采纳)**:带首页 token 重发——非错误且内容异于首页
/// → 支持翻页(分支 A);4xx 或返回相同首页 → 单页能力(分支 B)。
///
/// **AG-008 + 判定写回义务**:本探测产出**判定事实**(非回归断言)。控制器已据混合代码证据
/// (V2 有 pagination_token;V3 请求端无 token)+ 本机无 TIKHUB_API_KEY,由**用户裁决取保守
/// 分支 B**(2026-06-11),并已把 assumptions=分支 B 写回 `ledgers/assumptions.md` V1 行 /
/// `ledgers/cross-service-contracts.md` C-005。**若日后实跑确认支持翻页,可经 root 升级分支 A**
/// (届时 T3-A 解除 NOT-TAKEN,本探测的原始两页 JSON 回灌 `tests/fixtures/instagram/`)。
///
/// **预算(P-004 / DR-19)**:≤4 HTTP 请求上界;探测用**零重试**调用(`search_instagram_general`,
/// 非 `_with_retry`),确保「调用数 = 请求数」。本机无凭据 → 自动 skip。
/// **探测型,无 RED→GREEN 语义(允许先绿)**:断言仅「调用成功 + 判定逻辑可复算」,
/// 如实记录两种合法结局。
/// **反作弊**:判定不得「按希望的分支」倾向解读;模糊结果(token 接受但内容相同)按标准判为
/// 分支 B(保守),并记录原始证据。
#[tokio::test]
async fn real_instagram_general_search_pagination_probe() {
    if !live_api_tests_enabled() {
        eprintln!(
            "Skipping real_instagram_general_search_pagination_probe - credentials unset or CI opt-in (RUN_REAL_API_TESTS) absent"
        );
        return;
    }
    let Some(client) = create_client() else {
        return;
    };

    println!("\n🔬 M5-T1 V1 probe: Instagram general_search pagination-token capability...");

    // Call #1 (zero-retry): V3 first page. Record next_max_id / rank_token / has_more.
    let first = match client.search_instagram_general("#fitness").await {
        Ok(resp) => resp,
        Err(e) => {
            // First-page failure is itself evidence the V3 path is not usable for pagination
            // probing; record and stop (still within budget). Does NOT auto-flip the verdict.
            println!("⚠️  V3 first-page call errored: {:?} (verdict stays branch B, conservative)", e);
            return;
        }
    };

    let grid = first
        .data
        .as_ref()
        .and_then(|d| d.media_grid.as_ref());
    let next_max_id = grid.and_then(|g| g.next_max_id.clone());
    let rank_token = grid
        .and_then(|g| g.rank_token.clone())
        .or_else(|| first.data.as_ref().and_then(|d| d.rank_token.clone()));
    let has_more = grid.and_then(|g| g.has_more);
    let first_posts = TikHubClient::extract_instagram_general_posts(&first);
    let first_codes: Vec<String> = first_posts
        .iter()
        .map(|p| p.code.clone().unwrap_or_default())
        .collect();

    println!(
        "   first page: posts={}, next_max_id={:?}, rank_token={:?}, has_more={:?}",
        first_codes.len(),
        next_max_id,
        rank_token,
        has_more
    );

    // Assertion (probe-type): the first call succeeded and the verdict logic is recomputable.
    assert_eq!(first.code, 200, "probe requires a successful first-page call");

    // Re-send with first-page token (§2.1). NOTE: the current TikHubClient V3 general_search
    // exposes NO request-side pagination parameter (request端无 token,控制器实证);there is no
    // client method to forward `next_max_id`/`rank_token`. Per the §2.1 conservative standard,
    // "no token-acceptance path observable" → 单页能力(branch B). When a token-forwarding
    // client method is added (升级分支 A 时), this probe should re-send and compare page-2 codes
    // against `first_codes`: different & non-error ⇒ branch A; 4xx or identical ⇒ branch B.
    let verdict = if next_max_id.is_some() && has_more == Some(true) {
        // Token surfaced on the response side, but request side cannot forward it today.
        "INCONCLUSIVE-token-present-but-no-request-param → conservative branch B (per §2.1)"
    } else {
        "branch B (single-page: no next_max_id / has_more!=true)"
    };
    println!("   📌 verdict: {verdict}");
    println!(
        "   (ledger writeback already recorded branch B by controller decision 2026-06-11; \
         raw first-page codes captured: {first_codes:?})"
    );
}
