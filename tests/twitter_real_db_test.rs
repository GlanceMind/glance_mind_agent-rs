//! Real Twitter workflow test against a real PostgreSQL database.
//!
//! This test uses:
//! - Real TikHub Twitter API calls
//! - Real `PostgresAdapter` persistence into `gm_agent_twitter_tweets/comments`
//! - Real `WorkflowOrchestrator` processing flow
//! - Test doubles only for AI/prompt/progress dependencies
//!
//! Run with:
//! `DATABASE_URL=... TIKHUB_API_KEY=... cargo test --test twitter_real_db_test -- --nocapture`

#[path = "support/twitter_live.rs"]
mod twitter_live;

use std::sync::Arc;

use async_trait::async_trait;
use diesel::prelude::*;
use diesel::sql_types::Integer;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use glance_mind_agent_rs::{
    db::{models, schema},
    domain::errors::{AiResult, DbResult, GatewayResult},
    ports::{
        ai_analyzer::AnalysisContext,
        comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
        progress_tracker::{
            CampaignStopResult, TaskInfo, TaskProgressUpdate, TaskStatus, TaskTerminalReason,
        },
        prompt_repository::{CampaignConfig, CampaignStatus, PlatformConfig},
    },
    tikhub::{TwitterSearchParams, TwitterTweet as RawTwitterTweet},
    AiAnalyzer, Comment, CommentGateway, Content, ContentGateway, KeywordType, OrchestratorConfig,
    PostgresAdapter, ProgressTracker, PromptRepository, ReplySuggestion, SearchOptions, TaskConfig,
    TwitterAdapter, TwitterStrategy, WorkflowOrchestrator,
};

use twitter_live::{
    candidate_exact_queries, create_adapter, create_client, create_strategy,
    fetch_comments_for_content, fetch_first_content_for_keyword, live_test_mutex,
    pick_comment_seed, pick_rest_id_seed, pick_search_seed, push_unique_query,
    retry_tikhub_rate_limit, COMMENT_TARGET_COUNT,
};

const TEST_REPLY: &str = "Thanks from twitter real db test";
const TEST_DM: &str = "DM from twitter real db test";
const TEST_POST_REPLY: &str = "Post reply from twitter real db test";
const TEST_REASON: &str = "twitter real db test reason";
const SEARCH_TYPE: &str = "Top";
const SEARCH_CASE_FALLBACK_QUERIES: &[&str] = &["OpenAI", "rustlang", "langchain"];

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

#[derive(Clone)]
struct WorkflowCaseInput {
    raw_keyword: String,
    extra_pairs: Vec<(String, Value)>,
}

struct StaticAiAnalyzer;

#[async_trait]
impl AiAnalyzer for StaticAiAnalyzer {
    async fn analyze_comment(
        &self,
        comment: &Comment,
        _content: &Content,
        _context: &AnalysisContext,
    ) -> AiResult<ReplySuggestion> {
        Ok(ReplySuggestion::new(comment.comment_id.clone())
            .with_reply(TEST_REPLY)
            .with_dm(TEST_DM)
            .with_post_reply(TEST_POST_REPLY)
            .with_reason(TEST_REASON)
            .with_model_info("twitter-real-db-test", 42))
    }

    async fn analyze_batch(
        &self,
        comments: &[Comment],
        content: &Content,
        context: &AnalysisContext,
    ) -> AiResult<Vec<ReplySuggestion>> {
        let mut suggestions = Vec::with_capacity(comments.len());
        for comment in comments {
            suggestions.push(self.analyze_comment(comment, content, context).await?);
        }
        Ok(suggestions)
    }

    async fn health_check(&self) -> AiResult<bool> {
        Ok(true)
    }

    fn model_name(&self) -> &str {
        "twitter-real-db-test"
    }
}

struct StaticPromptRepository;

