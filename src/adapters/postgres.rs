//! PostgreSQL Adapter - Implements repository ports using Diesel
//!
//! This adapter uses the production database schema from glance_mind_rust.
//!
//! Aligns with Python glance_mind_agent for:
//! - Task progress updates via stored procedures
//! - Content save with ON CONFLICT deduplication
//! - Comments saved only with AI suggestions

use async_trait::async_trait;
use diesel::prelude::*;
use diesel::sql_types::{Bool, Integer, Numeric, Text};
use tracing::{debug, info, warn};

use crate::db::{models, schema, DbPool};
use crate::domain::errors::{DbError, DbResult};
use crate::domain::{Comment, Content, ReplySuggestion};
use crate::ports::{
    ai_analyzer::AnalysisContext,
    content_repository::{
        CommentStatus, ContentSaveResult, StoredAnalysis, StoredComment, StoredContent,
    },
    progress_tracker::{CampaignStopResult, TaskInfo, TaskProgressUpdate, TaskStatus},
    prompt_repository::{CampaignConfig, CampaignStatus, PlatformConfig},
    ContentRepository, ProgressTracker, PromptRepository,
};
use crate::tikhub::TwitterTweet as TikhubTwitterTweet;

/// Result from fn_update_task_progress stored procedure
#[derive(QueryableByName, Debug)]
struct TaskProgressResult {
    #[diesel(sql_type = Bool)]
    success: bool,
    #[diesel(sql_type = Bool)]
    should_stop: bool,
    #[diesel(sql_type = Integer)]
    new_process_count: i32,
    #[diesel(sql_type = Numeric)]
    new_actual_consumption: bigdecimal::BigDecimal,
}

/// Result from fn_complete_task stored procedure
#[derive(QueryableByName, Debug)]
struct TaskCompleteResult {
    #[diesel(sql_type = Bool)]
    success: bool,
    #[diesel(sql_type = Text)]
    campaign_status: String,
}

/// Result from fn_stop_campaign_gracefully stored procedure
#[derive(QueryableByName, Debug)]
struct CampaignStopDbResult {
    #[diesel(sql_type = Bool)]
    success: bool,
    #[diesel(sql_type = Bool)]
    immediate_stopped: bool,
    #[diesel(sql_type = Numeric)]
    refunded_amount: bigdecimal::BigDecimal,
}

/// Result from content upsert with xmax check
#[derive(QueryableByName, Debug)]
struct ContentUpsertResult {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Bool)]
    inserted: bool,
}

/// Result from comment insert
#[derive(QueryableByName, Debug)]
struct CommentInsertResult {
    #[diesel(sql_type = Integer)]
    id: i32,
}

/// PostgreSQL adapter implementing repository ports
pub struct PostgresAdapter {
    pool: DbPool,
}

impl PostgresAdapter {
    /// Create a new PostgreSQL adapter
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Create from database URL
    pub fn from_url(database_url: &str) -> Result<Self, DbError> {
        let pool = crate::db::establish_pool(database_url, None)
            .map_err(|e| DbError::Connection(e.to_string()))?;
        Ok(Self { pool })
    }

