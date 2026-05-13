//! Focused Postgres Twitter tests for edge cases.
//!
//! These tests validate:
//! - `save_twitter_tweet` and `save_twitter_comment` with missing/malformed `raw_data`
//! - Insert vs upsert (ON CONFLICT) behavior
//! - Comment suggestion update on re-save
//!
//! Run with:
//! `DATABASE_URL=... cargo test --test twitter_postgres_test -- --nocapture --test-threads=1`

use diesel::prelude::*;
use diesel::sql_types::{Integer, Nullable, Text};
use serde_json::json;

use glance_mind_agent_rs::{
    db::{models, schema},
    ports::progress_tracker::{ProgressTracker, TaskTerminalReason},
    Comment, Content, ContentRepository, Engagement, PostgresAdapter, ReplySuggestion,
};

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

#[derive(QueryableByName)]
struct TaskTerminalReasonRow {
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = Nullable<Text>)]
    terminal_reason: Option<String>,
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

fn postgres_tests_enabled() -> bool {
    let enabled = database_url().is_some();
    if !enabled {
        eprintln!(
            "Skipping Twitter Postgres tests - DATABASE_URL is unset or real DB tests are disabled on GitHub Actions"
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

fn create_test_campaign_and_task(conn: &mut PgConnection) -> (i32, i32) {
    let user_id = query_single_id(conn, "SELECT id FROM gm_users ORDER BY id LIMIT 1");
    let region_id =
        diesel::sql_query("SELECT id FROM gm_regions WHERE platform_id = 5 ORDER BY id LIMIT 1")
            .get_result::<IdRow>(conn)
            .or_else(|_| {
                diesel::sql_query("SELECT id FROM gm_regions ORDER BY id LIMIT 1").get_result(conn)
            })
            .map(|row| row.id)
            .expect("failed to resolve a region");
    let ai_model_id = query_single_id(conn, "SELECT id FROM gm_ai_models ORDER BY id LIMIT 1");
    let campaign_id = query_single_id(
        conn,
        "SELECT COALESCE(MAX(id), 0) + 2000 AS id FROM gm_campaigns",
    );
    let task_id = query_single_id(
        conn,
        "SELECT COALESCE(MAX(id), 0) + 2000 AS id FROM gm_crawler_tasks",
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
            {campaign_id}, {user_id}, 'twitter postgres test', 'ACTIVE', 5, {region_id}, {ai_model_id},
            'test', 'IMMEDIATE', 0,
            false, false, false,
            1.00, 0.00, false,
            true, true, NOW()
        )
        "#
    ))
    .execute(conn)
    .expect("failed to insert test campaign");

    diesel::sql_query(format!(
        r#"
        INSERT INTO gm_crawler_tasks (
            id, campaign_id, max_count, process_count, status,
            search_offset, search_limit, reserved_amount, actual_consumption, created_at
        ) VALUES (
            {task_id}, {campaign_id}, 1, 0, 'pending',
            0, 1, 1.00, 0.00, NOW()
        )
        "#
    ))
    .execute(conn)
    .expect("failed to insert test task");

    (campaign_id, task_id)
}

fn cleanup(conn: &mut PgConnection, campaign_id: i32, task_id: i32) {
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

fn make_content(content_id: &str, raw_data: Option<serde_json::Value>) -> Content {
    Content {
        platform: "twitter".to_string(),
        content_id: content_id.to_string(),
        author: "testhandle".to_string(),
        author_name: Some("Test User".to_string()),
        description: "hello world".to_string(),
        url: Some(format!(
            "https://twitter.com/testhandle/status/{content_id}"
        )),
        engagement: Engagement {
            likes: 10,
            comments: 3,
            shares: 2,
            views: 100,
        },
        created_at: Some(1709640645),
        raw_data,
    }
}

fn make_comment(
    comment_id: &str,
    content_id: &str,
    raw_data: Option<serde_json::Value>,
) -> Comment {
    Comment {
        platform: "twitter".to_string(),
        comment_id: comment_id.to_string(),
        content_id: content_id.to_string(),
        parent_id: Some(content_id.to_string()),
        author: "replier".to_string(),
        author_name: Some("Replier Name".to_string()),
        author_uid: Some("uid-replier".to_string()),
        text: "nice tweet!".to_string(),
        likes: 5,
        reply_count: 1,
        created_at: Some(1709640700),
        language: Some("en".to_string()),
        is_reply: true,
        raw_data,
    }
}

fn make_suggestion(comment_id: &str) -> ReplySuggestion {
    ReplySuggestion::new(comment_id)
        .with_reply("thanks!")
        .with_dm("dm message")
        .with_post_reply("post reply")
        .with_reason("test reason")
        .with_model_info("test-model", 1)
}

fn full_tweet_raw(tweet_id: &str) -> serde_json::Value {
    json!({
        "tweet_id": tweet_id,
        "type": "tweet",
        "text": "hello world",
        "created_at": "Tue Mar 05 12:30:45 +0000 2024",
        "conversation_id": tweet_id,
        "lang": "en",
        "favorites": 10,
        "retweets": 2,
        "replies": 3,
        "quotes": 1,
        "bookmarks": 4,
        "views": 100,
        "user_info": {
            "rest_id": "uid-test",
            "screen_name": "testhandle",
            "name": "Test User",
            "description": "a test account",
            "followers_count": 500,
            "avatar": "https://example.com/avatar.jpg",
            "verified": false,
            "blue_verified": true
        },
        "media": {
            "photo": [{"media_url_https": "https://example.com/photo.jpg", "id": "m1"}]
        }
    })
}

fn full_reply_raw(reply_id: &str, parent_id: &str) -> serde_json::Value {
    json!({
        "tweet_id": reply_id,
        "type": "tweet",
        "text": "nice tweet!",
        "created_at": "Tue Mar 05 12:31:00 +0000 2024",
        "conversation_id": parent_id,
        "in_reply_to_status_id_str": parent_id,
        "in_reply_to_user_id_str": "uid-test",
        "favorites": 5,
        "retweets": 0,
        "replies": 1,
        "author": {
            "rest_id": "uid-replier",
            "screen_name": "replier",
            "name": "Replier Name",
            "followers_count": 200
        }
    })
}

// ================================================================
// Tests
// ================================================================

#[tokio::test]
async fn test_complete_task_persists_terminal_reason() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();
    let terminal_reason = TaskTerminalReason::completed();

    repo.complete_task(task_id as i64, &terminal_reason)
        .await
        .expect("complete_task should persist terminal_reason");

    let row = diesel::sql_query(format!(
        "SELECT status, terminal_reason FROM gm_crawler_tasks WHERE id = {task_id}"
    ))
    .get_result::<TaskTerminalReasonRow>(&mut conn)
    .expect("task should exist after completion");

    assert_eq!(row.status, "completed");
    assert_eq!(
        row.terminal_reason,
        Some(terminal_reason.as_terminal_message())
    );

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_tweet_with_full_raw_data() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-full-{task_id}");
    let content = make_content(&tweet_id, Some(full_tweet_raw(&tweet_id)));

    let result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .expect("save_content with full raw_data should succeed");
    assert!(result.is_new, "first insert should be new");

    use schema::gm_agent_twitter_tweets::dsl;
    let persisted: models::TwitterTweet = dsl::gm_agent_twitter_tweets
        .filter(dsl::task_id.eq(task_id))
        .filter(dsl::twitter_tweet_id.eq(&tweet_id))
        .first(&mut conn)
        .expect("tweet should be persisted");

    assert_eq!(persisted.twitter_tweet_id, tweet_id);
    assert_eq!(persisted.full_text, "hello world");
    assert_eq!(persisted.lang, Some("en".to_string()));
    assert_eq!(persisted.screen_name, Some("testhandle".to_string()));
    assert_eq!(persisted.user_name, Some("Test User".to_string()));
    assert_eq!(persisted.user_id, Some("uid-test".to_string()));
    assert_eq!(
        persisted.user_description,
        Some("a test account".to_string())
    );
    assert_eq!(persisted.user_followers_count, Some(500));
    assert_eq!(
        persisted.user_avatar,
        Some("https://example.com/avatar.jpg".to_string())
    );
    assert_eq!(persisted.user_verified, Some(false));
    assert_eq!(persisted.favorite_count, Some(10));
    assert_eq!(persisted.retweet_count, Some(2));
    assert_eq!(persisted.reply_count, Some(3));
    assert_eq!(persisted.quote_count, Some(1));
    assert_eq!(persisted.bookmark_count, Some(4));
    assert_eq!(persisted.view_count, Some(100));
    assert_eq!(persisted.has_media, Some(true));
    assert!(persisted.media_urls.is_some());
    assert_eq!(persisted.is_reply, Some(false));
    assert!(persisted.updated_at.is_none());

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_tweet_with_null_raw_data_uses_fallbacks() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-null-{task_id}");
    let content = make_content(&tweet_id, None);

    let result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .expect("save_content with null raw_data should succeed");
    assert!(result.is_new);

    use schema::gm_agent_twitter_tweets::dsl;
    let persisted: models::TwitterTweet = dsl::gm_agent_twitter_tweets
        .filter(dsl::task_id.eq(task_id))
        .filter(dsl::twitter_tweet_id.eq(&tweet_id))
        .first(&mut conn)
        .expect("tweet should be persisted even with null raw_data");

    assert_eq!(persisted.full_text, "hello world");
    assert_eq!(persisted.screen_name, Some("testhandle".to_string()));
    assert_eq!(persisted.user_name, Some("Test User".to_string()));
    assert_eq!(persisted.favorite_count, Some(10));
    assert_eq!(persisted.retweet_count, Some(2));
    assert_eq!(persisted.reply_count, Some(3));
    assert_eq!(persisted.view_count, Some(100));
    assert_eq!(persisted.lang, None);
    assert_eq!(persisted.user_id, None);

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_tweet_with_malformed_raw_data_uses_fallbacks() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-bad-{task_id}");
    let content = make_content(&tweet_id, Some(json!("not-a-json-object")));

    let result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .expect("save_content with malformed raw_data should succeed");
    assert!(result.is_new);

    use schema::gm_agent_twitter_tweets::dsl;
    let persisted: models::TwitterTweet = dsl::gm_agent_twitter_tweets
        .filter(dsl::task_id.eq(task_id))
        .filter(dsl::twitter_tweet_id.eq(&tweet_id))
        .first(&mut conn)
        .expect("tweet should be persisted with fallback values");

    assert_eq!(persisted.full_text, "hello world");
    assert_eq!(persisted.screen_name, Some("testhandle".to_string()));
    assert_eq!(persisted.user_name, Some("Test User".to_string()));
    assert_eq!(persisted.favorite_count, Some(10));
    assert_eq!(persisted.view_count, Some(100));
    assert_eq!(persisted.lang, None);
    assert_eq!(persisted.user_id, None);

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_tweet_upsert_updates_and_sets_updated_at() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-upsert-{task_id}");
    let content_v1 = make_content(&tweet_id, Some(full_tweet_raw(&tweet_id)));

    let result_v1 = repo
        .save_content(&content_v1, Some(campaign_id), Some(task_id))
        .await
        .unwrap();
    assert!(result_v1.is_new, "first insert should be new");

    let mut content_v2 = make_content(&tweet_id, Some(full_tweet_raw(&tweet_id)));
    content_v2.description = "updated text".to_string();
    content_v2.engagement.likes = 999;

    let result_v2 = repo
        .save_content(&content_v2, Some(campaign_id), Some(task_id))
        .await
        .unwrap();
    assert!(
        !result_v2.is_new,
        "second save should be an update, not a new insert"
    );
    assert_eq!(
        result_v2.id, result_v1.id,
        "upsert should return the same DB id"
    );

    use schema::gm_agent_twitter_tweets::dsl;
    let persisted: models::TwitterTweet = dsl::gm_agent_twitter_tweets
        .find(result_v1.id)
        .first(&mut conn)
        .expect("tweet should exist after upsert");

    assert_eq!(persisted.full_text, "updated text");
    assert!(
        persisted.updated_at.is_some(),
        "upsert should set updated_at"
    );

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_comment_with_full_raw_data() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-cmt-parent-{task_id}");
    let content = make_content(&tweet_id, Some(full_tweet_raw(&tweet_id)));
    let content_result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .unwrap();

    let reply_id = format!("pg-test-reply-{task_id}");
    let comment = make_comment(
        &reply_id,
        &tweet_id,
        Some(full_reply_raw(&reply_id, &tweet_id)),
    );
    let suggestion = make_suggestion(&reply_id);

    let comment_db_id = repo
        .save_comment_with_analysis(&comment, content_result.id, campaign_id, &suggestion)
        .await
        .expect("save_comment_with_analysis should succeed");
    assert!(comment_db_id > 0);

    use schema::gm_agent_twitter_comments::dsl;
    let persisted: models::TwitterComment = dsl::gm_agent_twitter_comments
        .find(comment_db_id)
        .first(&mut conn)
        .expect("comment should be persisted");

    assert_eq!(persisted.twitter_comment_id, reply_id);
    assert_eq!(persisted.tweet_db_id, content_result.id);
    assert_eq!(persisted.campaign_id, Some(campaign_id));
    assert_eq!(persisted.comment_text, "nice tweet!");
    assert_eq!(persisted.comment_screen_name, Some("replier".to_string()));
    assert_eq!(
        persisted.comment_user_name,
        Some("Replier Name".to_string())
    );
    assert_eq!(persisted.comment_user_id, Some("uid-replier".to_string()));
    assert_eq!(persisted.comment_user_followers, Some(200));
    assert_eq!(persisted.favorite_count, Some(5));
    assert_eq!(persisted.reply_count, Some(1));
    assert_eq!(persisted.in_reply_to_status_id, Some(tweet_id.clone()));
    assert_eq!(persisted.is_reply, Some(true));
    assert_eq!(persisted.reason, Some("test reason".to_string()));
    assert_eq!(persisted.suggested_reply, Some("thanks!".to_string()));
    assert_eq!(persisted.suggested_dm, Some("dm message".to_string()));
    assert_eq!(
        persisted.suggested_reply_post,
        Some("post reply".to_string())
    );
    assert_eq!(persisted.status, Some(0));
    assert!(persisted.updated_at.is_none());

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_comment_upsert_updates_suggestion() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-cmt-up-parent-{task_id}");
    let content = make_content(&tweet_id, Some(full_tweet_raw(&tweet_id)));
    let content_result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .unwrap();

    let reply_id = format!("pg-test-reply-up-{task_id}");
    let comment = make_comment(
        &reply_id,
        &tweet_id,
        Some(full_reply_raw(&reply_id, &tweet_id)),
    );
    let suggestion_v1 = make_suggestion(&reply_id);

    let id_v1 = repo
        .save_comment_with_analysis(&comment, content_result.id, campaign_id, &suggestion_v1)
        .await
        .unwrap();

    let suggestion_v2 = ReplySuggestion::new(&reply_id)
        .with_reply("updated reply")
        .with_dm("updated dm")
        .with_post_reply("updated post reply")
        .with_reason("updated reason")
        .with_model_info("test-model-v2", 2);

    let id_v2 = repo
        .save_comment_with_analysis(&comment, content_result.id, campaign_id, &suggestion_v2)
        .await
        .unwrap();

    assert_eq!(id_v1, id_v2, "upsert should return the same comment DB id");

    use schema::gm_agent_twitter_comments::dsl;
    let persisted: models::TwitterComment = dsl::gm_agent_twitter_comments
        .find(id_v1)
        .first(&mut conn)
        .expect("comment should exist after upsert");

    assert_eq!(persisted.reason, Some("updated reason".to_string()));
    assert_eq!(persisted.suggested_reply, Some("updated reply".to_string()));
    assert_eq!(persisted.suggested_dm, Some("updated dm".to_string()));
    assert_eq!(
        persisted.suggested_reply_post,
        Some("updated post reply".to_string())
    );
    assert!(
        persisted.updated_at.is_some(),
        "comment upsert should set updated_at"
    );

    cleanup(&mut conn, campaign_id, task_id);
}

#[tokio::test]
async fn test_save_comment_with_null_raw_data_uses_fallbacks() {
    if !postgres_tests_enabled() {
        return;
    }
    let db_url = database_url().expect("Postgres Twitter tests require DATABASE_URL");
    let mut conn = connect(&db_url);
    let (campaign_id, task_id) = create_test_campaign_and_task(&mut conn);
    let repo = PostgresAdapter::from_url(&db_url).unwrap();

    let tweet_id = format!("pg-test-cmt-null-parent-{task_id}");
    let content = make_content(&tweet_id, Some(full_tweet_raw(&tweet_id)));
    let content_result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .unwrap();

    let reply_id = format!("pg-test-reply-null-{task_id}");
    let comment = make_comment(&reply_id, &tweet_id, None);
    let suggestion = make_suggestion(&reply_id);

    let comment_db_id = repo
        .save_comment_with_analysis(&comment, content_result.id, campaign_id, &suggestion)
        .await
        .expect("save comment with null raw_data should succeed");

    use schema::gm_agent_twitter_comments::dsl;
    let persisted: models::TwitterComment = dsl::gm_agent_twitter_comments
        .find(comment_db_id)
        .first(&mut conn)
        .expect("comment should be persisted even with null raw_data");

    assert_eq!(persisted.comment_text, "nice tweet!");
    assert_eq!(persisted.comment_screen_name, Some("replier".to_string()));
    assert_eq!(
        persisted.comment_user_name,
        Some("Replier Name".to_string())
    );
    assert_eq!(persisted.comment_user_id, Some("uid-replier".to_string()));
    assert_eq!(persisted.favorite_count, Some(5));
    assert_eq!(persisted.reply_count, Some(1));

    cleanup(&mut conn, campaign_id, task_id);
}
