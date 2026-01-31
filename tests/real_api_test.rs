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
    TwitterCommentParams,
    // Twitter
    TwitterSearchParams,
};

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

// ============================================================
// Instagram API Tests
// ============================================================

#[tokio::test]
async fn test_instagram_hashtag_search_v1_real() {
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
async fn test_instagram_v2_hashtag_search_real() {
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
// All APIs Summary Test (Search + Comments)
// ============================================================

#[tokio::test]
async fn test_all_apis_comprehensive() {
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