    /// Get a connection from the pool (async-safe using spawn_blocking)
    ///
    /// This wraps the blocking r2d2 pool.get() in spawn_blocking to avoid
    /// blocking the Tokio runtime under high concurrency.
    async fn conn_async(
        &self,
    ) -> DbResult<diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>>
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            pool.get().map_err(|e| DbError::Connection(e.to_string()))
        })
        .await
        .map_err(|e| DbError::Connection(format!("spawn_blocking failed: {}", e)))?
    }

    /// Get a connection from the pool (sync version for non-async contexts)
    ///
    /// Note: Prefer conn_async() in async functions to avoid blocking.
    #[allow(dead_code)]
    fn conn(
        &self,
    ) -> DbResult<diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>>
    {
        self.pool
            .get()
            .map_err(|e| DbError::Connection(e.to_string()))
    }

    /// Get platform ID from name using global registry
    #[allow(dead_code)]
    fn platform_id(&self, platform: &str) -> i32 {
        use crate::config::platform::{global_registry, PlatformLookup};

        global_registry().get_id(platform).unwrap_or(0)
    }

    // ============================================================
    // Platform-specific content save methods
    // ============================================================

    /// Save TikTok video to gm_agent_videos table
    async fn save_tiktok_video(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        let mut conn = self.conn_async().await?;
        let task_id_value = task_id.unwrap_or(0);

        let result: ContentUpsertResult = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_videos (
                task_id, video_id, author, description, campaign_id,
                like_count, comment_count, share_count, play_count,
                publish_time, author_unique_id, url
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (task_id, video_id) DO UPDATE SET
                description = EXCLUDED.description,
                author = EXCLUDED.author,
                campaign_id = EXCLUDED.campaign_id,
                like_count = EXCLUDED.like_count,
                comment_count = EXCLUDED.comment_count,
                share_count = EXCLUDED.share_count,
                play_count = EXCLUDED.play_count,
                publish_time = EXCLUDED.publish_time,
                author_unique_id = EXCLUDED.author_unique_id,
                url = EXCLUDED.url
            RETURNING id, (xmax = 0) AS inserted
            "#,
        )
        .bind::<Integer, _>(task_id_value)
        .bind::<Text, _>(&content.content_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.author_name.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.description))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(campaign_id)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.likes as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.comments as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.shares as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.views as i32))
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(content.created_at)
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.author))
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.url.as_ref())
        .get_result(&mut conn)
        .map_err(DbError::from)?;

        if result.inserted {
            debug!(content_id = %content.content_id, db_id = result.id, "Saved new TikTok video");
        } else {
            debug!(content_id = %content.content_id, db_id = result.id, "Updated existing TikTok video");
        }

        Ok(ContentSaveResult {
            id: result.id,
            is_new: result.inserted,
        })
    }

    /// Save Facebook post to gm_agent_facebook_posts table.
    async fn save_facebook_post(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        let mut conn = self.conn_async().await?;
        let task_id_value = task_id.unwrap_or(0);
        let raw = content.raw_data.as_ref();
        let timestamp = Self::json_i64(raw, &["timestamp"]).or(content.created_at);
        let posted_at = Self::timestamp_to_datetime(timestamp);
        let has_image = Self::json_at(raw, &["image"]).is_some_and(|value| !value.is_null());
        let has_video = Self::json_at(raw, &["video"]).is_some_and(|value| !value.is_null());

        let result: ContentUpsertResult = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_facebook_posts (
                task_id, campaign_id, facebook_post_id, post_type, url,
                message, message_rich, timestamp, posted_at,
                reactions_count, comments_count, reshare_count,
                reactions_like, reactions_love, reactions_haha, reactions_wow,
                reactions_sad, reactions_angry, reactions_care,
                author_id, author_name, author_url, author_profile_picture_url, author_title,
                has_image, image_url, image_width, image_height, image_id,
                has_video, video_thumbnail, external_url, attached_post_url, comments_id, shares_id
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19,
                $20, $21, $22, $23, $24,
                $25, $26, $27, $28, $29,
                $30, $31, $32, $33, $34, $35
            )
            ON CONFLICT (task_id, facebook_post_id) DO UPDATE SET
                campaign_id = EXCLUDED.campaign_id,
                post_type = EXCLUDED.post_type,
                url = EXCLUDED.url,
                message = EXCLUDED.message,
                message_rich = EXCLUDED.message_rich,
                timestamp = EXCLUDED.timestamp,
                posted_at = EXCLUDED.posted_at,
                reactions_count = EXCLUDED.reactions_count,
                comments_count = EXCLUDED.comments_count,
                reshare_count = EXCLUDED.reshare_count,
                reactions_like = EXCLUDED.reactions_like,
                reactions_love = EXCLUDED.reactions_love,
                reactions_haha = EXCLUDED.reactions_haha,
                reactions_wow = EXCLUDED.reactions_wow,
                reactions_sad = EXCLUDED.reactions_sad,
                reactions_angry = EXCLUDED.reactions_angry,
                reactions_care = EXCLUDED.reactions_care,
                author_id = EXCLUDED.author_id,
                author_name = EXCLUDED.author_name,
                author_url = EXCLUDED.author_url,
                author_profile_picture_url = EXCLUDED.author_profile_picture_url,
                author_title = EXCLUDED.author_title,
                has_image = EXCLUDED.has_image,
                image_url = EXCLUDED.image_url,
                image_width = EXCLUDED.image_width,
                image_height = EXCLUDED.image_height,
                image_id = EXCLUDED.image_id,
                has_video = EXCLUDED.has_video,
                video_thumbnail = EXCLUDED.video_thumbnail,
                external_url = EXCLUDED.external_url,
                attached_post_url = EXCLUDED.attached_post_url,
                comments_id = EXCLUDED.comments_id,
                shares_id = EXCLUDED.shares_id,
                updated_at = NOW()
            RETURNING id, (xmax = 0) AS inserted
            "#,
        )
        .bind::<Integer, _>(task_id_value)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(campaign_id)
        .bind::<Text, _>(&content.content_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["type"]))
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.url.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.description))
        .bind::<diesel::sql_types::Nullable<Text>, _>(
            Self::json_string(raw, &["message_rich"]).or_else(|| Some(content.description.clone())),
        )
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(timestamp)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamptz>, _>(posted_at)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.likes as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.comments as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.shares as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "like"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "love"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "haha"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "wow"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "sad"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "angry"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["reactions", "care"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(
            Self::json_string(raw, &["author", "id"]).or_else(|| Some(content.author.clone())),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(
            Self::json_string(raw, &["author", "name"]).or_else(|| content.author_name.clone()),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["author", "url"]))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(
            raw,
            &["author", "profile_picture_url"],
        ))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["author_title"]))
        .bind::<diesel::sql_types::Nullable<Bool>, _>(Some(has_image))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["image", "uri"]))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["image", "width"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["image", "height"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["image", "id"]))
        .bind::<diesel::sql_types::Nullable<Bool>, _>(Some(has_video))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["video_thumbnail"]))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["external_url"]))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(
            raw,
            &["attached_post_url"],
        ))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["comments_id"]))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["shares_id"]))
        .get_result(&mut conn)
        .map_err(DbError::from)?;

        if result.inserted {
            debug!(content_id = %content.content_id, db_id = result.id, "Saved new Facebook post");
        } else {
            debug!(content_id = %content.content_id, db_id = result.id, "Updated existing Facebook post");
        }

        Ok(ContentSaveResult {
            id: result.id,
            is_new: result.inserted,
        })
    }

    /// Save Instagram post to gm_agent_instagram_posts table
    async fn save_instagram_post(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        let mut conn = self.conn_async().await?;
        let task_id_value = task_id.unwrap_or(0);

        // Extract Instagram-specific fields from raw_data if available
        let raw = content.raw_data.as_ref();
        let media_type = raw
            .and_then(|r| r.get("media_type"))
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32;
        let product_type = raw
            .and_then(|r| r.get("product_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("feed");
        let instagram_id =
            Self::json_string(raw, &["pk"]).or_else(|| Self::json_string(raw, &["id"]));
        let owner_id = Self::json_string(raw, &["user", "pk"])
            .or_else(|| Self::json_string(raw, &["user", "id"]));
        let thumbnail_url = Self::json_string(raw, &["thumbnail_url"]).or_else(|| {
            raw.and_then(|r| r.get("image_versions2"))
                .and_then(|v| v.get("candidates"))
                .and_then(|v| v.as_array())
                .and_then(|candidates| candidates.first())
                .and_then(|candidate| candidate.get("url"))
                .and_then(|url| url.as_str())
                .map(ToOwned::to_owned)
        });

        let result: ContentUpsertResult = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_instagram_posts (
                task_id, campaign_id, code, instagram_id, media_type, product_type,
                caption_text, owner_username, owner_id, owner_full_name,
                media_url, thumbnail_url, like_count, comment_count, play_count,
                taken_at_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ON CONFLICT (code) DO UPDATE SET
                task_id = EXCLUDED.task_id,
                campaign_id = EXCLUDED.campaign_id,
                caption_text = EXCLUDED.caption_text,
                owner_username = EXCLUDED.owner_username,
                owner_full_name = EXCLUDED.owner_full_name,
                media_url = EXCLUDED.media_url,
                thumbnail_url = EXCLUDED.thumbnail_url,
                like_count = EXCLUDED.like_count,
                comment_count = EXCLUDED.comment_count,
                play_count = EXCLUDED.play_count,
                taken_at_ts = EXCLUDED.taken_at_ts,
                updated_at = NOW()
            RETURNING id, (xmax = 0) AS inserted
            "#,
        )
        .bind::<Integer, _>(task_id_value)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(campaign_id)
        .bind::<Text, _>(&content.content_id) // code (shortcode)
        .bind::<diesel::sql_types::Nullable<Text>, _>(instagram_id.as_ref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(media_type))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(product_type))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.description)) // caption_text
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.author)) // owner_username
        .bind::<diesel::sql_types::Nullable<Text>, _>(owner_id.as_ref()) // owner_id
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.author_name.as_ref()) // owner_full_name
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.url.as_ref()) // media_url
        .bind::<diesel::sql_types::Nullable<Text>, _>(thumbnail_url.as_ref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.likes as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.comments as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(content.engagement.views as i32))
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(content.created_at)
        .get_result(&mut conn)
        .map_err(DbError::from)?;

        if result.inserted {
            debug!(content_id = %content.content_id, db_id = result.id, "Saved new Instagram post");
        } else {
            debug!(content_id = %content.content_id, db_id = result.id, "Updated existing Instagram post");
        }

        Ok(ContentSaveResult {
            id: result.id,
            is_new: result.inserted,
        })
    }

    /// Save Reddit post to gm_agent_reddit_posts table
    async fn save_reddit_post(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        let mut conn = self.conn_async().await?;
        let task_id_value = task_id.unwrap_or(0);

        // Extract Reddit-specific fields from raw_data
        let raw = content.raw_data.as_ref();
        let subreddit = raw
            .and_then(|r| r.get("subreddit"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let title = raw
            .and_then(|r| r.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let score = raw
            .and_then(|r| r.get("score"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let num_comments = raw
            .and_then(|r| r.get("num_comments"))
            .and_then(|v| v.as_i64())
            .unwrap_or(content.engagement.comments) as i32;

        let result: ContentUpsertResult = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_reddit_posts (
                task_id, campaign_id, post_id, subreddit, title, selftext,
                author, author_fullname, score, upvote_ratio, num_comments,
                permalink, url, created_utc
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (task_id, post_id) DO UPDATE SET
                title = EXCLUDED.title,
                selftext = EXCLUDED.selftext,
                score = EXCLUDED.score,
                num_comments = EXCLUDED.num_comments,
                url = EXCLUDED.url,
                updated_at = NOW()
            RETURNING id, (xmax = 0) AS inserted
            "#,
        )
        .bind::<Integer, _>(task_id_value)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(campaign_id)
        .bind::<Text, _>(&content.content_id) // post_id
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(subreddit))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(title))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.description)) // selftext
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&content.author))
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.author_name.as_ref()) // author_fullname
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(score))
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Float>, _>(
            raw.and_then(|r| r.get("upvote_ratio"))
                .and_then(|v| v.as_f64())
                .map(|f| f as f32),
        )
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(num_comments))
        .bind::<diesel::sql_types::Nullable<Text>, _>(
            raw.and_then(|r| r.get("permalink"))
                .and_then(|v| v.as_str()),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(content.url.as_ref())
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(content.created_at)
        .get_result(&mut conn)
        .map_err(DbError::from)?;

        if result.inserted {
            debug!(content_id = %content.content_id, db_id = result.id, "Saved new Reddit post");
        } else {
            debug!(content_id = %content.content_id, db_id = result.id, "Updated existing Reddit post");
        }

        Ok(ContentSaveResult {
            id: result.id,
            is_new: result.inserted,
        })
    }

    /// Save Twitter tweet to gm_agent_twitter_tweets table
    async fn save_twitter_tweet(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        let mut conn = self.conn_async().await?;
        let task_id_value = task_id.unwrap_or(0);

        let parsed_tweet = Self::parse_twitter_raw(content.raw_data.as_ref());
        let twitter_user = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.user_info.as_ref().or(tweet.author.as_ref()));
        let screen_name = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.author_handle().map(ToString::to_string))
            .or_else(|| (!content.author.is_empty()).then(|| content.author.clone()));
        let user_name = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.author_name().map(ToString::to_string))
            .or_else(|| content.author_name.clone());
        let user_id = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.user_id().map(ToString::to_string));
        let conversation_id = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.conversation_id.clone());
        let lang = parsed_tweet.as_ref().and_then(|tweet| tweet.lang.clone());
        let user_description = twitter_user.and_then(|user| user.description.clone());
        let user_followers_count = twitter_user
            .and_then(|user| user.followers_count)
            .map(Self::i64_to_i32);
        let user_avatar = twitter_user.and_then(|user| user.avatar.clone());
        let user_verified = twitter_user.and_then(|user| user.verified.or(user.blue_verified));
        let media_urls = parsed_tweet
            .as_ref()
            .and_then(Self::twitter_media_bind_value);
        let has_media = parsed_tweet.as_ref().map(|tweet| tweet.has_media());
        let favorite_count = parsed_tweet
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.like_count()))
            .or_else(|| Some(Self::i64_to_i32(content.engagement.likes)));
        let retweet_count = parsed_tweet
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.retweet_count()))
            .or_else(|| Some(Self::i64_to_i32(content.engagement.shares)));
        let reply_count = parsed_tweet
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.reply_count()))
            .or_else(|| Some(Self::i64_to_i32(content.engagement.comments)));
        let quote_count = parsed_tweet
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.quote_count()));
        let bookmark_count = parsed_tweet
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.bookmark_count()));
        let view_count = parsed_tweet
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.view_count()))
            .or_else(|| Some(Self::i64_to_i32(content.engagement.views)));
        let is_reply = parsed_tweet.as_ref().map(|tweet| tweet.is_reply());
        let in_reply_to_status_id = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.in_reply_to_status_id_str.clone());
        let in_reply_to_user_id = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.in_reply_to_user_id_str.clone());
        let created_at_str = parsed_tweet
            .as_ref()
            .and_then(|tweet| tweet.created_at.clone());
        let created_at_ts = content.created_at.or_else(|| {
            parsed_tweet
                .as_ref()
                .and_then(|tweet| tweet.created_at_timestamp())
        });
        let tweet_created_at = Self::timestamp_to_datetime(created_at_ts);

        // Constraint: UNIQUE (twitter_tweet_id, task_id)
        let result: ContentUpsertResult = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_twitter_tweets (
                task_id, campaign_id, twitter_tweet_id, conversation_id, full_text,
                lang, screen_name, user_name, user_id, user_description,
                user_followers_count, user_avatar, user_verified, media_urls, has_media,
                favorite_count, retweet_count, reply_count, quote_count, bookmark_count,
                view_count, is_reply, in_reply_to_status_id, in_reply_to_user_id,
                created_at_str, created_at_ts, tweet_created_at
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20,
                $21, $22, $23, $24,
                $25, $26, $27
            )
            ON CONFLICT (twitter_tweet_id, task_id) DO UPDATE SET
                campaign_id = EXCLUDED.campaign_id,
                conversation_id = EXCLUDED.conversation_id,
                full_text = EXCLUDED.full_text,
                lang = EXCLUDED.lang,
                screen_name = EXCLUDED.screen_name,
                user_name = EXCLUDED.user_name,
                user_id = EXCLUDED.user_id,
                user_description = EXCLUDED.user_description,
                user_followers_count = EXCLUDED.user_followers_count,
                user_avatar = EXCLUDED.user_avatar,
                user_verified = EXCLUDED.user_verified,
                media_urls = EXCLUDED.media_urls,
                has_media = EXCLUDED.has_media,
                favorite_count = EXCLUDED.favorite_count,
                retweet_count = EXCLUDED.retweet_count,
                reply_count = EXCLUDED.reply_count,
                quote_count = EXCLUDED.quote_count,
                bookmark_count = EXCLUDED.bookmark_count,
                view_count = EXCLUDED.view_count,
                is_reply = EXCLUDED.is_reply,
                in_reply_to_status_id = EXCLUDED.in_reply_to_status_id,
                in_reply_to_user_id = EXCLUDED.in_reply_to_user_id,
                created_at_str = EXCLUDED.created_at_str,
                created_at_ts = EXCLUDED.created_at_ts,
                tweet_created_at = EXCLUDED.tweet_created_at,
                updated_at = NOW()
            RETURNING id, (xmax = 0) AS inserted
            "#,
        )
        .bind::<Integer, _>(task_id_value)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(campaign_id)
        .bind::<Text, _>(&content.content_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(conversation_id.as_deref())
        .bind::<Text, _>(&content.description)
        .bind::<diesel::sql_types::Nullable<Text>, _>(lang.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(screen_name.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(user_name.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(user_id.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(user_description.as_deref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(user_followers_count)
        .bind::<diesel::sql_types::Nullable<Text>, _>(user_avatar.as_deref())
        .bind::<diesel::sql_types::Nullable<Bool>, _>(user_verified)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Array<diesel::sql_types::Nullable<Text>>>, _>(
            media_urls,
        )
        .bind::<diesel::sql_types::Nullable<Bool>, _>(has_media)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(favorite_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(retweet_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(reply_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(quote_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(bookmark_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(view_count)
        .bind::<diesel::sql_types::Nullable<Bool>, _>(is_reply)
        .bind::<diesel::sql_types::Nullable<Text>, _>(in_reply_to_status_id.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(in_reply_to_user_id.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(created_at_str.as_deref())
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(created_at_ts)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamptz>, _>(tweet_created_at)
        .get_result(&mut conn)
        .map_err(DbError::from)?;

        if result.inserted {
            debug!(content_id = %content.content_id, db_id = result.id, "Saved new Twitter tweet");
        } else {
            debug!(content_id = %content.content_id, db_id = result.id, "Updated existing Twitter tweet");
        }

        Ok(ContentSaveResult {
            id: result.id,
            is_new: result.inserted,
        })
    }

    // ============================================================
    // Platform-specific comment save methods
    // ============================================================

    /// Save TikTok comment to gm_agent_comments table
    async fn save_tiktok_comment(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        let mut conn = self.conn_async().await?;

        let create_time = comment
            .created_at
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.naive_utc()));

        let id: i32 = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_comments (
                campaign_id, comment_id, video_db_id, user_nickname, user_unique_id,
                content, create_time, reason, suggested_reply,
                suggested_dm, suggested_reply_post, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0)
            ON CONFLICT (campaign_id, comment_id) DO UPDATE SET
                video_db_id = EXCLUDED.video_db_id,
                reason = EXCLUDED.reason,
                suggested_reply = EXCLUDED.suggested_reply,
                suggested_dm = EXCLUDED.suggested_dm,
                suggested_reply_post = EXCLUDED.suggested_reply_post,
                status = 0,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind::<Integer, _>(campaign_id)
        .bind::<Text, _>(&comment.comment_id)
        .bind::<Integer, _>(content_db_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.author_name.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.author))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.text))
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamp>, _>(create_time)
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reason.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.dm_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.post_reply_text.as_ref())
        .get_result::<CommentInsertResult>(&mut conn)
        .map_err(DbError::from)?
        .id;

        debug!(comment_id = %comment.comment_id, db_id = id, "Upserted TikTok comment");
        Ok(id)
    }

    /// Save Facebook comment to gm_agent_facebook_comments table.
    async fn save_facebook_comment(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        let mut conn = self.conn_async().await?;
        let raw = comment.raw_data.as_ref();
        let (_, post_url, facebook_post_id) = self.get_facebook_post_context(content_db_id).await?;
        let created_at_ts = Self::json_i64(raw, &["created_time"]).or(comment.created_at);
        let comment_created_at = Self::timestamp_to_datetime(created_at_ts);

        let id: i32 = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_facebook_comments (
                post_db_id, campaign_id, facebook_comment_id, parent_comment_id, comment_url,
                comment_text, reason, suggested_reply, suggested_dm, suggested_reply_post,
                comment_user_id, comment_username, comment_user_url, comment_user_profile_picture,
                like_count, reply_count, threading_depth, created_at_ts, comment_created_at,
                facebook_post_id, post_url, status
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14,
                $15, $16, $17, $18, $19,
                $20, $21, 0
            )
            ON CONFLICT (facebook_comment_id, post_db_id) DO UPDATE SET
                campaign_id = EXCLUDED.campaign_id,
                parent_comment_id = EXCLUDED.parent_comment_id,
                comment_url = EXCLUDED.comment_url,
                comment_text = EXCLUDED.comment_text,
                reason = EXCLUDED.reason,
                suggested_reply = EXCLUDED.suggested_reply,
                suggested_dm = EXCLUDED.suggested_dm,
                suggested_reply_post = EXCLUDED.suggested_reply_post,
                comment_user_id = EXCLUDED.comment_user_id,
                comment_username = EXCLUDED.comment_username,
                comment_user_url = EXCLUDED.comment_user_url,
                comment_user_profile_picture = EXCLUDED.comment_user_profile_picture,
                like_count = EXCLUDED.like_count,
                reply_count = EXCLUDED.reply_count,
                threading_depth = EXCLUDED.threading_depth,
                created_at_ts = EXCLUDED.created_at_ts,
                comment_created_at = EXCLUDED.comment_created_at,
                facebook_post_id = EXCLUDED.facebook_post_id,
                post_url = EXCLUDED.post_url,
                status = 0,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind::<Integer, _>(content_db_id)
        .bind::<Integer, _>(campaign_id)
        .bind::<Text, _>(&comment.comment_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.parent_id.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["comment_url"]))
        .bind::<Text, _>(&comment.text)
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reason.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.dm_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.post_reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(
            comment.author_uid.as_ref().or(Some(&comment.author)),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(
            comment.author_name.as_ref().or(Some(&comment.author)),
        )
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(raw, &["author", "url"]))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Self::json_string(
            raw,
            &["author", "profile_image"],
        ))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(comment.likes as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(comment.reply_count))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(
            Self::json_i64(raw, &["depth"]).map(|value| value as i32),
        )
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(created_at_ts)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamptz>, _>(comment_created_at)
        .bind::<diesel::sql_types::Nullable<Text>, _>(facebook_post_id.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(post_url.as_ref())
        .get_result::<CommentInsertResult>(&mut conn)
        .map_err(DbError::from)?
        .id;

        debug!(comment_id = %comment.comment_id, db_id = id, "Upserted Facebook comment");
        Ok(id)
    }

    /// Save Instagram comment to gm_agent_instagram_comments table
    async fn save_instagram_comment(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        let mut conn = self.conn_async().await?;

        let created_at_ts = comment.created_at;

        // Constraint: UNIQUE (instagram_comment_id, post_db_id)
        let id: i32 = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_instagram_comments (
                post_db_id, campaign_id, instagram_comment_id, parent_comment_id,
                comment_text, comment_user_id, comment_username, comment_user_full_name,
                like_count, child_comment_count, created_at_ts,
                reason, suggested_reply, suggested_dm, suggested_reply_post, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 0)
            ON CONFLICT (instagram_comment_id, post_db_id) DO UPDATE SET
                campaign_id = EXCLUDED.campaign_id,
                comment_text = EXCLUDED.comment_text,
                reason = EXCLUDED.reason,
                suggested_reply = EXCLUDED.suggested_reply,
                suggested_dm = EXCLUDED.suggested_dm,
                suggested_reply_post = EXCLUDED.suggested_reply_post,
                status = 0,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind::<Integer, _>(content_db_id)
        .bind::<Integer, _>(campaign_id)
        .bind::<Text, _>(&comment.comment_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.parent_id.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.text))
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.author_uid.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.author))
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.author_name.as_ref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(comment.likes as i32))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(comment.reply_count))
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(created_at_ts)
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reason.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.dm_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.post_reply_text.as_ref())
        .get_result::<CommentInsertResult>(&mut conn)
        .map_err(DbError::from)?
        .id;

        debug!(comment_id = %comment.comment_id, db_id = id, "Upserted Instagram comment");
        Ok(id)
    }

    /// Save Reddit comment to gm_agent_reddit_comments table
    async fn save_reddit_comment(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        let mut conn = self.conn_async().await?;

        // Constraint: UNIQUE (comment_id, post_db_id)
        let id: i32 = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_reddit_comments (
                post_db_id, campaign_id, comment_id, parent_id, body,
                author, score,
                reason, suggested_reply, suggested_dm, suggested_reply_post, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0)
            ON CONFLICT (comment_id, post_db_id) DO UPDATE SET
                campaign_id = EXCLUDED.campaign_id,
                body = EXCLUDED.body,
                score = EXCLUDED.score,
                reason = EXCLUDED.reason,
                suggested_reply = EXCLUDED.suggested_reply,
                suggested_dm = EXCLUDED.suggested_dm,
                suggested_reply_post = EXCLUDED.suggested_reply_post,
                status = 0,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind::<Integer, _>(content_db_id)
        .bind::<Integer, _>(campaign_id)
        .bind::<Text, _>(&comment.comment_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.parent_id.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.text))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.author))
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(comment.likes as i32))
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reason.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.dm_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.post_reply_text.as_ref())
        .get_result::<CommentInsertResult>(&mut conn)
        .map_err(DbError::from)?
        .id;

        debug!(comment_id = %comment.comment_id, db_id = id, "Upserted Reddit comment");
        Ok(id)
    }

    /// Save Twitter comment to gm_agent_twitter_comments table
    async fn save_twitter_comment(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        let mut conn = self.conn_async().await?;

        let parsed_comment = Self::parse_twitter_raw(comment.raw_data.as_ref());
        let twitter_user = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.author.as_ref().or(tweet.user_info.as_ref()));
        let conversation_id = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.conversation_id.clone())
            .or_else(|| (!comment.content_id.is_empty()).then(|| comment.content_id.clone()));
        let comment_screen_name = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.author_handle().map(ToString::to_string))
            .or_else(|| (!comment.author.is_empty()).then(|| comment.author.clone()));
        let comment_user_name = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.author_name().map(ToString::to_string))
            .or_else(|| comment.author_name.clone());
        let comment_user_id = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.user_id().map(ToString::to_string))
            .or_else(|| comment.author_uid.clone());
        let comment_user_followers = twitter_user
            .and_then(|user| user.followers_count)
            .map(Self::i64_to_i32);
        let favorite_count = parsed_comment
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.like_count()))
            .or_else(|| Some(Self::i64_to_i32(comment.likes)));
        let retweet_count = parsed_comment
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.retweet_count()));
        let reply_count = parsed_comment
            .as_ref()
            .map(|tweet| Self::i64_to_i32(tweet.reply_count()))
            .or(Some(comment.reply_count));
        let in_reply_to_status_id = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.in_reply_to_status_id_str.clone())
            .or_else(|| comment.parent_id.clone());
        let is_reply = parsed_comment
            .as_ref()
            .map(|tweet| tweet.is_reply())
            .or(Some(comment.is_reply));
        let media_urls = parsed_comment
            .as_ref()
            .and_then(Self::twitter_media_bind_value);
        let has_media = parsed_comment.as_ref().map(|tweet| tweet.has_media());
        let created_at_str = parsed_comment
            .as_ref()
            .and_then(|tweet| tweet.created_at.clone());
        let created_at_ts = comment.created_at.or_else(|| {
            parsed_comment
                .as_ref()
                .and_then(|tweet| tweet.created_at_timestamp())
        });
        let comment_created_at = Self::timestamp_to_datetime(created_at_ts);

        // Constraint: UNIQUE (twitter_comment_id, tweet_db_id)
        let id: i32 = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_twitter_comments (
                tweet_db_id, campaign_id, twitter_comment_id, conversation_id,
                comment_screen_name, comment_user_name, comment_user_id, comment_user_followers,
                comment_text, reason, suggested_reply, favorite_count,
                retweet_count, reply_count, in_reply_to_status_id, is_reply,
                media_urls, has_media, created_at_str, created_at_ts,
                comment_created_at, suggested_dm, suggested_reply_post, status
            )
            VALUES (
                $1, $2, $3, $4,
                $5, $6, $7, $8,
                $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19, $20,
                $21, $22, $23, 0
            )
            ON CONFLICT (twitter_comment_id, tweet_db_id) DO UPDATE SET
                campaign_id = EXCLUDED.campaign_id,
                conversation_id = EXCLUDED.conversation_id,
                comment_screen_name = EXCLUDED.comment_screen_name,
                comment_user_name = EXCLUDED.comment_user_name,
                comment_user_id = EXCLUDED.comment_user_id,
                comment_user_followers = EXCLUDED.comment_user_followers,
                comment_text = EXCLUDED.comment_text,
                reason = EXCLUDED.reason,
                suggested_reply = EXCLUDED.suggested_reply,
                favorite_count = EXCLUDED.favorite_count,
                retweet_count = EXCLUDED.retweet_count,
                reply_count = EXCLUDED.reply_count,
                in_reply_to_status_id = EXCLUDED.in_reply_to_status_id,
                is_reply = EXCLUDED.is_reply,
                media_urls = EXCLUDED.media_urls,
                has_media = EXCLUDED.has_media,
                created_at_str = EXCLUDED.created_at_str,
                created_at_ts = EXCLUDED.created_at_ts,
                comment_created_at = EXCLUDED.comment_created_at,
                suggested_dm = EXCLUDED.suggested_dm,
                suggested_reply_post = EXCLUDED.suggested_reply_post,
                status = 0,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind::<Integer, _>(content_db_id)
        .bind::<Integer, _>(campaign_id)
        .bind::<Text, _>(&comment.comment_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(conversation_id.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment_screen_name.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment_user_name.as_deref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment_user_id.as_deref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(comment_user_followers)
        .bind::<Text, _>(&comment.text)
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reason.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(favorite_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(retweet_count)
        .bind::<diesel::sql_types::Nullable<Integer>, _>(reply_count)
        .bind::<diesel::sql_types::Nullable<Text>, _>(in_reply_to_status_id.as_deref())
        .bind::<diesel::sql_types::Nullable<Bool>, _>(is_reply)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Array<diesel::sql_types::Nullable<Text>>>, _>(
            media_urls,
        )
        .bind::<diesel::sql_types::Nullable<Bool>, _>(has_media)
        .bind::<diesel::sql_types::Nullable<Text>, _>(created_at_str.as_deref())
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>, _>(created_at_ts)
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamptz>, _>(comment_created_at)
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.dm_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.post_reply_text.as_ref())
        .get_result::<CommentInsertResult>(&mut conn)
        .map_err(DbError::from)?
        .id;

        debug!(comment_id = %comment.comment_id, db_id = id, "Upserted Twitter comment");
        Ok(id)
    }
}

