//! Real Facebook workflow test against a real PostgreSQL database.
//!
//! This test uses:
//! - Real Facebook RapidAPI calls
//! - Real `PostgresAdapter` persistence into `gm_agent_facebook_posts/comments`
//! - Real `WorkflowOrchestrator` processing flow
//! - Test doubles only for AI/prompt/progress dependencies
//!
//! Run with:
//! `DATABASE_URL=... FACEBOOK_RAPIDAPI_KEY=... cargo test --test facebook_real_db_test -- --nocapture`

use std::{
    future::Future,
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use diesel::prelude::*;
use diesel::sql_types::Integer;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};

use glance_mind_agent_rs::{
    db::{models, schema},
    domain::errors::{AiResult, DbResult, GatewayError, GatewayResult},
    ports::{
        ai_analyzer::AnalysisContext,
        progress_tracker::{
            CampaignStopResult, TaskInfo, TaskProgressUpdate, TaskStatus, TaskTerminalReason,
        },
        prompt_repository::{CampaignConfig, CampaignStatus, PlatformConfig},
    },
    AiAnalyzer, Comment, CommentGateway, Content, ContentGateway, FacebookAdapter,
    FacebookStrategy, OrchestratorConfig, PlatformStrategy, PostgresAdapter, ProgressTracker,
    PromptRepository, ReplySuggestion, TaskConfig, WorkflowOrchestrator,
};

const COMMENT_TARGET_COUNT: u32 = 5;
const TEST_REPLY: &str = "Thanks from facebook real db test";
const TEST_DM: &str = "DM from facebook real db test";
const TEST_POST_REPLY: &str = "Post reply from facebook real db test";
const TEST_REASON: &str = "facebook real db test reason";
const POSTS_QUERY: &str = "china travel";
const POSTS_LOCATION: &str = "beijing,china";
const PAGE_ID: &str = "100064881934421";
const PAGE_QUERY_TEXT: &str = "National Geographic Museum";
const PAGE_QUERY_WITH_LOCATION: &str = "National Geographic Museum washington,usa";
const PAGE_LOCATION: &str = "washington,usa";
const PLACE_QUERY: &str = "beijing china";
const PLACE_QUERY_WITH_LOCATION: &str = "beijing china beijing,china";
const PLACE_LOCATION: &str = "beijing,china";
const DEFAULT_HOST: &str = "facebook-scraper3.p.rapidapi.com";
const DEFAULT_BASE_URL: &str = "https://facebook-scraper3.p.rapidapi.com";

struct WorkflowCaseInput {
    raw_keyword: String,
    extra_pairs: Vec<(String, Value)>,
}

struct RapidApiConfig {
    api_key: String,
    api_host: String,
    base_url: String,
}

struct RealDbPostCandidate {
    post_id: &'static str,
}

const REAL_DB_POST_CANDIDATES: &[RealDbPostCandidate] = &[
    RealDbPostCandidate {
        post_id: "1134687898785891",
    },
    RealDbPostCandidate {
        post_id: "859693417025820",
    },
];

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

fn live_test_mutex() -> &'static Mutex<()> {
    static LIVE_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    LIVE_TEST_MUTEX.get_or_init(|| Mutex::new(()))
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
            .with_model_info("facebook-real-db-test", 42))
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
        "facebook-real-db-test"
    }
}

struct StaticPromptRepository;

#[async_trait]
impl PromptRepository for StaticPromptRepository {
    async fn get_campaign(&self, campaign_id: i32) -> DbResult<Option<CampaignConfig>> {
        Ok(Some(CampaignConfig {
            id: campaign_id,
            user_id: 1,
            name: "facebook-real-db-test".to_string(),
            platform_id: PlatformConfig::FACEBOOK,
            status: CampaignStatus::Active,
            target_audience: Some("travel fans".to_string()),
            product_prompt: Some("Travel planning and destination content".to_string()),
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
            name: "facebook".to_string(),
            display_name: "Facebook".to_string(),
            is_active: true,
        }))
    }

