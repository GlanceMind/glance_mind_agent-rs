//! End-to-End Integration Test with Real Database
//!
//! This test simulates a complete workflow using:
//! - Real PostgreSQL database (Docker container on port 5433)
//! - Real Redis (Docker container on port 6380)
//! - Mock TikHub adapter (returns fixture data)
//! - Mock AI analyzer (always returns "OK")
//!
//! Test Scenario: "中国旅游" (China Travel) Campaign on TikTok
//!
//! ## Prerequisites
//!
//! Start the test containers:
//! ```bash
//! cd crates/db
//! docker-compose -f docker-compose.test.yml up -d
//! ```
//!
//! ## Running the Test
//!
//! ```bash
//! cargo test --test e2e_real_db_test -- --ignored --nocapture
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use diesel::prelude::*;

use glance_mind_agent_rs::{
    // Domain errors
    domain::errors::{AiResult, GatewayResult},
    init_global_registry,
    // Port types
    ports::ai_analyzer::AnalysisContext,
    // Protocol types
    protocol_gen::Platform,
    AiAnalyzer,
    Comment,
    CommentGateway,
    // Domain
    Content,
    // Ports
    ContentGateway,
    ContentRepository,
    CrawlerTaskBuilder,
    Engagement,
    OrchestratorConfig,
    // Config
    PlatformRegistry,
    // Adapters
    PostgresAdapter,
    ProgressTracker,
    PromptRepository,
    RedisTaskConsumer,
    ReplySuggestion,
    TaskConfig,
    // Strategies
    TikTokStrategy,
    // Orchestrator
    WorkflowOrchestrator,
};

// ============================================================
// Test Configuration
// ============================================================

const TEST_DATABASE_URL: &str =
    "postgresql://glancemind:testpassword@localhost:5433/glancemind_test";
const TEST_REDIS_URL: &str = "redis://localhost:6380";
const TEST_QUEUE_NAME: &str = "gm:agent:test:tasks";

// Campaign for testing
const TEST_CAMPAIGN_ID: i32 = 1000;
const TEST_USER_ID: i32 = 2; // Existing test user from init-test-data.sql

// ============================================================
// Mock TikHub Adapter - Returns "中国旅游" fixture data
// ============================================================

/// Mock TikHub adapter that returns predefined China Travel video
struct MockTikHubAdapter {
    video: Content,
    comments: Vec<Comment>,
}

impl MockTikHubAdapter {
    fn new() -> Self {
        // Create a China Travel video
        let video = Content::new("tiktok", "china_travel_video_001")
            .with_author("china_travel_guide")
            .with_author_name("中国旅游达人")
            .with_description(
                "探索中国最美的地方！从长城到桂林，带你看遍中国🇨🇳 #中国旅游 #travel #china"
                    .to_string(),
            )
            .with_url("https://tiktok.com/@china_guide/video/001")
            .with_engagement(Engagement {
                likes: 50000,
                comments: 1200,
                shares: 3000,
                views: 500000,
            })
            .with_created_at(1706000000);

        // Create some comments
        let comments = vec![
            Comment::new("tiktok", "cmt_001", "china_travel_video_001")
                .with_author("travel_fan_1")
                .with_author_name("旅游爱好者")
                .with_text("好想去长城啊！有推荐的旅行社吗？".to_string())
                .with_likes(150)
                .with_created_at(1706001000),
            Comment::new("tiktok", "cmt_002", "china_travel_video_001")
                .with_author("curious_tourist")
                .with_author_name("Curious Tourist")
                .with_text("How much does a trip to China cost?".to_string())
                .with_likes(85)
                .with_created_at(1706002000),
            Comment::new("tiktok", "cmt_003", "china_travel_video_001")
                .with_author("food_lover_88")
                .with_author_name("吃货小王")
                .with_text("北京烤鸭哪家最正宗？".to_string())
                .with_likes(200)
                .with_created_at(1706003000),
        ];

        Self { video, comments }
    }
}

use glance_mind_agent_rs::{
    ports::comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    KeywordType, SearchOptions,
};

#[async_trait]
impl ContentGateway for MockTikHubAdapter {
    async fn search(&self, _options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        // Return only 1 video as requested
        Ok(vec![self.video.clone()])
    }

    async fn fetch_by_keyword(
        &self,
        _keyword: &KeywordType,
        _options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        Ok(vec![self.video.clone()])
    }