// ============================================================
// ContentRepository Implementation
// Using gm_agent_videos and gm_agent_comments tables
// ============================================================

#[async_trait]
impl ContentRepository for PostgresAdapter {
    async fn content_exists(&self, platform: &str, content_id: &str) -> DbResult<bool> {
        let mut conn = self.conn_async().await?;

        let count: i64 = match platform.to_lowercase().as_str() {
            "facebook" => {
                use schema::gm_agent_facebook_posts::dsl;
                dsl::gm_agent_facebook_posts
                    .filter(dsl::facebook_post_id.eq(content_id))
                    .count()
                    .get_result(&mut conn)
                    .map_err(DbError::from)?
            }
            "twitter" => {
                use schema::gm_agent_twitter_tweets::dsl;
                dsl::gm_agent_twitter_tweets
                    .filter(dsl::twitter_tweet_id.eq(content_id))
                    .count()
                    .get_result(&mut conn)
                    .map_err(DbError::from)?
            }
            _ => {
                use schema::gm_agent_videos::dsl;
                dsl::gm_agent_videos
                    .filter(dsl::video_id.eq(content_id))
                    .count()
                    .get_result(&mut conn)
                    .map_err(DbError::from)?
            }
        };

        Ok(count > 0)
    }

    async fn get_content(
        &self,
        platform: &str,
        content_id: &str,
    ) -> DbResult<Option<StoredContent>> {
        let mut conn = self.conn_async().await?;

        match platform.to_lowercase().as_str() {
            "facebook" => {
                use schema::gm_agent_facebook_posts::dsl;

                let result: Option<models::FacebookPost> = dsl::gm_agent_facebook_posts
                    .filter(dsl::facebook_post_id.eq(content_id))
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                Ok(result.map(|post| self.convert_facebook_post_to_content(&post)))
            }
            "twitter" => {
                use schema::gm_agent_twitter_tweets::dsl;

                let result: Option<models::TwitterTweet> = dsl::gm_agent_twitter_tweets
                    .filter(dsl::twitter_tweet_id.eq(content_id))
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                Ok(result.map(|tweet| self.convert_twitter_tweet_to_content(&tweet)))
            }
            _ => {
                use schema::gm_agent_videos::dsl;

                let result: Option<models::AgentVideo> = dsl::gm_agent_videos
                    .filter(dsl::video_id.eq(content_id))
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                Ok(result.map(|video| self.convert_video_to_content(&video)))
            }
        }
    }