#[async_trait]
impl PromptRepository for StaticPromptRepository {
    async fn get_campaign(&self, campaign_id: i32) -> DbResult<Option<CampaignConfig>> {
        Ok(Some(CampaignConfig {
            id: campaign_id,
            user_id: 1,
            name: "twitter-real-db-test".to_string(),
            platform_id: PlatformConfig::TWITTER,
            status: CampaignStatus::Active,
            target_audience: Some("developers".to_string()),
            product_prompt: Some("Developer tooling and automation".to_string()),
            reply_strategy: Some("Be concise and friendly".to_string()),
            dm_strategy: Some("Offer to continue privately when needed".to_string()),
            reply_post_strategy: Some("Invite further discussion".to_string()),
            max_comments: Some(50),
            processed_comments: 0,
        }))
    }

    async fn get_analysis_context(&self, campaign_id: i32) -> DbResult<Option<AnalysisContext>> {
        Ok(self
            .get_campaign(campaign_id)
            .await?
            .map(|campaign| campaign.to_analysis_context()))
    }

    async fn get_platform(&self, platform_id: i32) -> DbResult<Option<PlatformConfig>> {
        Ok(Some(PlatformConfig {
            id: platform_id,
            name: "twitter".to_string(),
            display_name: "Twitter".to_string(),
            is_active: true,
        }))
    }

    async fn get_platform_by_name(&self, name: &str) -> DbResult<Option<PlatformConfig>> {
        Ok(Some(PlatformConfig {
            id: PlatformConfig::TWITTER,
            name: name.to_string(),
            display_name: "Twitter".to_string(),
            is_active: true,
        }))
    }

    async fn should_stop_campaign(&self, _campaign_id: i32) -> DbResult<bool> {
        Ok(false)
    }

    async fn update_processed_count(&self, _campaign_id: i32, _count: i32) -> DbResult<()> {
        Ok(())
    }
}

struct NoOpProgressTracker;

#[async_trait]
impl ProgressTracker for NoOpProgressTracker {
    async fn get_task(&self, task_id: i64) -> DbResult<Option<TaskInfo>> {
        Ok(Some(TaskInfo {
            id: task_id,
            campaign_id: 0,
            platform_id: PlatformConfig::TWITTER,
            keywords: None,
            status: TaskStatus::Pending,
            progress: 0,
            error_message: None,
            terminal_reason: None,
        }))
    }

    async fn update_task_status(&self, _task_id: i64, _status: TaskStatus) -> DbResult<()> {
        Ok(())
    }

    async fn update_task_progress(
        &self,
        _task_id: i64,
        increment: i32,
    ) -> DbResult<TaskProgressUpdate> {
        Ok(TaskProgressUpdate {
            success: true,
            should_stop: false,
            new_process_count: increment,
            new_actual_consumption: 0.0,
        })
    }

    async fn set_task_error(
        &self,
        _task_id: i64,
        _error: &str,
        _terminal_reason: &TaskTerminalReason,
    ) -> DbResult<()> {
        Ok(())
    }

    async fn complete_task(
        &self,
        _task_id: i64,
        _terminal_reason: &TaskTerminalReason,
    ) -> DbResult<()> {
        Ok(())
    }

    async fn fail_task(
        &self,
        _task_id: i64,
        _error: &str,
        _terminal_reason: &TaskTerminalReason,
    ) -> DbResult<()> {
        Ok(())
    }

    async fn should_stop(&self, _task_id: i64) -> DbResult<bool> {
        Ok(false)
    }

    async fn increment_processed(&self, _campaign_id: i32, _count: i32) -> DbResult<()> {
        Ok(())
    }

    async fn get_processed_count(&self, _campaign_id: i32) -> DbResult<i32> {
        Ok(0)
    }

    async fn stop_campaign_gracefully(&self, _campaign_id: i32) -> DbResult<CampaignStopResult> {
        Ok(CampaignStopResult {
            success: true,
            immediate_stopped: true,
            refunded_amount: 0.0,
        })
    }
}

#[derive(Clone, Default)]
struct RecordedWorkflowState {
    content: Arc<Mutex<Option<Content>>>,
    comments: Arc<Mutex<Vec<Comment>>>,
}

struct RecordingTwitterGateway {
    inner: TwitterAdapter,
    state: RecordedWorkflowState,
}

impl RecordingTwitterGateway {
    fn new(inner: TwitterAdapter) -> (Self, RecordedWorkflowState) {
        let state = RecordedWorkflowState::default();
        (
            Self {
                inner,
                state: state.clone(),
            },
            state,
        )
    }

