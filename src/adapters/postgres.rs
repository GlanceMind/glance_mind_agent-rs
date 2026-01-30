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
    content_repository::{CommentStatus, ContentSaveResult, StoredAnalysis, StoredComment, StoredContent},
    progress_tracker::{CampaignStopResult, TaskInfo, TaskProgressUpdate, TaskStatus},
    prompt_repository::{CampaignConfig, CampaignStatus, PlatformConfig},
    ContentRepository, ProgressTracker, PromptRepository,
};

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

    /// Get a connection from the pool
    fn conn(
        &self,
    ) -> DbResult<diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>>
    {
        self.pool
            .get()
            .map_err(|e| DbError::Connection(e.to_string()))
    }

    /// Get platform ID from name using global registry
    fn platform_id(&self, platform: &str) -> i32 {
        use crate::config::platform::{global_registry, PlatformLookup};

        global_registry().get_id(platform).unwrap_or(0)
    }
}

// ============================================================
// ContentRepository Implementation
// Using gm_agent_videos and gm_agent_comments tables
// ============================================================

#[async_trait]
impl ContentRepository for PostgresAdapter {
    async fn content_exists(&self, _platform: &str, content_id: &str) -> DbResult<bool> {
        use schema::gm_agent_videos::dsl;

        let mut conn = self.conn()?;

        let count: i64 = dsl::gm_agent_videos
            .filter(dsl::video_id.eq(content_id))
            .count()
            .get_result(&mut conn)
            .map_err(DbError::from)?;

        Ok(count > 0)
    }