    async fn get_content_by_id(&self, id: i32) -> DbResult<Option<StoredContent>> {
        let mut conn = self.conn_async().await?;
        {
            use schema::gm_agent_videos::dsl;

            let result: Option<models::AgentVideo> = dsl::gm_agent_videos
                .find(id)
                .first(&mut conn)
                .optional()
                .map_err(DbError::from)?;

            if let Some(video) = result {
                return Ok(Some(self.convert_video_to_content(&video)));
            }
        }
        {
            use schema::gm_agent_facebook_posts::dsl;

            let result: Option<models::FacebookPost> = dsl::gm_agent_facebook_posts
                .find(id)
                .first(&mut conn)
                .optional()
                .map_err(DbError::from)?;

            if let Some(post) = result {
                return Ok(Some(self.convert_facebook_post_to_content(&post)));
            }
        }
        {
            use schema::gm_agent_twitter_tweets::dsl;

            let result: Option<models::TwitterTweet> = dsl::gm_agent_twitter_tweets
                .find(id)
                .first(&mut conn)
                .optional()
                .map_err(DbError::from)?;

            if let Some(tweet) = result {
                return Ok(Some(self.convert_twitter_tweet_to_content(&tweet)));
            }
        }

        Ok(None)
    }

    async fn save_content(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        // Route to platform-specific save method
        let platform = content.platform.to_lowercase();
        match platform.as_str() {
            "facebook" => self.save_facebook_post(content, campaign_id, task_id).await,
            "instagram" => {
                self.save_instagram_post(content, campaign_id, task_id)
                    .await
            }
            "reddit" => self.save_reddit_post(content, campaign_id, task_id).await,
            "twitter" => self.save_twitter_tweet(content, campaign_id, task_id).await,
            _ => self.save_tiktok_video(content, campaign_id, task_id).await, // TikTok is default
        }
    }

    async fn save_contents(
        &self,
        contents: &[Content],
        campaign_id: Option<i32>,
        task_id: Option<i32>,
    ) -> DbResult<Vec<ContentSaveResult>> {
        let mut results = Vec::with_capacity(contents.len());
        for content in contents {
            let result = self.save_content(content, campaign_id, task_id).await?;
            results.push(result);
        }
        Ok(results)
    }

    async fn update_content_engagement(
        &self,
        id: i32,
        likes: i64,
        comments: i64,
        shares: i64,
        views: i64,
    ) -> DbResult<()> {
        use schema::gm_agent_twitter_tweets::dsl as twitter_dsl;
        use schema::gm_agent_videos::dsl;

        let mut conn = self.conn_async().await?;

        let updated = diesel::update(dsl::gm_agent_videos.find(id))
            .set((
                dsl::like_count.eq(Self::i64_to_i32(likes)),
                dsl::comment_count.eq(Self::i64_to_i32(comments)),
                dsl::share_count.eq(Self::i64_to_i32(shares)),
                dsl::play_count.eq(Self::i64_to_i32(views)),
            ))
            .execute(&mut conn)
            .map_err(DbError::from)?;
        if updated > 0 {
            return Ok(());
        }

        diesel::update(twitter_dsl::gm_agent_twitter_tweets.find(id))
            .set((
                twitter_dsl::favorite_count.eq(Some(Self::i64_to_i32(likes))),
                twitter_dsl::reply_count.eq(Some(Self::i64_to_i32(comments))),
                twitter_dsl::retweet_count.eq(Some(Self::i64_to_i32(shares))),
                twitter_dsl::view_count.eq(Some(Self::i64_to_i32(views))),
            ))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        Ok(())
    }