    async fn record_first_content(&self, contents: &[Content]) {
        if let Some(first) = contents.first().cloned() {
            *self.state.content.lock().await = Some(first);
        }
    }
}

#[async_trait]
impl ContentGateway for RecordingTwitterGateway {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let contents = self.inner.search(options).await?;
        self.record_first_content(&contents).await;
        Ok(contents)
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        let contents = self.inner.fetch_by_keyword(keyword, options).await?;
        self.record_first_content(&contents).await;
        Ok(contents)
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        let contents = self.inner.fetch_user_content(user_id, count).await?;
        self.record_first_content(&contents).await;
        Ok(contents)
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        let content = self.inner.fetch_by_id(content_id).await?;
        if let Some(ref content) = content {
            *self.state.content.lock().await = Some(content.clone());
        }
        Ok(content)
    }

    fn platform(&self) -> &str {
        ContentGateway::platform(&self.inner)
    }
}

#[async_trait]
impl CommentGateway for RecordingTwitterGateway {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        self.inner.fetch_comments(content_id, options).await
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        let comments = self.inner.fetch_all_comments(content_id, max_count).await?;
        *self.state.comments.lock().await = comments.clone();
        Ok(comments)
    }

    async fn fetch_replies(
        &self,
        content_id: &str,
        comment_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        self.inner
            .fetch_replies(content_id, comment_id, options)
            .await
    }

    fn platform(&self) -> &str {
        CommentGateway::platform(&self.inner)
    }
}

fn database_url() -> Option<String> {
    let _ = dotenvy::dotenv();
    if std::env::var_os("GITHUB_ACTIONS").is_some()
        && std::env::var_os("RUN_REAL_DB_TESTS").is_none()
    {
        return None;
    }
    std::env::var("DATABASE_URL").ok()
}

fn real_db_tests_enabled() -> bool {
    let enabled = database_url().is_some();
    if !enabled {
        eprintln!(
            "Skipping Twitter real DB workflow test - DATABASE_URL is unset or real DB tests are disabled on GitHub Actions"
        );
    }
    enabled
}

fn connect(database_url: &str) -> PgConnection {
    PgConnection::establish(database_url).expect("failed to connect to DATABASE_URL")
}

fn query_single_id(conn: &mut PgConnection, sql: &str) -> i32 {
    diesel::sql_query(sql)
        .get_result::<IdRow>(conn)
        .map(|row| row.id)
        .expect(sql)
}

fn create_supporting_campaign_and_task(conn: &mut PgConnection) -> (i32, i32) {
    let user_id = query_single_id(conn, "SELECT id FROM gm_users ORDER BY id LIMIT 1");
    let region_id =
        diesel::sql_query("SELECT id FROM gm_regions WHERE platform_id = 5 ORDER BY id LIMIT 1")
            .get_result::<IdRow>(conn)
            .or_else(|_| {
                diesel::sql_query("SELECT id FROM gm_regions ORDER BY id LIMIT 1").get_result(conn)
            })
            .map(|row| row.id)
            .expect("failed to resolve a region for the Twitter test campaign");
    let ai_model_id = query_single_id(conn, "SELECT id FROM gm_ai_models ORDER BY id LIMIT 1");
    let campaign_id = query_single_id(
        conn,
        "SELECT COALESCE(MAX(id), 0) + 1000 AS id FROM gm_campaigns",
    );
    let task_id = query_single_id(
        conn,
        "SELECT COALESCE(MAX(id), 0) + 1000 AS id FROM gm_crawler_tasks",
    );

    diesel::sql_query(format!(
        r#"
        INSERT INTO gm_campaigns (
            id, user_id, name, status, platform_id, region_id, ai_model_id,
            product_prompt, schedule_type, total_scanned,
            auto_like, auto_follow, auto_dm,
            pending_consumption, actual_consumption, is_frozen,
            auto_reply_comments, auto_reply_post, created_at
        ) VALUES (
            {campaign_id}, {user_id}, 'twitter real db test campaign', 'ACTIVE', 5, {region_id}, {ai_model_id},
            'Developer tooling and automation', 'IMMEDIATE', 0,
            false, false, false,
            0.00, 0.00, false,
            true, true, NOW()
        )
        "#
    ))
    .execute(conn)
    .expect("failed to insert supporting campaign");

    diesel::sql_query(format!(
        r#"
        INSERT INTO gm_crawler_tasks (
            id, campaign_id, max_count, process_count, status, search_offset, search_limit, created_at
        ) VALUES (
            {task_id}, {campaign_id}, 1, 0, 'pending', 0, 1, NOW()
        )
        "#
    ))
    .execute(conn)
    .expect("failed to insert supporting crawler task");

    (campaign_id, task_id)
}