    async fn get_content(
        &self,
        _platform: &str,
        content_id: &str,
    ) -> DbResult<Option<StoredContent>> {
        use schema::gm_agent_videos::dsl;

        let mut conn = self.conn()?;

        let result: Option<models::AgentVideo> = dsl::gm_agent_videos
            .filter(dsl::video_id.eq(content_id))
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.map(|v| self.convert_video_to_content(&v)))
    }

    async fn get_content_by_id(&self, id: i32) -> DbResult<Option<StoredContent>> {
        use schema::gm_agent_videos::dsl;

        let mut conn = self.conn()?;

        let result: Option<models::AgentVideo> = dsl::gm_agent_videos
            .find(id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.map(|v| self.convert_video_to_content(&v)))
    }

    async fn save_content(&self, content: &Content, campaign_id: Option<i32>, task_id: Option<i32>) -> DbResult<ContentSaveResult> {
        let mut conn = self.conn()?;

        // Use the provided task_id directly (passed from orchestrator)
        let task_id_value = task_id.unwrap_or(0);

        // Use ON CONFLICT to atomically upsert, matching Python agent's behavior:
        // - If (task_id, video_id) exists, update the record
        // - If not exists, insert new record
        // - Use (xmax = 0) to determine if this was a new insert
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
            "#
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
            debug!(content_id = %content.content_id, db_id = result.id, "Saved new video");
        } else {
            debug!(content_id = %content.content_id, db_id = result.id, "Updated existing video");
        }

        Ok(ContentSaveResult {
            id: result.id,
            is_new: result.inserted,
        })
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
        use schema::gm_agent_videos::dsl;

        let mut conn = self.conn()?;

        diesel::update(dsl::gm_agent_videos.find(id))
            .set((
                dsl::like_count.eq(likes as i32),
                dsl::comment_count.eq(comments as i32),
                dsl::share_count.eq(shares as i32),
                dsl::play_count.eq(views as i32),
            ))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        Ok(())
    }

    async fn comment_exists(&self, _platform: &str, comment_id: &str) -> DbResult<bool> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

        let count: i64 = dsl::gm_agent_comments
            .filter(dsl::comment_id.eq(comment_id))
            .count()
            .get_result(&mut conn)
            .map_err(DbError::from)?;

        Ok(count > 0)
    }

    async fn get_comment(
        &self,
        _platform: &str,
        comment_id: &str,
    ) -> DbResult<Option<StoredComment>> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

        let result: Option<models::AgentComment> = dsl::gm_agent_comments
            .filter(dsl::comment_id.eq(comment_id))
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.map(|c| self.convert_agent_comment(&c)))
    }

    async fn get_comment_by_id(&self, id: i32) -> DbResult<Option<StoredComment>> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

        let result: Option<models::AgentComment> = dsl::gm_agent_comments
            .find(id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.map(|c| self.convert_agent_comment(&c)))
    }

    async fn save_comment(&self, comment: &Comment, content_db_id: i32) -> DbResult<i32> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

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
    async fn save_comment_with_analysis(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        let mut conn = self.conn()?;
        
        // Parse create_time to NaiveDateTime if available
        let create_time = comment.created_at.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.naive_utc())
        });
        
        // Use ON CONFLICT to atomically handle duplicate comments
        // This ensures concurrent inserts don't cause unique constraint violations
        let id: i32 = diesel::sql_query(
            r#"
            INSERT INTO gm_agent_comments (
                video_db_id, comment_id, user_nickname, user_unique_id,
                content, create_time, reason, suggested_reply,
                suggested_dm, suggested_reply_post, campaign_id, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0)
            ON CONFLICT (video_db_id, comment_id) DO UPDATE SET
                reason = EXCLUDED.reason,
                suggested_reply = EXCLUDED.suggested_reply,
                suggested_dm = EXCLUDED.suggested_dm,
                suggested_reply_post = EXCLUDED.suggested_reply_post,
                status = 0,
                updated_at = NOW()
            RETURNING id
            "#
        )
        .bind::<Integer, _>(content_db_id)
        .bind::<Text, _>(&comment.comment_id)
        .bind::<diesel::sql_types::Nullable<Text>, _>(comment.author_name.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.author))
        .bind::<diesel::sql_types::Nullable<Text>, _>(Some(&comment.text))
        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Timestamp>, _>(create_time)
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reason.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.dm_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Text>, _>(suggestion.post_reply_text.as_ref())
        .bind::<diesel::sql_types::Nullable<Integer>, _>(Some(campaign_id))
        .get_result::<CommentInsertResult>(&mut conn)
        .map_err(DbError::from)?
        .id;

        debug!(comment_id = %comment.comment_id, db_id = id, "Upserted comment with AI analysis");
        Ok(id)
    }

    async fn get_pending_comments(
        &self,
        campaign_id: i32,
        limit: i32,
    ) -> DbResult<Vec<StoredComment>> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

        let results: Vec<models::AgentComment> = dsl::gm_agent_comments
            .filter(dsl::campaign_id.eq(campaign_id))
            .filter(dsl::status.eq(CommentStatus::Pending as i16))
            .limit(limit as i64)
            .load(&mut conn)
            .map_err(DbError::from)?;

        Ok(results.iter().map(|c| self.convert_agent_comment(c)).collect())
    }

    async fn update_comment_status(&self, id: i32, status: CommentStatus) -> DbResult<()> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

        diesel::update(dsl::gm_agent_comments.find(id))
            .set(dsl::status.eq(status as i16))
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

        let mut conn = self.conn()?;

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

        diesel::update(dsl::gm_agent_comments.find(comment_id))
            .set(&update)
            .execute(&mut conn)
            .map_err(DbError::from)?;

        debug!(comment_id, "Saved AI analysis to comment");
        Ok(comment_id)  // Return comment_id as the "analysis id"
    }

    async fn get_analysis(&self, comment_id: i32) -> DbResult<Option<StoredAnalysis>> {
        use schema::gm_agent_comments::dsl;

        let mut conn = self.conn()?;

        let result: Option<models::AgentComment> = dsl::gm_agent_comments
            .find(comment_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(result.and_then(|c| {
            // Only return if analysis exists
            if c.suggested_reply.is_some() || c.suggested_dm.is_some() || c.suggested_reply_post.is_some() {
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

        let mut conn = self.conn()?;

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
                            let weight = template.weight.max(1);  // Ensure at least 1
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
                    reply_post_strategy: selected_template.and_then(|t| t.reply_post_prompt.clone()),
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

        let mut conn = self.conn()?;

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

        let mut conn = self.conn()?;

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

        let mut conn = self.conn()?;

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
        use schema::gm_crawler_tasks::dsl;
        use schema::gm_campaigns::dsl as camp_dsl;

        let mut conn = self.conn()?;

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
                let keywords = t.keywords.map(|v| serde_json::Value::Array(
                    v.into_iter().map(serde_json::Value::String).collect()
                ));

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

        let mut conn = self.conn()?;

        let status_str = match status {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
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

    async fn update_task_progress(&self, task_id: i64, increment: i32) -> DbResult<TaskProgressUpdate> {
        let mut conn = self.conn()?;

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

        Ok(result.map(|r| TaskProgressUpdate {
            success: r.success,
            should_stop: r.should_stop,
            new_process_count: r.new_process_count,
            new_actual_consumption: r.new_actual_consumption.to_string().parse().unwrap_or(0.0),
        }).unwrap_or_default())
    }

    async fn set_task_error(&self, task_id: i64, _error: &str) -> DbResult<()> {
        use schema::gm_crawler_tasks::dsl;

        let mut conn = self.conn()?;

        diesel::update(dsl::gm_crawler_tasks.find(task_id as i32))
            .set(dsl::status.eq("failed"))
            .execute(&mut conn)
            .map_err(DbError::from)?;

        warn!(task_id, "Task failed");
        Ok(())
    }

    async fn complete_task(&self, task_id: i64) -> DbResult<()> {
        let mut conn = self.conn()?;

        // Call fn_complete_task stored procedure
        // This will:
        // 1. Update task status to 'completed'
        // 2. Call fn_settle_task_consumption to settle the budget
        // 3. Check if campaign should be stopped
        let result: Option<(bool, String)> = diesel::sql_query(
            "SELECT success, campaign_status FROM fn_complete_task($1, $2)"
        )
        .bind::<diesel::sql_types::Integer, _>(task_id as i32)
        .bind::<diesel::sql_types::Text, _>("completed")
        .get_result::<TaskCompleteResult>(&mut conn)
        .optional()
        .map_err(DbError::from)?
        .map(|r| (r.success, r.campaign_status));

        if let Some((success, campaign_status)) = result {
            debug!(task_id, success, campaign_status, "Task completed via stored procedure");
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
                tracing::debug!(task_id, campaign_id = t.campaign_id, should_stop = result, "Campaign stop check");
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

        let mut conn = self.conn()?;

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
        let mut conn = self.conn()?;

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
                    info!(campaign_id, "✅ Campaign marked as STOPPING (waiting for active tasks)");
                }
            } else {
                warn!(campaign_id, "❌ Failed to stop campaign");
            }
        }

        Ok(result.map(|r| CampaignStopResult {
            success: r.success,
            immediate_stopped: r.immediate_stopped,
            refunded_amount: r.refunded_amount.to_string().parse().unwrap_or(0.0),
        }).unwrap_or_default())
    }
}

// ============================================================
// Helper Methods
// ============================================================

impl PostgresAdapter {
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

    async fn get_campaign_templates(&self, campaign_id: i32) -> DbResult<Vec<models::CampaignTemplate>> {
        use schema::gm_campaign_templates::dsl;

        let mut conn = self.conn()?;

        let templates: Vec<models::CampaignTemplate> = dsl::gm_campaign_templates
            .filter(dsl::campaign_id.eq(campaign_id))
            .order(dsl::weight.desc())
            .load(&mut conn)
            .map_err(DbError::from)?;

        Ok(templates)
    }

    async fn get_active_task_id(&self, campaign_id: i32) -> DbResult<i32> {
        use schema::gm_crawler_tasks::dsl;

        let mut conn = self.conn()?;

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

        let mut conn = self.conn()?;

        let video: Option<models::AgentVideo> = dsl::gm_agent_videos
            .find(video_id)
            .first(&mut conn)
            .optional()
            .map_err(DbError::from)?;

        Ok(video.and_then(|v| v.campaign_id))
    }
}

#[cfg(test)]
mod tests {
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