    async fn fetch_user_content(&self, _user_id: &str, _count: u32) -> GatewayResult<Vec<Content>> {
        Ok(vec![self.video.clone()])
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        if content_id == self.video.content_id {
            Ok(Some(self.video.clone()))
        } else {
            Ok(None)
        }
    }

    fn platform(&self) -> &str {
        "tiktok"
    }
}

#[async_trait]
impl CommentGateway for MockTikHubAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        _options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        if content_id == self.video.content_id {
            Ok(FetchCommentsResult::new(self.comments.clone()))
        } else {
            Ok(FetchCommentsResult::empty())
        }
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        _max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        if content_id == self.video.content_id {
            Ok(self.comments.clone())
        } else {
            Ok(vec![])
        }
    }

    async fn fetch_replies(
        &self,
        _content_id: &str,
        _comment_id: &str,
        _options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        Ok(vec![])
    }

    fn platform(&self) -> &str {
        "tiktok"
    }
}

// ============================================================
// Mock AI Analyzer - Always returns "OK"
// ============================================================

/// Mock AI analyzer that always returns "OK" as the suggested reply
struct AlwaysOkAiAnalyzer;

#[async_trait]
impl AiAnalyzer for AlwaysOkAiAnalyzer {
    async fn analyze_comment(
        &self,
        comment: &Comment,
        _content: &Content,
        _context: &AnalysisContext,
    ) -> AiResult<ReplySuggestion> {
        // Always return "OK" for every comment
        Ok(ReplySuggestion::new(&comment.comment_id)
            .with_reply("OK")
            .with_dm("OK - 私信")
            .with_post_reply("OK - 帖子回复")
            .with_reason("统一回复测试")
            .with_model_info("mock-always-ok", 10))
    }

    async fn analyze_batch(
        &self,
        comments: &[Comment],
        content: &Content,
        context: &AnalysisContext,
    ) -> AiResult<Vec<ReplySuggestion>> {
        let mut results = Vec::with_capacity(comments.len());
        for comment in comments {
            results.push(self.analyze_comment(comment, content, context).await?);
        }
        Ok(results)
    }

    async fn health_check(&self) -> AiResult<bool> {
        Ok(true)
    }

    fn model_name(&self) -> &str {
        "mock-always-ok"
    }
}

// ============================================================
// Database Schema (for direct queries)
// ============================================================

diesel::table! {
    gm_content (id) {
        id -> Int4,
        platform_id -> Int4,
        content_id -> Varchar,
        author_unique_id -> Nullable<Varchar>,
        author_nickname -> Nullable<Varchar>,
        description -> Nullable<Text>,
        content_url -> Nullable<Varchar>,
        likes -> Nullable<Int8>,
        comments -> Nullable<Int8>,
        shares -> Nullable<Int8>,
        views -> Nullable<Int8>,
        content_created_at -> Nullable<Int8>,
        raw_data -> Nullable<Jsonb>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        campaign_id -> Nullable<Int4>,
    }
}