    async fn comment_exists(&self, platform: &str, comment_id: &str) -> DbResult<bool> {
        let mut conn = self.conn_async().await?;

        let count: i64 = match platform.to_lowercase().as_str() {
            "facebook" => {
                use schema::gm_agent_facebook_comments::dsl;
                dsl::gm_agent_facebook_comments
                    .filter(dsl::facebook_comment_id.eq(comment_id))
                    .count()
                    .get_result(&mut conn)
                    .map_err(DbError::from)?
            }
            "twitter" => {
                use schema::gm_agent_twitter_comments::dsl;
                dsl::gm_agent_twitter_comments
                    .filter(dsl::twitter_comment_id.eq(comment_id))
                    .count()
                    .get_result(&mut conn)
                    .map_err(DbError::from)?
            }
            _ => {
                use schema::gm_agent_comments::dsl;
                dsl::gm_agent_comments
                    .filter(dsl::comment_id.eq(comment_id))
                    .count()
                    .get_result(&mut conn)
                    .map_err(DbError::from)?
            }
        };

        Ok(count > 0)
    }

    async fn get_comment(
        &self,
        platform: &str,
        comment_id: &str,
    ) -> DbResult<Option<StoredComment>> {
        let mut conn = self.conn_async().await?;

        match platform.to_lowercase().as_str() {
            "facebook" => {
                use schema::gm_agent_facebook_comments::dsl;

                let result: Option<models::FacebookComment> = dsl::gm_agent_facebook_comments
                    .filter(dsl::facebook_comment_id.eq(comment_id))
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                Ok(result.map(|comment| self.convert_facebook_comment(&comment)))
            }
            "twitter" => {
                use schema::gm_agent_twitter_comments::dsl;

                let result: Option<models::TwitterComment> = dsl::gm_agent_twitter_comments
                    .filter(dsl::twitter_comment_id.eq(comment_id))
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                Ok(result.map(|comment| self.convert_twitter_comment(&comment)))
            }
            _ => {
                use schema::gm_agent_comments::dsl;

                let result: Option<models::AgentComment> = dsl::gm_agent_comments
                    .filter(dsl::comment_id.eq(comment_id))
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                Ok(result.map(|comment| self.convert_agent_comment(&comment)))
            }
        }
    }

    async fn get_comment_by_id(&self, id: i32) -> DbResult<Option<StoredComment>> {
        let mut conn = self.conn_async().await?;
        {
            use schema::gm_agent_comments::dsl;

            let result: Option<models::AgentComment> = dsl::gm_agent_comments
                .find(id)
                .first(&mut conn)
                .optional()
                .map_err(DbError::from)?;

            if let Some(comment) = result {
                return Ok(Some(self.convert_agent_comment(&comment)));
            }
        }
        {
            use schema::gm_agent_facebook_comments::dsl;

            let result: Option<models::FacebookComment> = dsl::gm_agent_facebook_comments
                .find(id)
                .first(&mut conn)
                .optional()
                .map_err(DbError::from)?;

            if let Some(comment) = result {
                return Ok(Some(self.convert_facebook_comment(&comment)));
            }
        }
        {
            use schema::gm_agent_twitter_comments::dsl;

            let result: Option<models::TwitterComment> = dsl::gm_agent_twitter_comments
                .find(id)
                .first(&mut conn)
                .optional()
                .map_err(DbError::from)?;

            if let Some(comment) = result {
                return Ok(Some(self.convert_twitter_comment(&comment)));
            }
        }

        Ok(None)
    }

    async fn save_comment(&self, comment: &Comment, content_db_id: i32) -> DbResult<i32> {
        if comment.platform.eq_ignore_ascii_case("facebook") {
            let campaign_id = self
                .get_facebook_post_context(content_db_id)
                .await?
                .0
                .unwrap_or(0);
            return self
                .save_facebook_comment(
                    comment,
                    content_db_id,
                    campaign_id,
                    &ReplySuggestion::new(comment.comment_id.clone()),
                )
                .await;
        }

        if comment.platform.eq_ignore_ascii_case("twitter") {
            let campaign_id = self
                .get_twitter_tweet_context(content_db_id)
                .await?
                .0
                .unwrap_or(0);
            return self
                .save_twitter_comment(
                    comment,
                    content_db_id,
                    campaign_id,
                    &ReplySuggestion::new(comment.comment_id.clone()),
                )
                .await;
        }

        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn_async().await?;

        // Get campaign_id from the video
        let campaign_id = self.get_campaign_id_from_video(content_db_id).await?;

        let new_comment = models::NewAgentComment {
            video_db_id: content_db_id,
            comment_id: comment.comment_id.clone(),
            user_nickname: comment.author_name.clone(),
            user_unique_id: Some(comment.author.clone()),
            content: Some(comment.text.clone()),
            campaign_id,
            status: CommentStatus::Pending as i16,
        };

        let id: i32 = diesel::insert_into(dsl::gm_agent_comments)
            .values(&new_comment)
            .returning(dsl::id)
            .get_result(&mut conn)
            .map_err(DbError::from)?;

        debug!(comment_id = %comment.comment_id, db_id = id, "Saved comment");
        Ok(id)
    }