fn cleanup_supporting_rows(conn: &mut PgConnection, campaign_id: i32, task_id: i32) {
    let _ = diesel::sql_query(format!(
        "DELETE FROM gm_agent_twitter_comments WHERE campaign_id = {campaign_id}"
    ))
    .execute(conn);
    let _ = diesel::sql_query(format!(
        "DELETE FROM gm_agent_twitter_tweets WHERE task_id = {task_id}"
    ))
    .execute(conn);
    let _ = diesel::sql_query(format!("DELETE FROM gm_crawler_tasks WHERE id = {task_id}"))
        .execute(conn);
    let _ = diesel::sql_query(format!("DELETE FROM gm_campaigns WHERE id = {campaign_id}"))
        .execute(conn);
}

fn as_i32(value: i64) -> i32 {
    i32::try_from(value).expect("value should fit in i32")
}

fn normalize_optional_str(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.is_empty())
}

fn assert_timestamp_close(actual: Option<i64>, expected: Option<i64>, field_name: &str) {
    match (actual, expected) {
        (Some(actual), Some(expected)) => assert!(
            (actual - expected).abs() <= 5,
            "{field_name} should match within 5 seconds: actual={actual}, expected={expected}"
        ),
        (None, None) => {}
        _ => panic!("{field_name} presence mismatch: actual={actual:?}, expected={expected:?}"),
    }
}

fn raw_tweet_from_content(content: &Content) -> RawTwitterTweet {
    serde_json::from_value(
        content
            .raw_data
            .clone()
            .expect("source content should retain raw_data"),
    )
    .expect("source content raw_data should deserialize into TwitterTweet")
}

fn raw_tweet_from_comment(comment: &Comment) -> RawTwitterTweet {
    serde_json::from_value(
        comment
            .raw_data
            .clone()
            .expect("source comment should retain raw_data"),
    )
    .expect("source comment raw_data should deserialize into TwitterTweet")
}

fn raw_media_urls(tweet: &RawTwitterTweet) -> Option<Vec<Option<String>>> {
    let urls = tweet.media_urls();
    if urls.is_empty() {
        None
    } else {
        Some(urls.into_iter().map(Some).collect())
    }
}

fn raw_user_description(tweet: &RawTwitterTweet) -> Option<&str> {
    tweet
        .user_info
        .as_ref()
        .and_then(|user| user.description.as_deref())
        .or_else(|| {
            tweet
                .author
                .as_ref()
                .and_then(|user| user.description.as_deref())
        })
}

fn raw_user_followers(tweet: &RawTwitterTweet) -> Option<i32> {
    tweet
        .user_info
        .as_ref()
        .and_then(|user| user.followers_count)
        .or_else(|| tweet.author.as_ref().and_then(|user| user.followers_count))
        .map(as_i32)
}

fn raw_user_avatar(tweet: &RawTwitterTweet) -> Option<&str> {
    tweet
        .user_info
        .as_ref()
        .and_then(|user| user.avatar.as_deref())
        .or_else(|| {
            tweet
                .author
                .as_ref()
                .and_then(|user| user.avatar.as_deref())
        })
}

fn raw_user_verified(tweet: &RawTwitterTweet) -> Option<bool> {
    tweet
        .user_info
        .as_ref()
        .and_then(|user| user.verified.or(user.blue_verified))
        .or_else(|| {
            tweet
                .author
                .as_ref()
                .and_then(|user| user.verified.or(user.blue_verified))
        })
}

