//! Real Instagram V3 persistence test against a real PostgreSQL database.
//!
//! This verifies the live TikHub V3 shape, adapter conversion, and
//! `PostgresAdapter` persistence into `gm_agent_instagram_posts/comments`.
//!
//! Run with:
//! `DATABASE_URL=... TIKHUB_API_KEY=... cargo test --test instagram_real_db_test -- --nocapture`

use diesel::prelude::*;
use diesel::sql_types::{Integer, Nullable, Text};

use glance_mind_agent_rs::{
    ContentGateway, ContentRepository, InstagramAdapter, KeywordType, PostgresAdapter,
    ReplySuggestion, SearchOptions,
};

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

#[derive(QueryableByName)]
struct InstagramPostRow {
    #[diesel(sql_type = Text)]
    code: String,
    #[diesel(sql_type = Nullable<Text>)]
    instagram_id: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    media_type: Option<i32>,
    #[diesel(sql_type = Nullable<Text>)]
    product_type: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    caption_text: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    owner_username: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    owner_id: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    owner_full_name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    media_url: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    thumbnail_url: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    like_count: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    comment_count: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    play_count: Option<i32>,
    #[diesel(sql_type = Nullable<diesel::sql_types::BigInt>)]
    taken_at_ts: Option<i64>,
}

#[derive(QueryableByName)]
struct InstagramCommentRow {
    #[diesel(sql_type = Text)]
    instagram_comment_id: String,
    #[diesel(sql_type = Nullable<Text>)]
    comment_text: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    comment_user_id: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    comment_username: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    comment_user_full_name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    reason: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    suggested_reply: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    suggested_dm: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    suggested_reply_post: Option<String>,
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
        diesel::sql_query("SELECT id FROM gm_regions WHERE platform_id = 4 ORDER BY id LIMIT 1")
            .get_result::<IdRow>(conn)
            .or_else(|_| {
                diesel::sql_query("SELECT id FROM gm_regions ORDER BY id LIMIT 1").get_result(conn)
            })
            .map(|row| row.id)
            .expect("failed to resolve a region for the Instagram test campaign");
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
            {campaign_id}, {user_id}, 'instagram real db test campaign', 'ACTIVE', 4, {region_id}, {ai_model_id},
            'Instagram V3 persistence validation', 'IMMEDIATE', 0,
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
        "DELETE FROM gm_agent_instagram_comments WHERE campaign_id = {campaign_id}"
    ))
    .execute(conn);
    let _ = diesel::sql_query(format!(
        "DELETE FROM gm_agent_instagram_posts WHERE task_id = {task_id}"
    ))
    .execute(conn);
    let _ = diesel::sql_query(format!("DELETE FROM gm_crawler_tasks WHERE id = {task_id}"))
        .execute(conn);
    let _ = diesel::sql_query(format!("DELETE FROM gm_campaigns WHERE id = {campaign_id}"))
        .execute(conn);
}