diesel::table! {
    gm_comments (id) {
        id -> Int4,
        platform_id -> Int4,
        content_id -> Int4,
        comment_id -> Varchar,
        parent_comment_id -> Nullable<Varchar>,
        author_uid -> Nullable<Varchar>,
        author_unique_id -> Nullable<Varchar>,
        author_nickname -> Nullable<Varchar>,
        comment_text -> Nullable<Text>,
        likes -> Nullable<Int8>,
        reply_count -> Nullable<Int4>,
        comment_created_at -> Nullable<Int8>,
        is_reply -> Bool,
        raw_data -> Nullable<Jsonb>,
        status -> Int2,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    gm_ai_analysis (id) {
        id -> Int4,
        comment_id -> Int4,
        campaign_id -> Int4,
        suggested_reply -> Nullable<Text>,
        suggested_dm -> Nullable<Text>,
        suggested_reply_post -> Nullable<Text>,
        reason -> Nullable<Text>,
        tokens_used -> Nullable<Int4>,
        model_name -> Nullable<Varchar>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    gm_campaigns (id) {
        id -> Int4,
        user_id -> Int4,
        name -> Varchar,
        platform_id -> Int4,
        status -> Int2,
        target_audience -> Nullable<Text>,
        product_prompt -> Nullable<Text>,
        reply_strategy -> Nullable<Text>,
        dm_strategy -> Nullable<Text>,
        reply_post_strategy -> Nullable<Text>,
        max_comments -> Nullable<Int4>,
        processed_comments -> Int4,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

#[derive(QueryableByName)]
struct TaskTerminalReasonRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    status: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    terminal_reason: Option<String>,
}

// ============================================================
// Test Helpers
// ============================================================

/// Check if test infrastructure is available
async fn check_test_infrastructure() -> bool {
    // Check PostgreSQL
    let pg_ok = glance_mind_agent_rs::db::establish_pool(TEST_DATABASE_URL, Some(1)).is_ok();
    if !pg_ok {
        eprintln!("PostgreSQL not available at {}", TEST_DATABASE_URL);
        return false;
    }

    // Check Redis - just verify client can be created (actual connection tested async)
    let redis_ok = redis::Client::open(TEST_REDIS_URL).is_ok();
    if !redis_ok {
        eprintln!("Redis not available at {}", TEST_REDIS_URL);
        return false;
    }

    true
}

/// Setup test campaign in database
fn setup_test_campaign(conn: &mut PgConnection) -> Result<(), diesel::result::Error> {
    // Insert test campaign for "中国旅游"
    diesel::sql_query(format!(
        r#"
        INSERT INTO gm_campaigns (
            id, user_id, name, platform_id, status, 
            product_prompt, max_comments, processed_comments
        ) VALUES (
            {}, {}, '中国旅游测试活动', 2, 1,
            '我们提供优质的中国旅游服务，包括长城、故宫、桂林等热门景点。', 1000, 0
        ) ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            status = 1,
            processed_comments = 0
    "#,
        TEST_CAMPAIGN_ID, TEST_USER_ID
    ))
    .execute(conn)?;

    Ok(())
}

/// Cleanup test data
fn cleanup_test_data(conn: &mut PgConnection) -> Result<(), diesel::result::Error> {
    // Delete AI analysis for test campaign
    diesel::sql_query(format!(
        "DELETE FROM gm_ai_analysis WHERE campaign_id = {}",
        TEST_CAMPAIGN_ID
    ))
    .execute(conn)?;

    // Delete comments for test content
    diesel::sql_query(format!(
        "DELETE FROM gm_comments WHERE content_id IN (SELECT id FROM gm_content WHERE campaign_id = {})",
        TEST_CAMPAIGN_ID
    ))
    .execute(conn)?;

    // Delete content for test campaign
    diesel::sql_query(format!(
        "DELETE FROM gm_content WHERE campaign_id = {}",
        TEST_CAMPAIGN_ID
    ))
    .execute(conn)?;

    Ok(())
}

// ============================================================
// Main E2E Test
// ============================================================

/// End-to-end test with real PostgreSQL and Redis
///
/// This test:
/// 1. Creates a test campaign in the database
/// 2. Runs the workflow with mock TikHub and AI
/// 3. Verifies data is correctly saved to PostgreSQL
#[tokio::test]
#[ignore] // Run with: cargo test --test e2e_real_db_test -- --ignored --nocapture
async fn test_china_travel_campaign_e2e() {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    println!("\n========================================");
    println!("E2E Test: 中国旅游 (China Travel) Campaign");
    println!("========================================\n");

    // Check infrastructure
    if !check_test_infrastructure().await {
        println!("⚠️  Test infrastructure not available. Skipping test.");
        println!("   Start containers with: cd crates/db && docker-compose -f docker-compose.test.yml up -d");
        return;
    }

    // Initialize platform registry
    init_global_registry(PlatformRegistry::with_defaults());

    // Setup database connection
    let pool = glance_mind_agent_rs::db::establish_pool(TEST_DATABASE_URL, Some(5))
        .expect("Failed to create database pool");
    let mut conn = pool.get().expect("Failed to get database connection");

    // Cleanup any previous test data
    println!("🧹 Cleaning up previous test data...");
    cleanup_test_data(&mut conn).expect("Failed to cleanup test data");

    // Setup test campaign
    println!("📝 Setting up test campaign: 中国旅游...");
    setup_test_campaign(&mut conn).expect("Failed to setup test campaign");

    // Create adapters
    println!("🔧 Creating adapters...");
    let tikhub = Arc::new(MockTikHubAdapter::new());
    let ai = Arc::new(AlwaysOkAiAnalyzer);
    let postgres = Arc::new(PostgresAdapter::new(pool.clone()));

    // Build orchestrator
    println!("🏗️  Building orchestrator...");
    let orchestrator = WorkflowOrchestrator::builder()
        .add_content_gateway("tiktok", tikhub.clone() as Arc<dyn ContentGateway>)
        .add_comment_gateway("tiktok", tikhub as Arc<dyn CommentGateway>)
        .ai_analyzer(ai as Arc<dyn AiAnalyzer>)
        .content_repository(postgres.clone() as Arc<dyn ContentRepository>)
        .prompt_repository(postgres.clone() as Arc<dyn PromptRepository>)
        .progress_tracker(postgres as Arc<dyn ProgressTracker>)
        .add_strategy(Arc::new(TikTokStrategy::new()))
        .config(OrchestratorConfig {
            max_videos_per_keyword: 1, // Only 1 video
            max_comments_per_video: 10,
            ai_batch_size: 5,
            continue_on_error: true,
        })
        .build()
        .expect("Failed to build orchestrator");

    // Create task config
    let task_config = TaskConfig::new(TEST_CAMPAIGN_ID, "tiktok")
        .with_keywords(vec!["中国旅游".to_string()])
        .with_region("US".to_string())
        .with_max_videos(1)
        .with_max_comments_per_video(10);

    // Run workflow
    println!("\n🚀 Running workflow for '中国旅游'...\n");
    let result = orchestrator
        .process_task(TEST_CAMPAIGN_ID as i64, task_config)
        .await;

    match &result {
        Ok(task_result) => {
            println!("✅ Workflow completed successfully!");
            println!(
                "   - Contents processed: {}",
                task_result.contents_processed
            );
            println!(
                "   - Comments processed: {}",
                task_result.comments_processed
            );
            println!(
                "   - AI analyses generated: {}",
                task_result.analyses_generated
            );
            if let Some(ref err) = task_result.error {
                println!("   - Error: {}", err);
            }
        }
        Err(e) => {
            println!("❌ Workflow failed: {:?}", e);
        }
    }

    // Verify database results
    println!("\n📊 Verifying database results...\n");

    // Check content was saved
    let content_count: i64 = gm_content::table
        .filter(gm_content::campaign_id.eq(TEST_CAMPAIGN_ID))
        .count()
        .get_result(&mut conn)
        .expect("Failed to query content");

    println!("   Content records: {}", content_count);
    assert_eq!(content_count, 1, "Expected 1 video to be saved");

    // Get the content ID for comment lookup
    let content_db_id: i32 = gm_content::table
        .filter(gm_content::campaign_id.eq(TEST_CAMPAIGN_ID))
        .select(gm_content::id)
        .first(&mut conn)
        .expect("Failed to get content ID");

    // Check comments were saved
    let comment_count: i64 = gm_comments::table
        .filter(gm_comments::content_id.eq(content_db_id))
        .count()
        .get_result(&mut conn)
        .expect("Failed to query comments");

    println!("   Comment records: {}", comment_count);
    assert_eq!(comment_count, 3, "Expected 3 comments to be saved");

    // Check AI analysis was saved
    let analysis_count: i64 = gm_ai_analysis::table
        .filter(gm_ai_analysis::campaign_id.eq(TEST_CAMPAIGN_ID))
        .count()
        .get_result(&mut conn)
        .expect("Failed to query analysis");

    println!("   AI Analysis records: {}", analysis_count);
    assert_eq!(analysis_count, 3, "Expected 3 AI analysis records");

    // Verify AI responses are all "OK"
    let suggested_replies: Vec<Option<String>> = gm_ai_analysis::table
        .filter(gm_ai_analysis::campaign_id.eq(TEST_CAMPAIGN_ID))
        .select(gm_ai_analysis::suggested_reply)
        .load(&mut conn)
        .expect("Failed to query suggested replies");

    println!("\n   Verifying AI responses:");
    for (i, reply) in suggested_replies.iter().enumerate() {
        let reply_text = reply.as_deref().unwrap_or("(empty)");
        println!("     Comment {}: {}", i + 1, reply_text);
        assert_eq!(reply_text, "OK", "Expected AI reply to be 'OK'");
    }

    // Verify workflow completion persisted terminal_reason when this E2E DB
    // includes the production crawler task row for this task id.
    if let Ok(task_terminal_reason) = diesel::sql_query(format!(
        "SELECT status, terminal_reason FROM gm_crawler_tasks WHERE id = {}",
        TEST_CAMPAIGN_ID
    ))
    .get_result::<TaskTerminalReasonRow>(&mut conn)
    {
        assert_eq!(task_terminal_reason.status, "completed");
        assert_eq!(
            task_terminal_reason.terminal_reason.as_deref(),
            Some("COMPLETED: Task completed successfully")
        );
    }

    // Verify content details
    let saved_content: (String, Option<String>, Option<String>) = gm_content::table
        .filter(gm_content::campaign_id.eq(TEST_CAMPAIGN_ID))
        .select((
            gm_content::content_id,
            gm_content::author_nickname,
            gm_content::description,
        ))
        .first(&mut conn)
        .expect("Failed to query content details");

    println!("\n   Saved content:");
    println!("     Video ID: {}", saved_content.0);
    println!(
        "     Author: {}",
        saved_content.1.as_deref().unwrap_or("N/A")
    );
    let desc_preview = saved_content.2.as_deref().unwrap_or("N/A");
    println!(
        "     Description: {}...",
        &desc_preview[..50.min(desc_preview.len())]
    );

    // Final summary
    println!("\n========================================");
    println!("✅ E2E Test PASSED!");
    println!("========================================");
    println!("Summary:");
    println!("  - Campaign: 中国旅游测试活动 (ID: {})", TEST_CAMPAIGN_ID);
    println!("  - Platform: TikTok");
    println!("  - Videos saved: {}", content_count);
    println!("  - Comments saved: {}", comment_count);
    println!("  - AI analyses: {}", analysis_count);
    println!("  - All AI replies are 'OK': ✅");
    println!("========================================\n");

    // Cleanup (optional - comment out to inspect data)
    // cleanup_test_data(&mut conn).expect("Failed to cleanup");
}

/// Test Redis task queue integration
#[tokio::test]
#[ignore]
async fn test_redis_task_queue_integration() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    println!("\n========================================");
    println!("Redis Task Queue Integration Test");
    println!("========================================\n");

    // Check infrastructure
    if !check_test_infrastructure().await {
        println!("⚠️  Test infrastructure not available. Skipping test.");
        return;
    }

    // Initialize platform registry
    init_global_registry(PlatformRegistry::with_defaults());

    // Create Redis consumer and initialize connection
    let mut consumer = RedisTaskConsumer::new(TEST_REDIS_URL, TEST_QUEUE_NAME)
        .expect("Failed to create Redis consumer");
    consumer
        .init()
        .await
        .expect("Failed to initialize Redis connection");

    // Create a test task (simulating scheduler)
    let task = CrawlerTaskBuilder::new(TEST_CAMPAIGN_ID as i64)
        .platform(Platform::Tiktok)
        .keywords(vec!["中国旅游".to_string()])
        .region("US")
        .max_count(1)
        .build();

    // Publish task to queue
    println!("📤 Publishing task to Redis queue...");
    let task_json = serde_json::to_string(&task).expect("Failed to serialize task");

    let client = redis::Client::open(TEST_REDIS_URL).expect("Failed to create Redis client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect to Redis");

    let _: () = redis::cmd("LPUSH")
        .arg(TEST_QUEUE_NAME)
        .arg(&task_json)
        .query_async(&mut conn)
        .await
        .expect("Failed to publish task");

    println!("✅ Task published: {}", task_json);

    // Consume task
    println!("\n📥 Consuming task from queue...");
    let consumed = consumer.try_consume().await;

    match consumed {
        Ok(Some(consumed_task)) => {
            use glance_mind_agent_rs::CrawlerTaskExt;
            println!("✅ Task consumed successfully!");
            println!("   Task ID: {}", consumed_task.task_id());
            println!("   Platform: {}", consumed_task.platform_name());
            println!("   Keywords: {:?}", consumed_task.keywords());

            assert_eq!(consumed_task.task_id(), TEST_CAMPAIGN_ID as i64);
            assert_eq!(consumed_task.platform_name(), "tiktok");
        }
        Ok(None) => {
            panic!("No task found in queue");
        }
        Err(e) => {
            panic!("Failed to consume task: {:?}", e);
        }
    }

    println!("\n✅ Redis integration test PASSED!\n");
}