async fn candidate_handle_names() -> Vec<String> {
    let search_seed = pick_search_seed().await;
    let comment_seed = pick_comment_seed().await;
    let client = create_client();
    let mut handles = Vec::new();

    push_unique_query(&mut handles, search_seed.screen_name.clone());
    push_unique_query(&mut handles, comment_seed.screen_name.clone());

    for query in [
        search_seed.query.clone(),
        comment_seed.query.clone(),
        "OpenAI".to_string(),
    ] {
        let response = retry_tikhub_rate_limit("twitter handle candidate search", || async {
            let params = TwitterSearchParams::new(query.clone()).with_search_type(SEARCH_TYPE);
            client.search_twitter_tweets_with_retry(&params).await
        })
        .await;

        if let Some(timeline) = response
            .data
            .as_ref()
            .and_then(|data| data.timeline.as_ref())
        {
            for tweet in timeline {
                if let Some(handle) = tweet.author_handle() {
                    push_unique_query(&mut handles, handle.to_string());
                }
            }
        }
    }

    handles
}

async fn prepare_search_case(
    adapter: &glance_mind_agent_rs::TwitterAdapter,
    strategy: &TwitterStrategy,
) -> WorkflowCaseInput {
    let search_seed = pick_search_seed().await;
    let comment_seed = pick_comment_seed().await;
    let extra_pairs = vec![("search_type".to_string(), json!(SEARCH_TYPE))];
    let mut queries = candidate_exact_queries(&search_seed.tweet);
    for query in candidate_exact_queries(&comment_seed.tweet) {
        push_unique_query(&mut queries, query);
    }
    push_unique_query(&mut queries, search_seed.query.clone());
    push_unique_query(&mut queries, comment_seed.query.clone());
    for query in SEARCH_CASE_FALLBACK_QUERIES {
        push_unique_query(&mut queries, (*query).to_string());
    }

    for query in queries {
        let Some(content) = fetch_first_content_for_keyword(
            adapter,
            strategy,
            "twitter search case first content",
            &query,
            1,
            &extra_pairs,
        )
        .await
        else {
            continue;
        };

        let comments = fetch_comments_for_content(
            adapter,
            "twitter search case comments",
            &content.content_id,
            COMMENT_TARGET_COUNT,
        )
        .await;
        if comments.len() >= COMMENT_TARGET_COUNT as usize {
            return WorkflowCaseInput {
                raw_keyword: query,
                extra_pairs,
            };
        }
    }

    panic!("twitter search workflow could not find a stable live query with enough comments");
}

async fn prepare_handle_case(
    adapter: &glance_mind_agent_rs::TwitterAdapter,
    strategy: &TwitterStrategy,
) -> WorkflowCaseInput {
    for handle in candidate_handle_names().await {
        let raw_keyword = format!("twitter_handle:{handle}");
        let Some(content) = fetch_first_content_for_keyword(
            adapter,
            strategy,
            "twitter handle case first content",
            &raw_keyword,
            1,
            &[],
        )
        .await
        else {
            continue;
        };

        let comments = fetch_comments_for_content(
            adapter,
            "twitter handle case comments",
            &content.content_id,
            COMMENT_TARGET_COUNT,
        )
        .await;
        if comments.len() >= COMMENT_TARGET_COUNT as usize {
            return WorkflowCaseInput {
                raw_keyword,
                extra_pairs: Vec::new(),
            };
        }
    }

    panic!("twitter handle workflow could not find a first-result handle with enough comments");
}

async fn prepare_tweet_id_case(
    adapter: &glance_mind_agent_rs::TwitterAdapter,
    strategy: &TwitterStrategy,
) -> WorkflowCaseInput {
    let seed = pick_comment_seed().await;
    let raw_keyword = format!("twitter_tweet_id:{}", seed.tweet_id);
    let content = fetch_first_content_for_keyword(
        adapter,
        strategy,
        "twitter detail case first content",
        &raw_keyword,
        1,
        &[],
    )
    .await
    .expect("twitter_tweet_id workflow should resolve a tweet");
    let comments = fetch_comments_for_content(
        adapter,
        "twitter detail case comments",
        &content.content_id,
        COMMENT_TARGET_COUNT,
    )
    .await;

    assert!(
        comments.len() >= COMMENT_TARGET_COUNT as usize,
        "twitter_tweet_id workflow should select a tweet with enough comments"
    );

    WorkflowCaseInput {
        raw_keyword,
        extra_pairs: Vec::new(),
    }
}