#[tokio::test]
async fn test_instagram_v3_fetch_parse_and_save_real_db() {
    let Some(database_url) = database_url() else {
        eprintln!(
            "Skipping Instagram real DB test - DATABASE_URL is unset or real DB tests are disabled on GitHub Actions"
        );
        return;
    };

    let adapter = InstagramAdapter::from_env().expect("TIKHUB_API_KEY must be configured");
    let contents = adapter
        .fetch_by_keyword(
            &KeywordType::Hashtag("muhameds".to_string()),
            &SearchOptions::new("muhameds")
                .with_platform("instagram")
                .with_count(1),
        )
        .await
        .expect("Instagram V3 fetch should succeed");
    let content = contents
        .first()
        .cloned()
        .expect("Instagram V3 fetch should return at least one post");
    let raw = content
        .raw_data
        .as_ref()
        .expect("adapter should retain raw Instagram V3 data");

    let expected_instagram_id = raw
        .get("pk")
        .or_else(|| raw.get("id"))
        .and_then(|value| value.as_str())
        .expect("raw V3 media should include pk or id")
        .to_string();
    let expected_owner_id = raw
        .get("user")
        .and_then(|user| user.get("pk").or_else(|| user.get("id")))
        .and_then(|value| value.as_str())
        .expect("raw V3 user should include pk or id")
        .to_string();
    let expected_thumbnail = raw
        .get("image_versions2")
        .and_then(|value| value.get("candidates"))
        .and_then(|value| value.as_array())
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("url"))
        .and_then(|value| value.as_str())
        .expect("raw V3 media should include image_versions2 thumbnail")
        .to_string();

    let mut conn = connect(&database_url);
    let (campaign_id, task_id) = create_supporting_campaign_and_task(&mut conn);

    let repo = PostgresAdapter::from_url(&database_url).expect("failed to create PostgresAdapter");
    let save_result = repo
        .save_content(&content, Some(campaign_id), Some(task_id))
        .await
        .expect("Instagram content should save to gm_agent_instagram_posts");

    let persisted_post = diesel::sql_query(format!(
        r#"
        SELECT code, instagram_id, media_type, product_type, caption_text,
               owner_username, owner_id, owner_full_name, media_url, thumbnail_url,
               like_count, comment_count, play_count, taken_at_ts
        FROM gm_agent_instagram_posts
        WHERE id = {}
        "#,
        save_result.id
    ))
    .get_result::<InstagramPostRow>(&mut conn)
    .expect("saved Instagram post should be queryable");

    assert_eq!(persisted_post.code, content.content_id);
    assert_eq!(
        persisted_post.instagram_id.as_deref(),
        Some(expected_instagram_id.as_str())
    );
    assert_eq!(
        persisted_post.media_type,
        raw.get("media_type")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
    );
    assert_eq!(
        persisted_post.product_type.as_deref(),
        raw.get("product_type").and_then(|v| v.as_str())
    );
    assert_eq!(
        persisted_post.caption_text.as_deref(),
        Some(content.description.as_str())
    );
    assert_eq!(
        persisted_post.owner_username.as_deref(),
        Some(content.author.as_str())
    );
    assert_eq!(
        persisted_post.owner_id.as_deref(),
        Some(expected_owner_id.as_str())
    );
    assert_eq!(
        persisted_post.owner_full_name.as_deref(),
        content.author_name.as_deref()
    );
    assert_eq!(persisted_post.media_url.as_deref(), content.url.as_deref());
    assert_eq!(
        persisted_post.thumbnail_url.as_deref(),
        Some(expected_thumbnail.as_str())
    );
    assert_eq!(
        persisted_post.like_count,
        Some(content.engagement.likes as i32)
    );
    assert_eq!(
        persisted_post.comment_count,
        Some(content.engagement.comments as i32)
    );
    assert_eq!(
        persisted_post.play_count,
        Some(content.engagement.views as i32)
    );
    assert_eq!(persisted_post.taken_at_ts, content.created_at);

    let comment = glance_mind_agent_rs::Comment::new(
        "instagram",
        format!("ig-real-db-test-{}", content.content_id),
        content.content_id.clone(),
    )
    .with_author("commenter")
    .with_author_name("Commenter Name")
    .with_author_uid("commenter-id")
    .with_text("Great post from real DB test")
    .with_likes(7)
    .with_reply_count(2)
    .with_created_at(1_744_123_839);
    let suggestion = ReplySuggestion::new(comment.comment_id.clone())
        .with_reply("Thanks from instagram real db test")
        .with_dm("DM from instagram real db test")
        .with_post_reply("Post reply from instagram real db test")
        .with_reason("instagram real db test reason");

    let comment_id = repo
        .save_comment_with_analysis(&comment, save_result.id, campaign_id, &suggestion)
        .await
        .expect("Instagram comment analysis should save to gm_agent_instagram_comments");

    let persisted_comment = diesel::sql_query(format!(
        r#"
        SELECT instagram_comment_id, comment_text, comment_user_id, comment_username,
               comment_user_full_name, reason, suggested_reply, suggested_dm, suggested_reply_post
        FROM gm_agent_instagram_comments
        WHERE id = {comment_id}
        "#
    ))
    .get_result::<InstagramCommentRow>(&mut conn)
    .expect("saved Instagram comment should be queryable");

    assert_eq!(persisted_comment.instagram_comment_id, comment.comment_id);
    assert_eq!(
        persisted_comment.comment_text.as_deref(),
        Some(comment.text.as_str())
    );
    assert_eq!(
        persisted_comment.comment_user_id.as_deref(),
        comment.author_uid.as_deref()
    );
    assert_eq!(
        persisted_comment.comment_username.as_deref(),
        Some(comment.author.as_str())
    );
    assert_eq!(
        persisted_comment.comment_user_full_name.as_deref(),
        comment.author_name.as_deref()
    );
    assert_eq!(
        persisted_comment.reason.as_deref(),
        suggestion.reason.as_deref()
    );
    assert_eq!(
        persisted_comment.suggested_reply.as_deref(),
        suggestion.reply_text.as_deref()
    );
    assert_eq!(
        persisted_comment.suggested_dm.as_deref(),
        suggestion.dm_text.as_deref()
    );
    assert_eq!(
        persisted_comment.suggested_reply_post.as_deref(),
        suggestion.post_reply_text.as_deref()
    );

    cleanup_supporting_rows(&mut conn, campaign_id, task_id);
}