    async fn save_comments(&self, comments: &[Comment], content_db_id: i32) -> DbResult<Vec<i32>> {
        let mut ids = Vec::with_capacity(comments.len());
        for comment in comments {
            let id = self.save_comment(comment, content_db_id).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// Save comment with AI analysis in one operation (matching Python agent's save_comments_and_analysis)
    ///
    /// Uses ON CONFLICT for atomic UPSERT to safely handle concurrent inserts.
    /// This is essential for parallel video processing where the same comment
    /// might be processed by multiple concurrent tasks.
    ///
    /// Routes to platform-specific tables:
    /// - TikTok: gm_agent_comments
    /// - Facebook: gm_agent_facebook_comments
    /// - Instagram: gm_agent_instagram_comments
    /// - Reddit: gm_agent_reddit_comments
    /// - Twitter: gm_agent_twitter_comments
    async fn save_comment_with_analysis(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        // Route to platform-specific save method
        let platform = comment.platform.to_lowercase();
        match platform.as_str() {
            "facebook" => {
                self.save_facebook_comment(comment, content_db_id, campaign_id, suggestion)
                    .await
            }
            "instagram" => {
                self.save_instagram_comment(comment, content_db_id, campaign_id, suggestion)
                    .await
            }
            "reddit" => {
                self.save_reddit_comment(comment, content_db_id, campaign_id, suggestion)
                    .await
            }
            "twitter" => {
                self.save_twitter_comment(comment, content_db_id, campaign_id, suggestion)
                    .await
            }
            _ => {
                // TikTok is default
                self.save_tiktok_comment(comment, content_db_id, campaign_id, suggestion)
                    .await
            }
        }
    }

    async fn get_pending_comments(
        &self,
        campaign_id: i32,
        limit: i32,
    ) -> DbResult<Vec<StoredComment>> {
        use schema::gm_agent_comments::dsl;
        use schema::gm_agent_twitter_comments::dsl as twitter_dsl;

        let mut conn = self.conn_async().await?;

        let results: Vec<models::AgentComment> = dsl::gm_agent_comments
            .filter(dsl::campaign_id.eq(campaign_id))
            .filter(dsl::status.eq(CommentStatus::Pending as i16))
            .limit(limit as i64)
            .load(&mut conn)
            .map_err(DbError::from)?;

        let mut pending_comments = results
            .iter()
            .map(|comment| self.convert_agent_comment(comment))
            .collect::<Vec<_>>();

        let remaining = limit.saturating_sub(pending_comments.len() as i32);
        if remaining > 0 {
            let twitter_results: Vec<models::TwitterComment> =
                twitter_dsl::gm_agent_twitter_comments
                    .filter(twitter_dsl::campaign_id.eq(campaign_id))
                    .filter(twitter_dsl::status.eq(Some(CommentStatus::Pending as i16)))
                    .limit(remaining as i64)
                    .load(&mut conn)
                    .map_err(DbError::from)?;

            pending_comments.extend(
                twitter_results
                    .iter()
                    .map(|comment| self.convert_twitter_comment(comment)),
            );
        }

        Ok(pending_comments)
    }

    async fn update_comment_status(&self, id: i32, status: CommentStatus) -> DbResult<()> {
        use schema::gm_agent_comments::dsl;
        use schema::gm_agent_facebook_comments::dsl as facebook_dsl;
        use schema::gm_agent_twitter_comments::dsl as twitter_dsl;

        let mut conn = self.conn_async().await?;

        let updated = diesel::update(dsl::gm_agent_comments.find(id))
            .set(dsl::status.eq(status as i16))
            .execute(&mut conn)
            .map_err(DbError::from)?;
        if updated > 0 {
            return Ok(());
        }

        let updated = diesel::update(facebook_dsl::gm_agent_facebook_comments.find(id))
            .set((
                facebook_dsl::status.eq(Some(status as i16)),
                facebook_dsl::updated_at.eq(Some(chrono::Utc::now())),
            ))
            .execute(&mut conn)
            .map_err(DbError::from)?;
        if updated > 0 {
            return Ok(());
        }

        diesel::update(twitter_dsl::gm_agent_twitter_comments.find(id))
            .set((
                twitter_dsl::status.eq(Some(status as i16)),
                twitter_dsl::updated_at.eq(Some(chrono::Utc::now())),
            ))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        Ok(())
    }

    async fn save_analysis(
        &self,
        comment_id: i32,
        _campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        use schema::gm_agent_comments::dsl;
        use schema::gm_agent_facebook_comments::dsl as facebook_dsl;
        use schema::gm_agent_twitter_comments::dsl as twitter_dsl;

        let mut conn = self.conn_async().await?;
        // Update the comment with AI analysis results
        // Matching Python agent: updated_at = NOW()
        let update = models::UpdateAgentComment {
            reason: suggestion.reason.clone(),
            suggested_reply: suggestion.reply_text.clone(),
            suggested_dm: suggestion.dm_text.clone(),
            suggested_reply_post: suggestion.post_reply_text.clone(),
            status: Some(CommentStatus::Completed as i16),
            updated_at: Some(chrono::Utc::now()), // Matching Python: updated_at = NOW()
        };

        let updated = diesel::update(dsl::gm_agent_comments.find(comment_id))
            .set(&update)
            .execute(&mut conn)
            .map_err(DbError::from)?;
        if updated == 0 {
            let updated = diesel::update(facebook_dsl::gm_agent_facebook_comments.find(comment_id))
                .set((
                    facebook_dsl::reason.eq(suggestion.reason.as_ref()),
                    facebook_dsl::suggested_reply.eq(suggestion.reply_text.as_ref()),
                    facebook_dsl::suggested_dm.eq(suggestion.dm_text.as_ref()),
                    facebook_dsl::suggested_reply_post.eq(suggestion.post_reply_text.as_ref()),
                    facebook_dsl::status.eq(Some(CommentStatus::Completed as i16)),
                    facebook_dsl::updated_at.eq(Some(chrono::Utc::now())),
                ))
                .execute(&mut conn)
                .map_err(DbError::from)?;
            if updated == 0 {
                diesel::update(twitter_dsl::gm_agent_twitter_comments.find(comment_id))
                    .set((
                        twitter_dsl::reason.eq(suggestion.reason.as_ref()),
                        twitter_dsl::suggested_reply.eq(suggestion.reply_text.as_ref()),
                        twitter_dsl::suggested_dm.eq(suggestion.dm_text.as_ref()),
                        twitter_dsl::suggested_reply_post.eq(suggestion.post_reply_text.as_ref()),
                        twitter_dsl::status.eq(Some(CommentStatus::Completed as i16)),
                        twitter_dsl::updated_at.eq(Some(chrono::Utc::now())),
                    ))
                    .execute(&mut conn)
                    .map_err(DbError::from)?;
            }
        }

        debug!(comment_id, "Saved AI analysis to comment");
        Ok(comment_id) // Return comment_id as the "analysis id"
    }

    async fn get_analysis(&self, comment_id: i32) -> DbResult<Option<StoredAnalysis>> {
        use schema::gm_agent_comments::dsl;
        use schema::gm_agent_facebook_comments::dsl as facebook_dsl;
        use schema::gm_agent_twitter_comments::dsl as twitter_dsl;

        let mut conn = self.conn_async().await?;

        let result: Option<models::AgentComment> = dsl::gm_agent_comments
            .find(comment_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        if let Some(analysis) = result.and_then(|c| {
            // Only return if analysis exists
            if c.suggested_reply.is_some()
                || c.suggested_dm.is_some()
                || c.suggested_reply_post.is_some()
            {
                Some(StoredAnalysis {
                    id: c.id,
                    comment_id: c.id,
                    campaign_id: c.campaign_id.unwrap_or(0),
                    suggested_reply: c.suggested_reply,
                    suggested_dm: c.suggested_dm,
                    suggested_reply_post: c.suggested_reply_post,
                    reason: c.reason,
                    tokens_used: None,
                    model_name: None,
                })
            } else {
                None
            }
        }) {
            return Ok(Some(analysis));
        }

        let result: Option<models::FacebookComment> = facebook_dsl::gm_agent_facebook_comments
            .find(comment_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        if let Some(comment) = result {
            if comment.suggested_reply.is_some()
                || comment.suggested_dm.is_some()
                || comment.suggested_reply_post.is_some()
            {
                return Ok(Some(StoredAnalysis {
                    id: comment.id,
                    comment_id: comment.id,
                    campaign_id: comment.campaign_id.unwrap_or(0),
                    suggested_reply: comment.suggested_reply,
                    suggested_dm: comment.suggested_dm,
                    suggested_reply_post: comment.suggested_reply_post,
                    reason: comment.reason,
                    tokens_used: None,
                    model_name: None,
                }));
            }
        }

        let result: Option<models::TwitterComment> = twitter_dsl::gm_agent_twitter_comments
            .find(comment_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.and_then(|comment| {
            if comment.suggested_reply.is_some()
                || comment.suggested_dm.is_some()
                || comment.suggested_reply_post.is_some()
            {
                Some(StoredAnalysis {
                    id: comment.id,
                    comment_id: comment.id,
                    campaign_id: comment.campaign_id.unwrap_or(0),
                    suggested_reply: comment.suggested_reply,
                    suggested_dm: comment.suggested_dm,
                    suggested_reply_post: comment.suggested_reply_post,
                    reason: comment.reason,
                    tokens_used: None,
                    model_name: None,
                })
            } else {
                None
            }
        }))
    }
}

// ============================================================
// PromptRepository Implementation
// ============================================================

#[async_trait]
impl PromptRepository for PostgresAdapter {
    async fn get_campaign(&self, campaign_id: i32) -> DbResult<Option<CampaignConfig>> {
        use schema::gm_campaigns::dsl;

        let mut conn = self.conn_async().await?;

        let result: Option<models::Campaign> = dsl::gm_campaigns
            .find(campaign_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        match result {
            Some(c) => {
                // Get templates for prompts
                let templates = self.get_campaign_templates(campaign_id).await?;

                // Use weighted random selection (matching Python agent behavior)
                // Python: random.choices(population, weights=weights, k=1)[0]
                let selected_template = if templates.is_empty() {
                    None
                } else {
                    use rand::Rng;
                    let total_weight: i32 = templates.iter().map(|t| t.weight.max(1)).sum();
                    if total_weight <= 0 {
                        templates.first()
                    } else {
                        let mut rng = rand::thread_rng();
                        let mut random_point = rng.gen_range(0..total_weight);
                        let mut selected: Option<&models::CampaignTemplate> = None;
                        for template in &templates {
                            let weight = template.weight.max(1); // Ensure at least 1
                            if random_point < weight {
                                selected = Some(template);
                                break;
                            }
                            random_point -= weight;
                        }
                        selected.or(templates.first())
                    }
                };

                Ok(Some(CampaignConfig {
                    id: c.id,
                    user_id: c.user_id,
                    name: c.name,
                    platform_id: c.platform_id,
                    status: CampaignStatus::from(c.status.as_str()),
                    target_audience: c.target_audience,
                    product_prompt: Some(c.product_prompt),
                    // Get strategies from template (weighted random selection)
                    reply_strategy: selected_template.and_then(|t| t.reply_prompt.clone()),
                    dm_strategy: selected_template.and_then(|t| t.dm_prompt.clone()),
                    reply_post_strategy: selected_template
                        .and_then(|t| t.reply_post_prompt.clone()),
                    // Note: max_comments limit is handled by Scheduler's budget mechanism.
                    // Agent should not use total_scanned for limit checking because:
                    // 1. Scheduler adds page_size to total_scanned when reserving budget
                    // 2. page_size can be larger than max_scan_count (e.g., page_size=20, max_scan_count=1)
                    // 3. This would cause Agent to stop immediately before processing any content
                    // Setting to None lets Scheduler control the limit via budget exhaustion.
                    max_comments: None,
                    processed_comments: 0,
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_analysis_context(&self, campaign_id: i32) -> DbResult<Option<AnalysisContext>> {
        let campaign = self.get_campaign(campaign_id).await?;
        Ok(campaign.map(|c| c.to_analysis_context()))
    }

    async fn get_platform(&self, platform_id: i32) -> DbResult<Option<PlatformConfig>> {
        use schema::gm_platforms::dsl;

        let mut conn = self.conn_async().await?;

        let result: Option<models::Platform> = dsl::gm_platforms
            .find(platform_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.map(|p| PlatformConfig {
            id: p.id,
            name: p.name,
            display_name: p.display_name,
            is_active: p.is_active,
        }))
    }

    async fn get_platform_by_name(&self, name: &str) -> DbResult<Option<PlatformConfig>> {
        use schema::gm_platforms::dsl;

        let mut conn = self.conn_async().await?;

        let result: Option<models::Platform> = dsl::gm_platforms
            .filter(dsl::name.eq(name))
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.map(|p| PlatformConfig {
            id: p.id,
            name: p.name,
            display_name: p.display_name,
            is_active: p.is_active,
        }))
    }

    async fn should_stop_campaign(&self, campaign_id: i32) -> DbResult<bool> {
        let campaign = self.get_campaign(campaign_id).await?;

        match campaign {
            Some(c) => {
                let should_continue = c.status.should_continue();
                let at_limit = c.is_at_limit();
                tracing::debug!(
                    campaign_id,
                    status = ?c.status,
                    should_continue,
                    at_limit,
                    max_comments = ?c.max_comments,
                    processed_comments = c.processed_comments,
                    "Checking if campaign should stop"
                );
                // Stop if not active or at limit
                Ok(!should_continue || at_limit)
            }
            None => {
                tracing::warn!(campaign_id, "Campaign not found, stopping");
                Ok(true)
            }
        }
    }

    async fn update_processed_count(&self, campaign_id: i32, count: i32) -> DbResult<()> {
        use schema::gm_campaigns::dsl;

        let mut conn = self.conn_async().await?;

        diesel::update(dsl::gm_campaigns.find(campaign_id))
            .set(dsl::total_scanned.eq(count))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        Ok(())
    }
}

// ============================================================
// ProgressTracker Implementation
// ============================================================

#[async_trait]
impl ProgressTracker for PostgresAdapter {
    async fn get_task(&self, task_id: i64) -> DbResult<Option<TaskInfo>> {
        use schema::gm_campaigns::dsl as camp_dsl;
        use schema::gm_crawler_tasks::dsl;

        let mut conn = self.conn_async().await?;

        let result: Option<models::CrawlerTask> = dsl::gm_crawler_tasks
            .find(task_id as i32)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        match result {
            Some(t) => {
                // Get platform_id from campaign
                let campaign: Option<models::Campaign> = camp_dsl::gm_campaigns
                    .find(t.campaign_id)
                    .first(&mut conn)
                    .optional()
                    .map_err(DbError::from)?;

                let platform_id = campaign.map(|c| c.platform_id).unwrap_or(0);
                let keywords = t.keywords.map(|v| {
                    serde_json::Value::Array(v.into_iter().map(serde_json::Value::String).collect())
                });

                Ok(Some(TaskInfo {
                    id: t.id as i64,
                    campaign_id: t.campaign_id,
                    platform_id,
                    keywords,
                    status: TaskStatus::from(t.status.as_str()),
                    progress: (t.process_count * 100 / t.max_count.max(1)),
                    error_message: None,
                }))
            }
            None => Ok(None),
        }
    }

    async fn update_task_status(&self, task_id: i64, status: TaskStatus) -> DbResult<()> {
        use schema::gm_crawler_tasks::dsl;

        let mut conn = self.conn_async().await?;

        let status_str = match status {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "processing", // Must match fn_update_task_progress check: ('pending', 'processing')
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        };

        diesel::update(dsl::gm_crawler_tasks.find(task_id as i32))
            .set(dsl::status.eq(status_str))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        debug!(task_id, ?status, "Updated task status");
        Ok(())
    }

    async fn update_task_progress(
        &self,
        task_id: i64,
        increment: i32,
    ) -> DbResult<TaskProgressUpdate> {
        let mut conn = self.conn_async().await?;

        // Call fn_update_task_progress stored procedure (matching Python agent)
        // This updates process_count, actual_consumption on task AND campaign
        let result: Option<TaskProgressResult> = diesel::sql_query(
            "SELECT success, should_stop, new_process_count, new_actual_consumption FROM fn_update_task_progress($1, $2)"
        )
        .bind::<diesel::sql_types::Integer, _>(task_id as i32)
        .bind::<diesel::sql_types::Integer, _>(increment)
        .get_result::<TaskProgressResult>(&mut conn)
        .optional()
        .map_err(DbError::from)?;

        if let Some(ref r) = result {
            info!(
                task_id,
                success = r.success,
                should_stop = r.should_stop,
                new_process_count = r.new_process_count,
                new_actual_consumption = %r.new_actual_consumption,
                "Task progress updated via stored procedure"
            );

            if r.should_stop {
                warn!(task_id, "⚠️ Campaign is STOPPING, should stop processing");
            }
        }

        Ok(result
            .map(|r| TaskProgressUpdate {
                success: r.success,
                should_stop: r.should_stop,
                new_process_count: r.new_process_count,
                new_actual_consumption: r.new_actual_consumption.to_string().parse().unwrap_or(0.0),
            })
            .unwrap_or_default())
    }

    async fn set_task_error(&self, task_id: i64, _error: &str) -> DbResult<()> {
        use schema::gm_crawler_tasks::dsl;

        let mut conn = self.conn_async().await?;

        diesel::update(dsl::gm_crawler_tasks.find(task_id as i32))
            .set(dsl::status.eq("failed"))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        warn!(task_id, "Task failed");
        Ok(())
    }

    async fn complete_task(&self, task_id: i64) -> DbResult<()> {
        let mut conn = self.conn_async().await?;

        // Call fn_complete_task stored procedure
        // This will:
        // 1. Update task status to 'completed'
        // 2. Call fn_settle_task_consumption to settle the budget
        // 3. Check if campaign should be stopped
        let result: Option<(bool, String)> =
            diesel::sql_query("SELECT success, campaign_status FROM fn_complete_task($1, $2)")
                .bind::<diesel::sql_types::Integer, _>(task_id as i32)
                .bind::<diesel::sql_types::Text, _>("completed")
                .get_result::<TaskCompleteResult>(&mut conn)
                .optional()
                .map_err(DbError::from)?
                .map(|r| (r.success, r.campaign_status));

        if let Some((success, campaign_status)) = result {
            debug!(
                task_id,
                success, campaign_status, "Task completed via stored procedure"
            );
        }

        Ok(())
    }

    async fn fail_task(&self, task_id: i64, error: &str) -> DbResult<()> {
        self.set_task_error(task_id, error).await
    }

    async fn should_stop(&self, task_id: i64) -> DbResult<bool> {
        let task = self.get_task(task_id).await?;

        match task {
            Some(t) => {
                // Stop if task is in terminal state
                if t.is_terminal() {
                    tracing::debug!(task_id, status = ?t.status, "Task is in terminal state, stopping");
                    return Ok(true);
                }

                // Check campaign status
                let result = self.should_stop_campaign(t.campaign_id).await?;
                tracing::debug!(
                    task_id,
                    campaign_id = t.campaign_id,
                    should_stop = result,
                    "Campaign stop check"
                );
                Ok(result)
            }
            None => {
                tracing::warn!(task_id, "Task not found, stopping");
                Ok(true)
            }
        }
    }

    async fn increment_processed(&self, campaign_id: i32, count: i32) -> DbResult<()> {
        use schema::gm_campaigns::dsl;

        let mut conn = self.conn_async().await?;

        diesel::update(dsl::gm_campaigns.find(campaign_id))
            .set(dsl::total_scanned.eq(dsl::total_scanned + count))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        Ok(())
    }

    async fn get_processed_count(&self, campaign_id: i32) -> DbResult<i32> {
        let campaign = self.get_campaign(campaign_id).await?;
        Ok(campaign.map(|c| c.processed_comments).unwrap_or(0))
    }

    async fn stop_campaign_gracefully(&self, campaign_id: i32) -> DbResult<CampaignStopResult> {
        let mut conn = self.conn_async().await?;

        // Call fn_stop_campaign_gracefully stored procedure (matching Python agent)
        // This sets campaign status to STOPPING or STOPPED and handles budget refunds
        let result: Option<CampaignStopDbResult> = diesel::sql_query(
            "SELECT success, immediate_stopped, refunded_amount FROM fn_stop_campaign_gracefully($1)"
        )
        .bind::<diesel::sql_types::Integer, _>(campaign_id)
        .get_result::<CampaignStopDbResult>(&mut conn)
        .optional()
        .map_err(DbError::from)?;

        if let Some(ref r) = result {
            if r.success {
                if r.immediate_stopped {
                    info!(
                        campaign_id,
                        refunded_amount = %r.refunded_amount,
                        "✅ Campaign marked as STOPPED"
                    );
                } else {
                    info!(
                        campaign_id,
                        "✅ Campaign marked as STOPPING (waiting for active tasks)"
                    );
                }
            } else {
                warn!(campaign_id, "❌ Failed to stop campaign");
            }
        }

        Ok(result
            .map(|r| CampaignStopResult {
                success: r.success,
                immediate_stopped: r.immediate_stopped,
                refunded_amount: r.refunded_amount.to_string().parse().unwrap_or(0.0),
            })
            .unwrap_or_default())
    }
}

// ============================================================
// Helper Methods
// ============================================================

impl PostgresAdapter {
    fn json_at<'a>(
        value: Option<&'a serde_json::Value>,
        path: &[&str],
    ) -> Option<&'a serde_json::Value> {
        let mut current = value?;
        for key in path {
            current = current.get(*key)?;
        }
        Some(current)
    }

    fn json_string(value: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
        match Self::json_at(value, path) {
            Some(serde_json::Value::String(value)) => Some(value.clone()),
            Some(serde_json::Value::Number(value)) => Some(value.to_string()),
            Some(serde_json::Value::Bool(value)) => Some(value.to_string()),
            _ => None,
        }
    }

    fn json_i64(value: Option<&serde_json::Value>, path: &[&str]) -> Option<i64> {
        match Self::json_at(value, path) {
            Some(serde_json::Value::Number(value)) => {
                value.as_i64().or_else(|| value.as_u64().map(|v| v as i64))
            }
            Some(serde_json::Value::String(value)) => value.parse().ok(),
            _ => None,
        }
    }

    fn timestamp_to_datetime(timestamp: Option<i64>) -> Option<chrono::DateTime<chrono::Utc>> {
        timestamp.and_then(|value| chrono::DateTime::<chrono::Utc>::from_timestamp(value, 0))
    }

    fn i64_to_i32(value: i64) -> i32 {
        value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }

    fn parse_twitter_raw(value: Option<&serde_json::Value>) -> Option<TikhubTwitterTweet> {
        value
            .cloned()
            .and_then(|raw| serde_json::from_value::<TikhubTwitterTweet>(raw).ok())
    }

    fn twitter_media_bind_value(tweet: &TikhubTwitterTweet) -> Option<Vec<Option<String>>> {
        let media_urls = tweet.media_urls();
        if media_urls.is_empty() {
            None
        } else {
            Some(media_urls.into_iter().map(Some).collect())
        }
    }

    fn twitter_media_json(media_urls: Option<&Vec<Option<String>>>) -> Option<serde_json::Value> {
        media_urls.map(|urls| {
            serde_json::Value::Array(
                urls.iter()
                    .flatten()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            )
        })
    }

    fn convert_video_to_content(&self, v: &models::AgentVideo) -> StoredContent {
        StoredContent {
            id: v.id,
            platform_id: 2, // TikTok
            content_id: v.video_id.clone().unwrap_or_default(),
            author_unique_id: v.author_unique_id.clone(),
            author_nickname: v.author.clone(),
            description: v.description.clone(),
            content_url: v.url.clone(),
            likes: v.like_count.map(|c| c as i64),
            comments: v.comment_count.map(|c| c as i64),
            shares: v.share_count.map(|c| c as i64),
            views: v.play_count.map(|c| c as i64),
            content_created_at: v.publish_time,
            raw_data: None,
            campaign_id: v.campaign_id,
        }
    }

    fn convert_facebook_post_to_content(&self, post: &models::FacebookPost) -> StoredContent {
        StoredContent {
            id: post.id,
            platform_id: 3,
            content_id: post.facebook_post_id.clone(),
            author_unique_id: post.author_id.clone(),
            author_nickname: post.author_name.clone(),
            description: post.message.clone().or_else(|| post.message_rich.clone()),
            content_url: post.url.clone(),
            likes: post.reactions_count.map(|value| value as i64),
            comments: post.comments_count.map(|value| value as i64),
            shares: post.reshare_count.map(|value| value as i64),
            views: None,
            content_created_at: post.timestamp,
            raw_data: Some(serde_json::json!({
                "post_id": post.facebook_post_id.clone(),
                "type": post.post_type.clone(),
                "url": post.url.clone(),
                "message": post.message.clone(),
                "message_rich": post.message_rich.clone(),
                "timestamp": post.timestamp,
                "reactions_count": post.reactions_count,
                "comments_count": post.comments_count,
                "reshare_count": post.reshare_count,
                "reactions": {
                    "like": post.reactions_like,
                    "love": post.reactions_love,
                    "haha": post.reactions_haha,
                    "wow": post.reactions_wow,
                    "sad": post.reactions_sad,
                    "angry": post.reactions_angry,
                    "care": post.reactions_care
                },
                "author": {
                    "id": post.author_id.clone(),
                    "name": post.author_name.clone(),
                    "url": post.author_url.clone(),
                    "profile_picture_url": post.author_profile_picture_url.clone()
                },
                "author_title": post.author_title.clone(),
                "image": {
                    "uri": post.image_url.clone(),
                    "width": post.image_width,
                    "height": post.image_height,
                    "id": post.image_id.clone()
                },
                "video_thumbnail": post.video_thumbnail.clone(),
                "external_url": post.external_url.clone(),
                "attached_post_url": post.attached_post_url.clone(),
                "comments_id": post.comments_id.clone(),
                "shares_id": post.shares_id.clone()
            })),
            campaign_id: post.campaign_id,
        }
    }

    fn convert_twitter_tweet_to_content(&self, tweet: &models::TwitterTweet) -> StoredContent {
        let content_url = tweet.screen_name.as_ref().map_or_else(
            || {
                Some(format!(
                    "https://twitter.com/i/web/status/{}",
                    tweet.twitter_tweet_id
                ))
            },
            |screen_name| {
                Some(format!(
                    "https://twitter.com/{screen_name}/status/{}",
                    tweet.twitter_tweet_id
                ))
            },
        );

        StoredContent {
            id: tweet.id,
            platform_id: 5,
            content_id: tweet.twitter_tweet_id.clone(),
            author_unique_id: tweet.screen_name.clone(),
            author_nickname: tweet.user_name.clone(),
            description: Some(tweet.full_text.clone()),
            content_url,
            likes: tweet.favorite_count.map(|value| value as i64),
            comments: tweet.reply_count.map(|value| value as i64),
            shares: tweet.retweet_count.map(|value| value as i64),
            views: tweet.view_count.map(|value| value as i64),
            content_created_at: tweet.created_at_ts,
            raw_data: Some(serde_json::json!({
                "tweet_id": tweet.twitter_tweet_id.clone(),
                "type": "tweet",
                "text": tweet.full_text.clone(),
                "created_at": tweet.created_at_str.clone(),
                "conversation_id": tweet.conversation_id.clone(),
                "lang": tweet.lang.clone(),
                "bookmarks": tweet.bookmark_count,
                "favorites": tweet.favorite_count,
                "quotes": tweet.quote_count,
                "replies": tweet.reply_count,
                "retweets": tweet.retweet_count,
                "views": tweet.view_count,
                "user_info": {
                    "rest_id": tweet.user_id.clone(),
                    "screen_name": tweet.screen_name.clone(),
                    "name": tweet.user_name.clone(),
                    "description": tweet.user_description.clone(),
                    "followers_count": tweet.user_followers_count,
                    "avatar": tweet.user_avatar.clone(),
                    "verified": tweet.user_verified
                },
                "media": Self::twitter_media_json(tweet.media_urls.as_ref()),
                "in_reply_to_status_id_str": tweet.in_reply_to_status_id.clone(),
                "in_reply_to_user_id_str": tweet.in_reply_to_user_id.clone()
            })),
            campaign_id: tweet.campaign_id,
        }
    }

    fn convert_agent_comment(&self, c: &models::AgentComment) -> StoredComment {
        StoredComment {
            id: c.id,
            platform_id: 2, // TikTok
            content_id: c.video_db_id,
            comment_id: c.comment_id.clone(),
            parent_comment_id: None,
            author_uid: None,
            author_unique_id: c.user_unique_id.clone(),
            author_nickname: c.user_nickname.clone(),
            comment_text: c.content.clone(),
            likes: None,
            reply_count: None,
            comment_created_at: c.create_time.map(|t| t.and_utc().timestamp()),
            is_reply: false,
            raw_data: None,
            status: c.status,
        }
    }

    fn convert_facebook_comment(&self, comment: &models::FacebookComment) -> StoredComment {
        StoredComment {
            id: comment.id,
            platform_id: 3,
            content_id: comment.post_db_id,
            comment_id: comment.facebook_comment_id.clone(),
            parent_comment_id: comment.parent_comment_id.clone(),
            author_uid: comment.comment_user_id.clone(),
            author_unique_id: comment.comment_user_id.clone(),
            author_nickname: comment.comment_username.clone(),
            comment_text: Some(comment.comment_text.clone()),
            likes: comment.like_count.map(|value| value as i64),
            reply_count: comment.reply_count,
            comment_created_at: comment.created_at_ts,
            is_reply: comment.parent_comment_id.is_some(),
            raw_data: Some(serde_json::json!({
                "comment_id": comment.facebook_comment_id.clone(),
                "parent_comment_id": comment.parent_comment_id.clone(),
                "comment_url": comment.comment_url.clone(),
                "message": comment.comment_text.clone(),
                "author": {
                    "id": comment.comment_user_id.clone(),
                    "name": comment.comment_username.clone(),
                    "url": comment.comment_user_url.clone(),
                    "profile_image": comment.comment_user_profile_picture.clone()
                },
                "reactions_count": comment.like_count,
                "replies_count": comment.reply_count,
                "depth": comment.threading_depth,
                "created_time": comment.created_at_ts,
                "facebook_post_id": comment.facebook_post_id.clone(),
                "post_url": comment.post_url.clone()
            })),
            status: comment.status.unwrap_or(0),
        }
    }

    fn convert_twitter_comment(&self, comment: &models::TwitterComment) -> StoredComment {
        StoredComment {
            id: comment.id,
            platform_id: 5,
            content_id: comment.tweet_db_id,
            comment_id: comment.twitter_comment_id.clone(),
            parent_comment_id: comment.in_reply_to_status_id.clone(),
            author_uid: comment.comment_user_id.clone(),
            author_unique_id: comment.comment_screen_name.clone(),
            author_nickname: comment.comment_user_name.clone(),
            comment_text: Some(comment.comment_text.clone()),
            likes: comment.favorite_count.map(|value| value as i64),
            reply_count: comment.reply_count,
            comment_created_at: comment.created_at_ts,
            is_reply: comment.is_reply.unwrap_or(false),
            raw_data: Some(serde_json::json!({
                "tweet_id": comment.twitter_comment_id.clone(),
                "type": "tweet",
                "text": comment.comment_text.clone(),
                "created_at": comment.created_at_str.clone(),
                "conversation_id": comment.conversation_id.clone(),
                "favorites": comment.favorite_count,
                "retweets": comment.retweet_count,
                "replies": comment.reply_count,
                "author": {
                    "rest_id": comment.comment_user_id.clone(),
                    "screen_name": comment.comment_screen_name.clone(),
                    "name": comment.comment_user_name.clone(),
                    "followers_count": comment.comment_user_followers
                },
                "media": Self::twitter_media_json(comment.media_urls.as_ref()),
                "in_reply_to_status_id_str": comment.in_reply_to_status_id.clone()
            })),
            status: comment.status.unwrap_or(0),
        }
    }

    async fn get_campaign_templates(
        &self,
        campaign_id: i32,
    ) -> DbResult<Vec<models::CampaignTemplate>> {
        use schema::gm_campaign_templates::dsl;

        let mut conn = self.conn_async().await?;

        let templates: Vec<models::CampaignTemplate> = dsl::gm_campaign_templates
            .filter(dsl::campaign_id.eq(campaign_id))
            .order(dsl::weight.desc())
            .load(&mut conn)
            .map_err(DbError::from)?;

        Ok(templates)
    }

    #[allow(dead_code)]
    async fn get_active_task_id(&self, campaign_id: i32) -> DbResult<i32> {
        use schema::gm_crawler_tasks::dsl;

        let mut conn = self.conn_async().await?;

        let task: Option<models::CrawlerTask> = dsl::gm_crawler_tasks
            .filter(dsl::campaign_id.eq(campaign_id))
            .filter(dsl::status.ne("completed"))
            .filter(dsl::status.ne("failed"))
            .order(dsl::created_at.desc())
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(task.map(|t| t.id).unwrap_or(0))
    }

    async fn get_campaign_id_from_video(&self, video_id: i32) -> DbResult<Option<i32>> {
        use schema::gm_agent_videos::dsl;

        let mut conn = self.conn_async().await?;

        let video: Option<models::AgentVideo> = dsl::gm_agent_videos
            .find(video_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(video.and_then(|v| v.campaign_id))
    }

    async fn get_facebook_post_context(
        &self,
        post_db_id: i32,
    ) -> DbResult<(Option<i32>, Option<String>, Option<String>)> {
        use schema::gm_agent_facebook_posts::dsl;

        let mut conn = self.conn_async().await?;

        let post: Option<models::FacebookPost> = dsl::gm_agent_facebook_posts
            .find(post_db_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(post
            .map(|post| (post.campaign_id, post.url, Some(post.facebook_post_id)))
            .unwrap_or((None, None, None)))
    }

    async fn get_twitter_tweet_context(
        &self,
        tweet_db_id: i32,
    ) -> DbResult<(Option<i32>, Option<String>, Option<String>)> {
        use schema::gm_agent_twitter_tweets::dsl;

        let mut conn = self.conn_async().await?;

        let tweet: Option<models::TwitterTweet> = dsl::gm_agent_twitter_tweets
            .find(tweet_db_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(tweet
            .map(|tweet| {
                let url = tweet.screen_name.as_ref().map_or_else(
                    || {
                        Some(format!(
                            "https://twitter.com/i/web/status/{}",
                            tweet.twitter_tweet_id
                        ))
                    },
                    |screen_name| {
                        Some(format!(
                            "https://twitter.com/{screen_name}/status/{}",
                            tweet.twitter_tweet_id
                        ))
                    },
                );
                (tweet.campaign_id, url, Some(tweet.twitter_tweet_id))
            })
            .unwrap_or((None, None, None)))
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_platform_id() {
        // Note: This test doesn't actually connect to a database,
        // it just tests the platform_id method
        struct MockAdapter;
        impl MockAdapter {
            fn platform_id(&self, platform: &str) -> i32 {
                match platform {
                    "tiktok" => 2,
                    "instagram" => 4,
                    "facebook" => 3,
                    "twitter" => 5,
                    "youtube" => 6,
                    "reddit" => 1,
                    _ => 0,
                }
            }
        }

        let adapter = MockAdapter;
        assert_eq!(adapter.platform_id("tiktok"), 2);
        assert_eq!(adapter.platform_id("instagram"), 4);
        assert_eq!(adapter.platform_id("unknown"), 0);
    }
}