async fn prepare_rest_id_case(
    adapter: &glance_mind_agent_rs::TwitterAdapter,
    strategy: &TwitterStrategy,
) -> WorkflowCaseInput {
    let seed = pick_rest_id_seed().await;
    let rest_id = seed
        .tweet
        .user_id()
        .expect("rest_id seed should have user_id")
        .to_string();
    let raw_keyword = format!("twitter_rest_id:{rest_id}");

    let content = fetch_first_content_for_keyword(
        adapter,
        strategy,
        "twitter rest_id case first content",
        &raw_keyword,
        1,
        &[],
    )
    .await
    .expect("twitter_rest_id workflow should resolve a tweet");

    let comments = fetch_comments_for_content(
        adapter,
        "twitter rest_id case comments",
        &content.content_id,
        COMMENT_TARGET_COUNT,
    )
    .await;

    assert!(
        comments.len() >= COMMENT_TARGET_COUNT as usize,
        "twitter_rest_id workflow should select a tweet with enough comments (got {})",
        comments.len()
    );

    WorkflowCaseInput {
        raw_keyword,
        extra_pairs: Vec::new(),
    }
}

async fn run_real_workflow_case(
    adapter: glance_mind_agent_rs::TwitterAdapter,
    case: WorkflowCaseInput,
) {
    let WorkflowCaseInput {
        raw_keyword,
        extra_pairs,
    } = case;

    let Some(database_url) = database_url() else {
        eprintln!(
            "Skipping Twitter real DB workflow case - DATABASE_URL is unset or real DB tests are disabled on GitHub Actions"
        );
        return;
    };
    let mut conn = connect(&database_url);
    let (campaign_id, task_id) = create_supporting_campaign_and_task(&mut conn);

    let repo = Arc::new(
        PostgresAdapter::from_url(&database_url).expect("failed to create PostgresAdapter"),
    );
    let (gateway, recorded) = RecordingTwitterGateway::new(adapter);
    let gateway = Arc::new(gateway);
    let orchestrator = WorkflowOrchestrator::builder()
        .add_content_gateway("twitter", gateway.clone())
        .add_comment_gateway("twitter", gateway.clone())
        .ai_analyzer(Arc::new(StaticAiAnalyzer))
        .content_repository(repo)
        .prompt_repository(Arc::new(StaticPromptRepository))
        .progress_tracker(Arc::new(NoOpProgressTracker))
        .add_strategy(Arc::new(TwitterStrategy::new()))
        .config(OrchestratorConfig {
            max_videos_per_keyword: 1,
            max_comments_per_video: COMMENT_TARGET_COUNT,
            ai_batch_size: COMMENT_TARGET_COUNT as usize,
            continue_on_error: false,
        })
        .build()
        .expect("failed to build real Twitter orchestrator");

    let mut task_config = TaskConfig::new(campaign_id, "twitter")
        .with_keywords(vec![raw_keyword])
        .with_region("GLOBAL")
        .with_max_videos(1)
        .with_max_comments_per_video(COMMENT_TARGET_COUNT as i32);
    for (key, value) in &extra_pairs {
        task_config.extra.insert(key.clone(), value.clone());
    }

    let result = orchestrator
        .process_task(task_id as i64, task_config)
        .await
        .expect("twitter real workflow should succeed");

    assert!(result.success, "workflow should succeed");
    assert_eq!(result.contents_processed, 1);
    assert_eq!(result.comments_processed, COMMENT_TARGET_COUNT as i32);
    assert_eq!(result.analyses_generated, COMMENT_TARGET_COUNT as i32);

    let expected_content = recorded
        .content
        .lock()
        .await
        .clone()
        .expect("workflow should record the source content it processed");
    let expected_comments = recorded.comments.lock().await.clone();
    assert_eq!(
        expected_comments.len(),
        COMMENT_TARGET_COUNT as usize,
        "workflow should record all analyzed comments"
    );

    use schema::gm_agent_twitter_comments::dsl as comments_dsl;
    use schema::gm_agent_twitter_tweets::dsl as tweets_dsl;

    let persisted_tweet: models::TwitterTweet = tweets_dsl::gm_agent_twitter_tweets
        .filter(tweets_dsl::task_id.eq(task_id))
        .first(&mut conn)
        .expect("twitter tweet should be persisted");

    let persisted_comments: Vec<models::TwitterComment> = comments_dsl::gm_agent_twitter_comments
        .filter(comments_dsl::campaign_id.eq(campaign_id))
        .order(comments_dsl::twitter_comment_id.asc())
        .load(&mut conn)
        .expect("twitter comments should be persisted");

    let expected_raw_tweet = raw_tweet_from_content(&expected_content);
    assert_eq!(persisted_tweet.task_id, task_id);
    assert_eq!(persisted_tweet.campaign_id, Some(campaign_id));
    assert_eq!(
        persisted_tweet.twitter_tweet_id,
        expected_content.content_id
    );
    assert_eq!(
        persisted_tweet.conversation_id,
        expected_raw_tweet.conversation_id
    );
    assert_eq!(persisted_tweet.full_text, expected_content.description);
    assert_eq!(persisted_tweet.lang, expected_raw_tweet.lang);
    assert_eq!(
        normalize_optional_str(persisted_tweet.screen_name.as_deref()),
        normalize_optional_str(expected_raw_tweet.author_handle())
    );
    assert_eq!(
        normalize_optional_str(persisted_tweet.user_name.as_deref()),
        normalize_optional_str(expected_raw_tweet.author_name())
    );
    assert_eq!(
        normalize_optional_str(persisted_tweet.user_id.as_deref()),
        normalize_optional_str(expected_raw_tweet.user_id())
    );
    assert_eq!(
        normalize_optional_str(persisted_tweet.user_description.as_deref()),
        normalize_optional_str(raw_user_description(&expected_raw_tweet))
    );
    assert_eq!(
        persisted_tweet.user_followers_count,
        raw_user_followers(&expected_raw_tweet)
    );
    assert_eq!(
        normalize_optional_str(persisted_tweet.user_avatar.as_deref()),
        normalize_optional_str(raw_user_avatar(&expected_raw_tweet))
    );
    assert_eq!(
        persisted_tweet.user_verified,
        raw_user_verified(&expected_raw_tweet)
    );
    assert_eq!(
        persisted_tweet.media_urls,
        raw_media_urls(&expected_raw_tweet)
    );
    assert_eq!(
        persisted_tweet.has_media,
        Some(expected_raw_tweet.has_media())
    );
    assert_eq!(
        persisted_tweet.favorite_count,
        expected_raw_tweet
            .favorites
            .or(expected_raw_tweet.likes)
            .map(as_i32)
    );
    assert_eq!(
        persisted_tweet.retweet_count,
        expected_raw_tweet.retweets.map(as_i32)
    );
    assert_eq!(
        persisted_tweet.reply_count,
        expected_raw_tweet.replies.map(as_i32)
    );
    assert_eq!(
        persisted_tweet.quote_count,
        expected_raw_tweet.quotes.map(as_i32)
    );
    assert_eq!(
        persisted_tweet.bookmark_count,
        expected_raw_tweet.bookmarks.map(as_i32)
    );
    assert_eq!(
        persisted_tweet.view_count,
        expected_raw_tweet.views.map(as_i32)
    );
    assert_eq!(
        persisted_tweet.is_reply,
        Some(expected_raw_tweet.is_reply())
    );
    assert_eq!(
        persisted_tweet.in_reply_to_status_id,
        expected_raw_tweet.in_reply_to_status_id_str
    );
    assert_eq!(
        persisted_tweet.in_reply_to_user_id,
        expected_raw_tweet.in_reply_to_user_id_str
    );
    assert_eq!(
        persisted_tweet.created_at_str,
        expected_raw_tweet.created_at
    );
    assert_timestamp_close(
        persisted_tweet.created_at_ts,
        expected_raw_tweet.created_at_timestamp(),
        "persisted_tweet.created_at_ts",
    );
    assert_timestamp_close(
        persisted_tweet
            .tweet_created_at
            .map(|value| value.timestamp()),
        expected_raw_tweet.created_at_timestamp(),
        "persisted_tweet.tweet_created_at",
    );
    assert!(persisted_tweet.created_at.timestamp() > 0);
    assert!(
        persisted_tweet.updated_at.is_none(),
        "a fresh twitter tweet insert should not set updated_at"
    );

    assert_eq!(
        persisted_comments.len(),
        COMMENT_TARGET_COUNT as usize,
        "all analyzed Twitter comments should be persisted"
    );

    let expected_comment_lookup = expected_comments
        .iter()
        .map(|comment| (comment.comment_id.clone(), comment.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    for persisted_comment in &persisted_comments {
        let source_comment = expected_comment_lookup
            .get(&persisted_comment.twitter_comment_id)
            .expect("persisted comment should map back to the fetched source comment");
        let raw = raw_tweet_from_comment(source_comment);

        assert_eq!(persisted_comment.tweet_db_id, persisted_tweet.id);
        assert_eq!(persisted_comment.campaign_id, Some(campaign_id));
        assert_eq!(
            persisted_comment.twitter_comment_id,
            source_comment.comment_id
        );
        assert_eq!(persisted_comment.conversation_id, raw.conversation_id);
        assert_eq!(
            normalize_optional_str(persisted_comment.comment_screen_name.as_deref()),
            normalize_optional_str(raw.author_handle())
        );
        assert_eq!(
            normalize_optional_str(persisted_comment.comment_user_name.as_deref()),
            normalize_optional_str(raw.author_name())
        );
        assert_eq!(
            normalize_optional_str(persisted_comment.comment_user_id.as_deref()),
            normalize_optional_str(raw.user_id())
        );
        assert_eq!(
            persisted_comment.comment_user_followers,
            raw_user_followers(&raw)
        );
        assert_eq!(persisted_comment.comment_text, source_comment.text);
        assert_eq!(persisted_comment.reason.as_deref(), Some(TEST_REASON));
        assert_eq!(
            persisted_comment.suggested_reply.as_deref(),
            Some(TEST_REPLY)
        );
        assert_eq!(persisted_comment.suggested_dm.as_deref(), Some(TEST_DM));
        assert_eq!(
            persisted_comment.suggested_reply_post.as_deref(),
            Some(TEST_POST_REPLY)
        );
        assert_eq!(
            persisted_comment.favorite_count,
            raw.favorites.or(raw.likes).map(as_i32)
        );
        assert_eq!(persisted_comment.retweet_count, raw.retweets.map(as_i32));
        assert_eq!(persisted_comment.reply_count, raw.replies.map(as_i32));
        assert_eq!(
            persisted_comment.in_reply_to_status_id,
            raw.in_reply_to_status_id_str
        );
        assert_eq!(persisted_comment.is_reply, Some(raw.is_reply()));
        assert_eq!(persisted_comment.media_urls, raw_media_urls(&raw));
        assert_eq!(persisted_comment.has_media, Some(raw.has_media()));
        assert_eq!(persisted_comment.created_at_str, raw.created_at);
        assert_timestamp_close(
            persisted_comment.created_at_ts,
            raw.created_at_timestamp(),
            "persisted_comment.created_at_ts",
        );
        assert_timestamp_close(
            persisted_comment
                .comment_created_at
                .map(|value| value.timestamp()),
            raw.created_at_timestamp(),
            "persisted_comment.comment_created_at",
        );
        assert_eq!(persisted_comment.status, Some(0));
        assert!(persisted_comment.created_at.timestamp() > 0);
        assert!(
            persisted_comment.updated_at.is_none(),
            "a fresh twitter comment insert should not set updated_at"
        );
    }

    cleanup_supporting_rows(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_twitter_real_workflow_search_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = create_strategy();
    let case = prepare_search_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_twitter_real_workflow_handle_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = create_strategy();
    let case = prepare_handle_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_twitter_real_workflow_tweet_id_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = create_strategy();
    let case = prepare_tweet_id_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_twitter_real_workflow_rest_id_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = create_strategy();
    let case = prepare_rest_id_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}