    async fn get_platform_by_name(&self, name: &str) -> DbResult<Option<PlatformConfig>> {
        Ok(Some(PlatformConfig {
            id: PlatformConfig::FACEBOOK,
            name: name.to_string(),
            display_name: "Facebook".to_string(),
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
            platform_id: PlatformConfig::FACEBOOK,
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
            "Skipping Facebook real DB workflow test - DATABASE_URL is unset or real DB tests are disabled on GitHub Actions"
        );
    }
    enabled
}

fn create_adapter() -> FacebookAdapter {
    let _ = dotenvy::dotenv();
    FacebookAdapter::from_env()
        .expect("FACEBOOK_RAPIDAPI_* must be set for the real Facebook workflow test")
}

fn require_rapidapi_config() -> RapidApiConfig {
    let _ = dotenvy::dotenv();
    let api_key = std::env::var("FACEBOOK_RAPIDAPI_KEY")
        .expect("FACEBOOK_RAPIDAPI_KEY must be set for the real Facebook workflow test");
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

fn with_extra_pairs(mut task_config: TaskConfig, extra_pairs: &[(String, Value)]) -> TaskConfig {
    for (key, value) in extra_pairs {
        task_config.extra.insert(key.clone(), value.clone());
    }
    task_config
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

async fn fetch_contents_for_keyword(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
    label: &str,
    raw_keyword: &str,
    max_videos: i32,
    extra_pairs: &[(String, Value)],
) -> Vec<Content> {
    let keyword = strategy.parse_keyword(raw_keyword);
    let task_config = with_extra_pairs(
        TaskConfig::new(1, "facebook")
            .with_region("US")
            .with_max_videos(max_videos)
            .with_max_comments_per_video(COMMENT_TARGET_COUNT as i32),
        extra_pairs,
    );
    let options = strategy.build_search_options(&task_config, &keyword);
    retry_gateway_rate_limit(label, || adapter.fetch_by_keyword(&keyword, &options)).await
}

async fn fetch_first_content_for_keyword(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
    label: &str,
    raw_keyword: &str,
    extra_pairs: &[(String, Value)],
) -> Option<Content> {
    fetch_contents_for_keyword(adapter, strategy, label, raw_keyword, 1, extra_pairs)
        .await
        .into_iter()
        .next()
}

async fn fetch_comments_for_content(
    adapter: &FacebookAdapter,
    label: &str,
    content_id: &str,
) -> Vec<Comment> {
    retry_gateway_rate_limit(label, || {
        adapter.fetch_all_comments(content_id, COMMENT_TARGET_COUNT)
    })
    .await
}

fn push_unique_query(queries: &mut Vec<String>, query: String) {
    let query = query.trim().to_string();
    if !query.is_empty() && !queries.iter().any(|existing| existing == &query) {
        queries.push(query);
    }
}

fn query_seed_from_text(text: &str) -> Option<String> {
    let words = text
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|word| word.len() >= 3)
        .take(6)
        .collect::<Vec<_>>();
    if words.len() >= 2 {
        Some(words.join(" "))
    } else {
        None
    }
}

async fn first_content_for_workflow_input(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
    label: &str,
    raw_keyword: &str,
    extra_pairs: &[(String, Value)],
) -> Option<Content> {
    fetch_first_content_for_keyword(adapter, strategy, label, raw_keyword, extra_pairs).await
}

async fn workflow_input_has_enough_comments(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
    label: &str,
    raw_keyword: &str,
    extra_pairs: &[(String, Value)],
) -> bool {
    let Some(content) = first_content_for_workflow_input(
        adapter,
        strategy,
        &format!("{label} first-content"),
        raw_keyword,
        extra_pairs,
    )
    .await
    else {
        return false;
    };

    if content.url.is_none() {
        return false;
    }

    let comments =
        fetch_comments_for_content(adapter, &format!("{label} comments"), &content.content_id)
            .await;

    comments.len() >= COMMENT_TARGET_COUNT as usize
}

async fn candidate_post_query_seeds(adapter: &FacebookAdapter) -> Vec<String> {
    let mut queries = Vec::new();
    for candidate in REAL_DB_POST_CANDIDATES {
        let content = retry_gateway_rate_limit("facebook fixed post seed fetch", || {
            adapter.fetch_by_id(candidate.post_id)
        })
        .await;
        let Some(content) = content else {
            continue;
        };
        if let Some(name) = content.author_name.clone() {
            push_unique_query(&mut queries, name);
        }
        if let Some(seed) = query_seed_from_text(&content.description) {
            push_unique_query(&mut queries, seed);
        }
    }
    push_unique_query(&mut queries, POSTS_QUERY.to_string());
    push_unique_query(&mut queries, "travel".to_string());
    queries
}

async fn fixed_candidate_page_names(adapter: &FacebookAdapter) -> Vec<String> {
    let mut names = Vec::new();
    for candidate in REAL_DB_POST_CANDIDATES {
        let content = retry_gateway_rate_limit("facebook fixed page-name fetch", || {
            adapter.fetch_by_id(candidate.post_id)
        })
        .await;
        let Some(content) = content else {
            continue;
        };
        if let Some(name) = content.author_name.clone() {
            push_unique_query(&mut names, name);
        }
        if let Some(raw_author_name) = content
            .raw_data
            .as_ref()
            .and_then(|raw| raw.get("author"))
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
        {
            push_unique_query(&mut names, raw_author_name.to_string());
        }
    }
    names
}

async fn fixed_candidate_page_ids(adapter: &FacebookAdapter) -> Vec<String> {
    let mut ids = Vec::new();
    for candidate in REAL_DB_POST_CANDIDATES {
        let content = retry_gateway_rate_limit("facebook fixed page-id fetch", || {
            adapter.fetch_by_id(candidate.post_id)
        })
        .await;
        let Some(content) = content else {
            continue;
        };
        if let Some(raw_author_id) = content
            .raw_data
            .as_ref()
            .and_then(|raw| raw.get("author"))
            .and_then(|author| author.get("id"))
            .and_then(Value::as_str)
        {
            push_unique_query(&mut ids, raw_author_id.to_string());
        }
    }
    ids
}

async fn discover_candidate_names(path: &str, query: &str) -> Vec<String> {
    let body = fetch_raw_json(path, &[("query", query)]).await;
    body.get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

async fn discover_candidate_page_ids(path: &str, query: &str) -> Vec<String> {
    let body = fetch_raw_json(path, &[("query", query)]).await;
    body.get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|item| item.get("facebook_id").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

async fn prepare_exact_workflow_case(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
    label: &str,
    raw_keyword: &str,
    base_extra_pairs: &[(String, Value)],
) -> Option<WorkflowCaseInput> {
    if workflow_input_has_enough_comments(adapter, strategy, label, raw_keyword, base_extra_pairs)
        .await
    {
        return Some(WorkflowCaseInput {
            raw_keyword: raw_keyword.to_string(),
            extra_pairs: base_extra_pairs.to_vec(),
        });
    }

    let contents = fetch_contents_for_keyword(
        adapter,
        strategy,
        &format!("{label} selection"),
        raw_keyword,
        10,
        base_extra_pairs,
    )
    .await;

    for content in contents {
        let Some(timestamp) = content.created_at else {
            continue;
        };
        let Some(_) = content.url.as_ref() else {
            continue;
        };
        let comments = fetch_comments_for_content(
            adapter,
            &format!("{label} content-comments"),
            &content.content_id,
        )
        .await;
        if comments.len() < COMMENT_TARGET_COUNT as usize {
            continue;
        }

        let (start_date, end_date) = surrounding_date_window(timestamp);
        let mut exact_extra_pairs = base_extra_pairs.to_vec();
        exact_extra_pairs.push(("start_date".to_string(), json!(start_date)));
        exact_extra_pairs.push(("end_date".to_string(), json!(end_date)));

        let exact_content = fetch_first_content_for_keyword(
            adapter,
            strategy,
            &format!("{label} exact"),
            raw_keyword,
            &exact_extra_pairs,
        )
        .await;

        if exact_content
            .as_ref()
            .is_some_and(|resolved| resolved.content_id == content.content_id)
        {
            return Some(WorkflowCaseInput {
                raw_keyword: raw_keyword.to_string(),
                extra_pairs: exact_extra_pairs,
            });
        }
    }

    None
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

fn comparable_asset_url(url: &str) -> &str {
    url.split('?')
        .next()
        .and_then(|without_query| without_query.rsplit('/').next())
        .unwrap_or(url)
}

fn assert_asset_url_eq(actual: Option<&str>, expected: Option<&str>, field_name: &str) {
    match (actual, expected) {
        (Some(actual), Some(expected)) => assert_eq!(
            comparable_asset_url(actual),
            comparable_asset_url(expected),
            "{field_name} should point to the same asset"
        ),
        (None, None) => {}
        _ => panic!("{field_name} presence mismatch: actual={actual:?}, expected={expected:?}"),
    }
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

async fn retry_gateway_rate_limit<T, F, Fut>(label: &str, mut operation: F) -> T
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
                    "facebook real db test hit rate limit during {label}; retry {} in {}s",
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

fn create_supporting_campaign_and_task(conn: &mut PgConnection) -> (i32, i32) {
    let user_id = query_single_id(conn, "SELECT id FROM gm_users ORDER BY id LIMIT 1");
    let region_id =
        diesel::sql_query("SELECT id FROM gm_regions WHERE platform_id = 3 ORDER BY id LIMIT 1")
            .get_result::<IdRow>(conn)
            .or_else(|_| {
                diesel::sql_query("SELECT id FROM gm_regions ORDER BY id LIMIT 1").get_result(conn)
            })
            .map(|row| row.id)
            .expect("failed to resolve a region for the Facebook test campaign");
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
            {campaign_id}, {user_id}, 'facebook real db test campaign', 'ACTIVE', 3, {region_id}, {ai_model_id},
            'Travel planning and destination content', 'IMMEDIATE', 0,
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
        "DELETE FROM gm_agent_facebook_comments WHERE campaign_id = {campaign_id}"
    ))
    .execute(conn);
    let _ = diesel::sql_query(format!(
        "DELETE FROM gm_agent_facebook_posts WHERE task_id = {task_id}"
    ))
    .execute(conn);
    let _ = diesel::sql_query(format!("DELETE FROM gm_crawler_tasks WHERE id = {task_id}"))
        .execute(conn);
    let _ = diesel::sql_query(format!("DELETE FROM gm_campaigns WHERE id = {campaign_id}"))
        .execute(conn);
}

async fn pick_post_with_comments(adapter: &FacebookAdapter) -> (Content, Vec<Comment>) {
    for candidate in REAL_DB_POST_CANDIDATES {
        let maybe_content = retry_gateway_rate_limit("facebook fixed post fetch", || {
            adapter.fetch_by_id(candidate.post_id)
        })
        .await;
        let Some(content) = maybe_content else {
            continue;
        };

        let comments = retry_gateway_rate_limit("facebook fixed post comment fetch", || {
            adapter.fetch_all_comments(candidate.post_id, COMMENT_TARGET_COUNT)
        })
        .await;

        if comments.len() >= COMMENT_TARGET_COUNT as usize && content.url.is_some() {
            assert_eq!(content.content_id, candidate.post_id);
            return (content, comments);
        }
    }

    panic!(
        "none of the fixed Facebook post candidates returned at least {COMMENT_TARGET_COUNT} comments"
    );
}

async fn prepare_posts_case(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
) -> WorkflowCaseInput {
    for query in candidate_post_query_seeds(adapter).await {
        let extra_sets = if query == POSTS_QUERY || query == "travel" {
            vec![
                vec![
                    ("search_type".to_string(), json!("posts")),
                    ("recent_posts".to_string(), json!(true)),
                    ("location".to_string(), json!(POSTS_LOCATION)),
                ],
                vec![
                    ("search_type".to_string(), json!("posts")),
                    ("recent_posts".to_string(), json!(true)),
                ],
            ]
        } else {
            vec![vec![
                ("search_type".to_string(), json!("posts")),
                ("recent_posts".to_string(), json!(true)),
            ]]
        };

        for extra_pairs in extra_sets {
            if let Some(case) = prepare_exact_workflow_case(
                adapter,
                strategy,
                "facebook posts case",
                &query,
                &extra_pairs,
            )
            .await
            {
                return case;
            }
        }
    }

    panic!("facebook posts live workflow could not find a first-result query with enough comments");
}

async fn prepare_pages_case(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
) -> WorkflowCaseInput {
    let mut queries = fixed_candidate_page_names(adapter).await;
    push_unique_query(&mut queries, PAGE_QUERY_TEXT.to_string());
    for name in discover_candidate_names("/search/pages", PAGE_QUERY_WITH_LOCATION).await {
        push_unique_query(&mut queries, name);
    }

    for query in queries {
        for extra_pairs in [
            vec![
                ("search_type".to_string(), json!("pages")),
                ("recent_posts".to_string(), json!(true)),
                ("location".to_string(), json!(PAGE_LOCATION)),
            ],
            vec![
                ("search_type".to_string(), json!("pages")),
                ("recent_posts".to_string(), json!(true)),
            ],
        ] {
            if let Some(case) = prepare_exact_workflow_case(
                adapter,
                strategy,
                "facebook pages case",
                &query,
                &extra_pairs,
            )
            .await
            {
                return case;
            }
        }
    }

    panic!("facebook pages live workflow could not find a first-result query with enough comments");
}

async fn prepare_places_case(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
) -> WorkflowCaseInput {
    let mut queries = fixed_candidate_page_names(adapter).await;
    push_unique_query(&mut queries, PLACE_QUERY.to_string());
    for name in discover_candidate_names("/search/places", PLACE_QUERY_WITH_LOCATION).await {
        push_unique_query(&mut queries, name);
    }

    for query in queries {
        for extra_pairs in [
            vec![
                ("search_type".to_string(), json!("places")),
                ("location".to_string(), json!(PLACE_LOCATION)),
            ],
            vec![("search_type".to_string(), json!("places"))],
        ] {
            if let Some(case) = prepare_exact_workflow_case(
                adapter,
                strategy,
                "facebook places case",
                &query,
                &extra_pairs,
            )
            .await
            {
                return case;
            }
        }
    }

    panic!(
        "facebook places live workflow could not find a first-result query with enough comments"
    );
}

async fn prepare_facebook_page_case(
    adapter: &FacebookAdapter,
    strategy: &FacebookStrategy,
) -> WorkflowCaseInput {
    let mut page_ids = fixed_candidate_page_ids(adapter).await;
    push_unique_query(&mut page_ids, PAGE_ID.to_string());
    for page_id in discover_candidate_page_ids("/search/pages", PAGE_QUERY_WITH_LOCATION).await {
        if !page_ids.iter().any(|existing| existing == &page_id) {
            page_ids.push(page_id);
        }
    }

    for page_id in page_ids {
        let raw_keyword = format!("facebook_page:{page_id}");
        let extra_pairs = vec![("recent_posts".to_string(), json!(true))];
        if let Some(case) = prepare_exact_workflow_case(
            adapter,
            strategy,
            "facebook page case",
            &raw_keyword,
            &extra_pairs,
        )
        .await
        {
            return case;
        }
    }

    panic!("facebook_page live workflow could not find a first-result query with enough comments");
}

async fn prepare_post_url_case(adapter: &FacebookAdapter) -> WorkflowCaseInput {
    let (selected_content, _selected_comments) = pick_post_with_comments(adapter).await;
    let selected_url = selected_content
        .url
        .clone()
        .expect("selected Facebook post should have a URL");

    WorkflowCaseInput {
        raw_keyword: format!("facebook_post_url:{selected_url}"),
        extra_pairs: Vec::new(),
    }
}

async fn run_real_workflow_case(adapter: FacebookAdapter, case: WorkflowCaseInput) {
    let WorkflowCaseInput {
        raw_keyword,
        extra_pairs,
    } = case;
    let workflow_label = format!("raw_keyword={raw_keyword:?}, extra_pairs={extra_pairs:?}");
    let Some(database_url) = database_url() else {
        eprintln!(
            "Skipping Facebook real DB workflow case - DATABASE_URL is unset or real DB tests are disabled on GitHub Actions"
        );
        return;
    };
    let mut conn = connect(&database_url);
    let (campaign_id, task_id) = create_supporting_campaign_and_task(&mut conn);

    let repo = Arc::new(
        PostgresAdapter::from_url(&database_url).expect("failed to create PostgresAdapter"),
    );
    let adapter = Arc::new(adapter);
    let orchestrator = WorkflowOrchestrator::builder()
        .add_content_gateway("facebook", adapter.clone())
        .add_comment_gateway("facebook", adapter.clone())
        .ai_analyzer(Arc::new(StaticAiAnalyzer))
        .content_repository(repo)
        .prompt_repository(Arc::new(StaticPromptRepository))
        .progress_tracker(Arc::new(NoOpProgressTracker))
        .add_strategy(Arc::new(FacebookStrategy::new()))
        .config(OrchestratorConfig {
            max_videos_per_keyword: 1,
            max_comments_per_video: COMMENT_TARGET_COUNT,
            ai_batch_size: COMMENT_TARGET_COUNT as usize,
            continue_on_error: false,
        })
        .build()
        .expect("failed to build real Facebook orchestrator");

    let task_config = with_extra_pairs(
        TaskConfig::new(campaign_id, "facebook")
            .with_keywords(vec![raw_keyword.clone()])
            .with_region("US")
            .with_max_videos(1)
            .with_max_comments_per_video(COMMENT_TARGET_COUNT as i32),
        &extra_pairs,
    );

    let result = orchestrator
        .process_task(task_id as i64, task_config)
        .await
        .expect("facebook real workflow should succeed");

    assert!(result.success, "workflow should succeed");
    assert_eq!(result.contents_processed, 1);
    assert_eq!(
        result.comments_processed, COMMENT_TARGET_COUNT as i32,
        "{workflow_label}"
    );
    assert_eq!(
        result.analyses_generated, COMMENT_TARGET_COUNT as i32,
        "{workflow_label}"
    );

    use schema::gm_agent_facebook_comments::dsl as comments_dsl;
    use schema::gm_agent_facebook_posts::dsl as posts_dsl;

    let persisted_post: models::FacebookPost = posts_dsl::gm_agent_facebook_posts
        .filter(posts_dsl::task_id.eq(task_id))
        .first(&mut conn)
        .expect("facebook post should be persisted");

    let persisted_comments: Vec<models::FacebookComment> = comments_dsl::gm_agent_facebook_comments
        .filter(comments_dsl::campaign_id.eq(campaign_id))
        .order(comments_dsl::facebook_comment_id.asc())
        .load(&mut conn)
        .expect("facebook comments should be persisted");

    let strategy = FacebookStrategy::new();
    let expected_persisted_content = first_content_for_workflow_input(
        adapter.as_ref(),
        &strategy,
        "facebook workflow source content for DB assertions",
        &raw_keyword,
        &extra_pairs,
    )
    .await
    .expect("facebook workflow input should still resolve to a post for DB assertions");
    let expected_comments =
        retry_gateway_rate_limit("facebook direct comment fetch for DB assertions", || {
            adapter.fetch_all_comments(&persisted_post.facebook_post_id, COMMENT_TARGET_COUNT)
        })
        .await;
    let comment_lookup = expected_comments
        .iter()
        .map(|comment| (comment.comment_id.clone(), comment.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let post_raw = expected_persisted_content
        .raw_data
        .as_ref()
        .expect("expected persisted content should retain raw_data");

    assert_eq!(persisted_post.task_id, task_id);
    assert_eq!(persisted_post.campaign_id, Some(campaign_id));
    assert_eq!(
        persisted_post.facebook_post_id,
        expected_persisted_content.content_id
    );
    assert_eq!(
        persisted_post.post_type.as_deref(),
        post_raw.get("type").and_then(|value| value.as_str())
    );
    assert_eq!(persisted_post.url, expected_persisted_content.url);
    assert_eq!(
        normalize_optional_str(persisted_post.message.as_deref()),
        normalize_optional_str(post_raw.get("message").and_then(|value| value.as_str()))
    );
    assert_eq!(
        normalize_optional_str(persisted_post.message_rich.as_deref()),
        normalize_optional_str(
            post_raw
                .get("message_rich")
                .and_then(|value| value.as_str())
        )
    );
    assert_timestamp_close(
        persisted_post.timestamp,
        post_raw.get("timestamp").and_then(|value| value.as_i64()),
        "persisted_post.timestamp",
    );
    assert_timestamp_close(
        persisted_post.posted_at.map(|value| value.timestamp()),
        persisted_post.timestamp,
        "persisted_post.posted_at",
    );
    assert_eq!(
        persisted_post.reactions_count,
        Some(expected_persisted_content.engagement.likes as i32)
    );
    assert_eq!(
        persisted_post.comments_count,
        Some(expected_persisted_content.engagement.comments as i32)
    );
    assert_eq!(
        persisted_post.reshare_count,
        Some(expected_persisted_content.engagement.shares as i32)
    );
    assert_eq!(
        persisted_post.reactions_like,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("like"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.reactions_love,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("love"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.reactions_haha,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("haha"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.reactions_wow,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("wow"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.reactions_sad,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("sad"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.reactions_angry,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("angry"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.reactions_care,
        post_raw
            .get("reactions")
            .and_then(|value| value.get("care"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.author_id.as_deref(),
        post_raw
            .get("author")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.author_name.as_deref(),
        post_raw
            .get("author")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.author_url.as_deref(),
        post_raw
            .get("author")
            .and_then(|value| value.get("url"))
            .and_then(|value| value.as_str())
    );
    assert_asset_url_eq(
        persisted_post.author_profile_picture_url.as_deref(),
        post_raw
            .get("author")
            .and_then(|value| value.get("profile_picture_url"))
            .and_then(|value| value.as_str()),
        "persisted_post.author_profile_picture_url",
    );
    assert_eq!(
        persisted_post.author_title.as_deref(),
        post_raw
            .get("author_title")
            .and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.has_image,
        Some(
            post_raw.get("image").is_some()
                && !post_raw.get("image").is_some_and(|value| value.is_null())
        )
    );
    assert_asset_url_eq(
        persisted_post.image_url.as_deref(),
        post_raw
            .get("image")
            .and_then(|value| value.get("uri"))
            .and_then(|value| value.as_str()),
        "persisted_post.image_url",
    );
    assert_eq!(
        persisted_post.image_width,
        post_raw
            .get("image")
            .and_then(|value| value.get("width"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.image_height,
        post_raw
            .get("image")
            .and_then(|value| value.get("height"))
            .and_then(|value| value.as_i64())
            .map(|value| value as i32)
    );
    assert_eq!(
        persisted_post.image_id.as_deref(),
        post_raw
            .get("image")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.has_video,
        Some(
            post_raw.get("video").is_some()
                && !post_raw.get("video").is_some_and(|value| value.is_null())
        )
    );
    assert_asset_url_eq(
        persisted_post.video_thumbnail.as_deref(),
        post_raw
            .get("video_thumbnail")
            .and_then(|value| value.as_str()),
        "persisted_post.video_thumbnail",
    );
    assert_eq!(
        persisted_post.external_url.as_deref(),
        post_raw
            .get("external_url")
            .and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.attached_post_url.as_deref(),
        post_raw
            .get("attached_post_url")
            .and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.comments_id.as_deref(),
        post_raw.get("comments_id").and_then(|value| value.as_str())
    );
    assert_eq!(
        persisted_post.shares_id.as_deref(),
        post_raw.get("shares_id").and_then(|value| value.as_str())
    );
    assert!(persisted_post.created_at.timestamp() > 0);
    assert!(
        persisted_post.updated_at.is_none(),
        "a fresh insert should not set updated_at"
    );

    assert_eq!(
        persisted_comments.len(),
        COMMENT_TARGET_COUNT as usize,
        "all analyzed Facebook comments should be persisted"
    );

    for persisted_comment in &persisted_comments {
        let source_comment = comment_lookup
            .get(&persisted_comment.facebook_comment_id)
            .expect("persisted comment should map back to the fetched source comment");
        let raw = source_comment
            .raw_data
            .as_ref()
            .expect("source comment should retain raw_data");

        assert_eq!(persisted_comment.post_db_id, persisted_post.id);
        assert_eq!(persisted_comment.campaign_id, Some(campaign_id));
        assert_eq!(
            persisted_comment.facebook_comment_id,
            source_comment.comment_id
        );
        assert_eq!(
            persisted_comment.parent_comment_id.as_deref(),
            raw.get("parent_comment_id")
                .and_then(|value| value.as_str())
        );
        assert_eq!(
            persisted_comment.comment_url.as_deref(),
            raw.get("comment_url").and_then(|value| value.as_str())
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
            persisted_comment.comment_user_id.as_deref(),
            source_comment
                .author_uid
                .as_deref()
                .or(Some(source_comment.author.as_str()))
        );
        assert_eq!(
            persisted_comment.comment_username.as_deref(),
            source_comment
                .author_name
                .as_deref()
                .or(Some(source_comment.author.as_str()))
        );
        assert_eq!(
            persisted_comment.comment_user_url.as_deref(),
            raw.get("author")
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str())
        );
        assert_asset_url_eq(
            persisted_comment.comment_user_profile_picture.as_deref(),
            raw.get("author")
                .and_then(|value| value.get("profile_image"))
                .and_then(|value| value.as_str()),
            "persisted_comment.comment_user_profile_picture",
        );
        assert_eq!(
            persisted_comment.like_count,
            Some(source_comment.likes as i32)
        );
        assert_eq!(
            persisted_comment.reply_count,
            Some(source_comment.reply_count)
        );
        assert_eq!(
            persisted_comment.threading_depth,
            raw.get("depth")
                .and_then(|value| value.as_i64())
                .map(|value| value as i32)
        );
        assert_timestamp_close(
            persisted_comment.created_at_ts,
            source_comment.created_at,
            "persisted_comment.created_at_ts",
        );
        assert_timestamp_close(
            persisted_comment
                .comment_created_at
                .map(|value| value.timestamp()),
            source_comment.created_at,
            "persisted_comment.comment_created_at",
        );
        assert_eq!(
            persisted_comment.facebook_post_id.as_deref(),
            Some(expected_persisted_content.content_id.as_str())
        );
        assert_eq!(
            persisted_comment.post_url.as_deref(),
            expected_persisted_content.url.as_deref()
        );
        assert_eq!(persisted_comment.status, Some(0));
        assert!(persisted_comment.created_at.timestamp() > 0);
        assert!(
            persisted_comment.updated_at.is_none(),
            "a fresh insert should not set updated_at"
        );
    }

    cleanup_supporting_rows(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_facebook_real_workflow_posts_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let case = prepare_posts_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_facebook_real_workflow_pages_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let case = prepare_pages_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_facebook_real_workflow_places_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let case = prepare_places_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_facebook_real_workflow_page_keyword_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let strategy = FacebookStrategy::new();
    let case = prepare_facebook_page_case(&adapter, &strategy).await;
    run_real_workflow_case(adapter, case).await;
}

#[tokio::test]
async fn test_facebook_real_workflow_post_url_persists_all_fields() {
    if !real_db_tests_enabled() {
        return;
    }
    let _guard = live_test_mutex().lock().await;
    let adapter = create_adapter();
    let case = prepare_post_url_case(&adapter).await;
    run_real_workflow_case(adapter, case).await;
}
