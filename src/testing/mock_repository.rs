//! Mock Repository for testing database operations

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::RwLock;

use crate::domain::errors::{DbError, DbResult};
use crate::domain::{Comment, Content, ReplySuggestion};
use crate::ports::{
    ai_analyzer::AnalysisContext,
    content_repository::{
        CommentStatus, ContentSaveResult, StoredAnalysis, StoredComment, StoredContent,
    },
    progress_tracker::{CampaignStopResult, TaskInfo, TaskProgressUpdate, TaskStatus},
    prompt_repository::{CampaignConfig, PlatformConfig},
    ContentRepository, ProgressTracker, PromptRepository,
};

/// Mock repository implementing all repository ports
pub struct MockRepository {
    // Content storage
    contents: RwLock<HashMap<i32, StoredContent>>,
    content_by_platform_id: RwLock<HashMap<(i32, String), i32>>,
    next_content_id: AtomicI32,

    // Comment storage
    comments: RwLock<HashMap<i32, StoredComment>>,
    comment_by_platform_id: RwLock<HashMap<(i32, String), i32>>,
    next_comment_id: AtomicI32,

    // Analysis storage
    analyses: RwLock<HashMap<i32, StoredAnalysis>>,
    next_analysis_id: AtomicI32,

    // Campaign storage
    campaigns: RwLock<HashMap<i32, CampaignConfig>>,

    // Task storage
    tasks: RwLock<HashMap<i64, TaskInfo>>,

    // Platform storage
    platforms: RwLock<HashMap<i32, PlatformConfig>>,

    // Error simulation
    error_mode: RwLock<Option<MockDbError>>,
}

/// Simulated database errors
#[derive(Debug, Clone)]
pub enum MockDbError {
    Connection,
    NotFound,
    Duplicate,
    Constraint,
}

impl MockRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        let repo = Self {
            contents: RwLock::new(HashMap::new()),
            content_by_platform_id: RwLock::new(HashMap::new()),
            next_content_id: AtomicI32::new(1),
            comments: RwLock::new(HashMap::new()),
            comment_by_platform_id: RwLock::new(HashMap::new()),
            next_comment_id: AtomicI32::new(1),
            analyses: RwLock::new(HashMap::new()),
            next_analysis_id: AtomicI32::new(1),
            campaigns: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            platforms: RwLock::new(HashMap::new()),
            error_mode: RwLock::new(None),
        };

        // Initialize default platforms
        repo.init_default_platforms();

        repo
    }

    fn init_default_platforms(&self) {
        use crate::config::platform::{global_registry, PlatformLookup};

        let mut platforms = self.platforms.write().unwrap();
        let registry = global_registry();

        // Load platforms from global registry (initialized from database at startup)
        for platform_info in registry.all_platforms() {
            platforms.insert(
                platform_info.id,
                PlatformConfig {
                    id: platform_info.id,
                    name: platform_info.name.clone(),
                    display_name: platform_info.display_name.clone(),
                    is_active: platform_info.is_active,
                },
            );
        }
    }

    /// Add a campaign for testing
    pub fn add_campaign(&self, campaign: CampaignConfig) {
        let mut campaigns = self.campaigns.write().unwrap();
        campaigns.insert(campaign.id, campaign);
    }

    /// Add a task for testing
    pub fn add_task(&self, task: TaskInfo) {
        let mut tasks = self.tasks.write().unwrap();
        tasks.insert(task.id, task);
    }

    /// Set error mode
    pub fn set_error(&self, error: Option<MockDbError>) {
        *self.error_mode.write().unwrap() = error;
    }

    /// Get all stored contents
    pub fn get_all_contents(&self) -> Vec<StoredContent> {
        self.contents.read().unwrap().values().cloned().collect()
    }

    /// Get all stored comments
    pub fn get_all_comments(&self) -> Vec<StoredComment> {
        self.comments.read().unwrap().values().cloned().collect()
    }

    /// Get all stored analyses
    pub fn get_all_analyses(&self) -> Vec<StoredAnalysis> {
        self.analyses.read().unwrap().values().cloned().collect()
    }

    fn check_error(&self) -> DbResult<()> {
        let mode = self.error_mode.read().unwrap();
        match mode.as_ref() {
            Some(MockDbError::Connection) => {
                Err(DbError::Connection("Mock connection error".into()))
            }
            Some(MockDbError::NotFound) => Err(DbError::NotFound("Mock not found".into())),
            Some(MockDbError::Duplicate) => Err(DbError::Duplicate("Mock duplicate".into())),
            Some(MockDbError::Constraint) => Err(DbError::Constraint("Mock constraint".into())),
            None => Ok(()),
        }
    }

    fn platform_id(&self, platform: &str) -> i32 {
        use crate::config::platform::{global_registry, PlatformLookup};

        global_registry().get_id(platform).unwrap_or(0)
    }
}

impl Default for MockRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentRepository for MockRepository {
    async fn content_exists(&self, platform: &str, content_id: &str) -> DbResult<bool> {
        self.check_error()?;
        let pid = self.platform_id(platform);
        let idx = self.content_by_platform_id.read().unwrap();
        Ok(idx.contains_key(&(pid, content_id.to_string())))
    }

    async fn get_content(
        &self,
        platform: &str,
        content_id: &str,
    ) -> DbResult<Option<StoredContent>> {
        self.check_error()?;
        let pid = self.platform_id(platform);
        let idx = self.content_by_platform_id.read().unwrap();
        if let Some(&id) = idx.get(&(pid, content_id.to_string())) {
            let contents = self.contents.read().unwrap();
            return Ok(contents.get(&id).cloned());
        }
        Ok(None)
    }

    async fn get_content_by_id(&self, id: i32) -> DbResult<Option<StoredContent>> {
        self.check_error()?;
        let contents = self.contents.read().unwrap();
        Ok(contents.get(&id).cloned())
    }

    async fn save_content(
        &self,
        content: &Content,
        campaign_id: Option<i32>,
        _task_id: Option<i32>,
    ) -> DbResult<ContentSaveResult> {
        self.check_error()?;
        let pid = self.platform_id(&content.platform);

        // Check if content already exists (ON CONFLICT simulation)
        let existing_id = {
            let idx = self.content_by_platform_id.read().unwrap();
            idx.get(&(pid, content.content_id.clone())).copied()
        };

        if let Some(existing_id) = existing_id {
            // Content exists - update it (matching ON CONFLICT DO UPDATE behavior)
            {
                let mut contents = self.contents.write().unwrap();
                if let Some(stored) = contents.get_mut(&existing_id) {
                    stored.description = Some(content.description.clone());
                    stored.author_nickname = content.author_name.clone();
                    stored.campaign_id = campaign_id;
                    stored.likes = Some(content.engagement.likes);
                    stored.comments = Some(content.engagement.comments);
                    stored.shares = Some(content.engagement.shares);
                    stored.views = Some(content.engagement.views);
                }
            }
            return Ok(ContentSaveResult {
                id: existing_id,
                is_new: false, // Existing record was updated
            });
        }

        // New content - insert
        let id = self.next_content_id.fetch_add(1, Ordering::SeqCst);

        let stored = StoredContent {
            id,
            platform_id: pid,
            content_id: content.content_id.clone(),
            author_unique_id: Some(content.author.clone()),
            author_nickname: content.author_name.clone(),
            description: Some(content.description.clone()),
            content_url: content.url.clone(),
            likes: Some(content.engagement.likes),
            comments: Some(content.engagement.comments),
            shares: Some(content.engagement.shares),
            views: Some(content.engagement.views),
            content_created_at: content.created_at,
            raw_data: content.raw_data.clone(),
            campaign_id,
        };

        {
            let mut contents = self.contents.write().unwrap();
            contents.insert(id, stored);
        }
        {
            let mut idx = self.content_by_platform_id.write().unwrap();
            idx.insert((pid, content.content_id.clone()), id);
        }

        Ok(ContentSaveResult {
            id,
            is_new: true, // New record was inserted
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
            results.push(self.save_content(content, campaign_id, task_id).await?);
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
        self.check_error()?;
        let mut contents = self.contents.write().unwrap();
        if let Some(c) = contents.get_mut(&id) {
            c.likes = Some(likes);
            c.comments = Some(comments);
            c.shares = Some(shares);
            c.views = Some(views);
        }
        Ok(())
    }

    async fn comment_exists(&self, platform: &str, comment_id: &str) -> DbResult<bool> {
        self.check_error()?;
        let pid = self.platform_id(platform);
        let idx = self.comment_by_platform_id.read().unwrap();
        Ok(idx.contains_key(&(pid, comment_id.to_string())))
    }

    async fn get_comment(
        &self,
        platform: &str,
        comment_id: &str,
    ) -> DbResult<Option<StoredComment>> {
        self.check_error()?;
        let pid = self.platform_id(platform);
        let idx = self.comment_by_platform_id.read().unwrap();
        if let Some(&id) = idx.get(&(pid, comment_id.to_string())) {
            let comments = self.comments.read().unwrap();
            return Ok(comments.get(&id).cloned());
        }
        Ok(None)
    }

    async fn get_comment_by_id(&self, id: i32) -> DbResult<Option<StoredComment>> {
        self.check_error()?;
        let comments = self.comments.read().unwrap();
        Ok(comments.get(&id).cloned())
    }

    async fn save_comment(&self, comment: &Comment, content_db_id: i32) -> DbResult<i32> {
        self.check_error()?;
        let id = self.next_comment_id.fetch_add(1, Ordering::SeqCst);
        let pid = self.platform_id(&comment.platform);

        let stored = StoredComment {
            id,
            platform_id: pid,
            content_id: content_db_id,
            comment_id: comment.comment_id.clone(),
            parent_comment_id: comment.parent_id.clone(),
            author_uid: comment.author_uid.clone(),
            author_unique_id: Some(comment.author.clone()),
            author_nickname: comment.author_name.clone(),
            comment_text: Some(comment.text.clone()),
            likes: Some(comment.likes),
            reply_count: Some(comment.reply_count),
            comment_created_at: comment.created_at,
            is_reply: comment.is_reply,
            raw_data: comment.raw_data.clone(),
            status: CommentStatus::Pending as i16,
        };

        {
            let mut comments = self.comments.write().unwrap();
            comments.insert(id, stored);
        }
        {
            let mut idx = self.comment_by_platform_id.write().unwrap();
            idx.insert((pid, comment.comment_id.clone()), id);
        }

        Ok(id)
    }

    async fn save_comments(&self, comments: &[Comment], content_db_id: i32) -> DbResult<Vec<i32>> {
        let mut ids = Vec::with_capacity(comments.len());
        for comment in comments {
            ids.push(self.save_comment(comment, content_db_id).await?);
        }
        Ok(ids)
    }

    async fn get_pending_comments(
        &self,
        campaign_id: i32,
        limit: i32,
    ) -> DbResult<Vec<StoredComment>> {
        self.check_error()?;
        let contents = self.contents.read().unwrap();
        let comments = self.comments.read().unwrap();

        let campaign_content_ids: Vec<i32> = contents
            .values()
            .filter(|c| c.campaign_id == Some(campaign_id))
            .map(|c| c.id)
            .collect();

        Ok(comments
            .values()
            .filter(|c| {
                campaign_content_ids.contains(&c.content_id)
                    && c.status == CommentStatus::Pending as i16
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn update_comment_status(&self, id: i32, status: CommentStatus) -> DbResult<()> {
        self.check_error()?;
        let mut comments = self.comments.write().unwrap();
        if let Some(c) = comments.get_mut(&id) {
            c.status = status as i16;
        }
        Ok(())
    }

    async fn save_comment_with_analysis(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        self.check_error()?;
        let pid = self.platform_id(&comment.platform);

        // Check if comment already exists
        let existing_id = {
            let idx = self.comment_by_platform_id.read().unwrap();
            idx.get(&(pid, comment.comment_id.clone())).copied()
        };

        if let Some(existing_id) = existing_id {
            // Update existing comment with analysis
            {
                let mut comments = self.comments.write().unwrap();
                if let Some(stored) = comments.get_mut(&existing_id) {
                    stored.status = CommentStatus::Pending as i16; // 0 for user review
                }
            }

            // Save/update analysis
            let _analysis_id = self
                .save_analysis(existing_id, campaign_id, suggestion)
                .await?;
            return Ok(existing_id);
        }

        // Insert new comment with analysis
        let id = self.next_comment_id.fetch_add(1, Ordering::SeqCst);

        let stored = StoredComment {
            id,
            platform_id: pid,
            content_id: content_db_id,
            comment_id: comment.comment_id.clone(),
            parent_comment_id: comment.parent_id.clone(),
            author_uid: comment.author_uid.clone(),
            author_unique_id: Some(comment.author.clone()),
            author_nickname: comment.author_name.clone(),
            comment_text: Some(comment.text.clone()),
            likes: Some(comment.likes),
            reply_count: Some(comment.reply_count),
            comment_created_at: comment.created_at,
            is_reply: comment.is_reply,
            raw_data: comment.raw_data.clone(),
            status: CommentStatus::Pending as i16, // 0 for user review
        };

        {
            let mut comments = self.comments.write().unwrap();
            comments.insert(id, stored);
        }
        {
            let mut idx = self.comment_by_platform_id.write().unwrap();
            idx.insert((pid, comment.comment_id.clone()), id);
        }

        // Save analysis
        let _ = self.save_analysis(id, campaign_id, suggestion).await?;

        Ok(id)
    }

    async fn save_analysis(
        &self,
        comment_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32> {
        self.check_error()?;
        let id = self.next_analysis_id.fetch_add(1, Ordering::SeqCst);

        let stored = StoredAnalysis {
            id,
            comment_id,
            campaign_id,
            suggested_reply: suggestion.reply_text.clone(),
            suggested_dm: suggestion.dm_text.clone(),
            suggested_reply_post: suggestion.post_reply_text.clone(),
            reason: suggestion.reason.clone(),
            tokens_used: suggestion.tokens_used,
            model_name: suggestion.model.clone(),
        };

        {
            let mut analyses = self.analyses.write().unwrap();
            analyses.insert(id, stored);
        }

        // Update comment status to completed (with analysis)
        self.update_comment_status(comment_id, CommentStatus::Completed)
            .await?;

        Ok(id)
    }

    async fn get_analysis(&self, comment_id: i32) -> DbResult<Option<StoredAnalysis>> {
        self.check_error()?;
        let analyses = self.analyses.read().unwrap();
        Ok(analyses
            .values()
            .find(|a| a.comment_id == comment_id)
            .cloned())
    }
}

#[async_trait]
impl PromptRepository for MockRepository {
    async fn get_campaign(&self, campaign_id: i32) -> DbResult<Option<CampaignConfig>> {
        self.check_error()?;
        let campaigns = self.campaigns.read().unwrap();
        Ok(campaigns.get(&campaign_id).cloned())
    }

    async fn get_analysis_context(&self, campaign_id: i32) -> DbResult<Option<AnalysisContext>> {
        self.check_error()?;
        let campaign = self.get_campaign(campaign_id).await?;
        Ok(campaign.map(|c| c.to_analysis_context()))
    }

    async fn get_platform(&self, platform_id: i32) -> DbResult<Option<PlatformConfig>> {
        self.check_error()?;
        let platforms = self.platforms.read().unwrap();
        Ok(platforms.get(&platform_id).cloned())
    }

    async fn get_platform_by_name(&self, name: &str) -> DbResult<Option<PlatformConfig>> {
        self.check_error()?;
        let platforms = self.platforms.read().unwrap();
        Ok(platforms.values().find(|p| p.name == name).cloned())
    }

    async fn should_stop_campaign(&self, campaign_id: i32) -> DbResult<bool> {
        let campaign = self.get_campaign(campaign_id).await?;
        match campaign {
            Some(c) => Ok(!c.status.should_continue() || c.is_at_limit()),
            None => Ok(true),
        }
    }

    async fn update_processed_count(&self, campaign_id: i32, count: i32) -> DbResult<()> {
        self.check_error()?;
        let mut campaigns = self.campaigns.write().unwrap();
        if let Some(c) = campaigns.get_mut(&campaign_id) {
            c.processed_comments = count;
        }
        Ok(())
    }
}

#[async_trait]
impl ProgressTracker for MockRepository {
    async fn get_task(&self, task_id: i64) -> DbResult<Option<TaskInfo>> {
        self.check_error()?;
        let tasks = self.tasks.read().unwrap();
        Ok(tasks.get(&task_id).cloned())
    }

    async fn update_task_status(&self, task_id: i64, status: TaskStatus) -> DbResult<()> {
        self.check_error()?;
        let mut tasks = self.tasks.write().unwrap();
        if let Some(t) = tasks.get_mut(&task_id) {
            t.status = status;
        }
        Ok(())
    }

    async fn update_task_progress(
        &self,
        task_id: i64,
        increment: i32,
    ) -> DbResult<TaskProgressUpdate> {
        self.check_error()?;

        let (campaign_id, progress) = {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(t) = tasks.get_mut(&task_id) {
                t.progress += increment;
                (Some(t.campaign_id), t.progress)
            } else {
                (None, 0)
            }
        }; // Lock released here before await

        let mut result = TaskProgressUpdate::default();

        if let Some(campaign_id) = campaign_id {
            result.success = true;
            result.new_process_count = progress;
            result.new_actual_consumption = progress as f64 * 1.5; // Simulated unit price

            // Check if campaign is stopping
            result.should_stop = self
                .should_stop_campaign(campaign_id)
                .await
                .unwrap_or(false);
        }

        Ok(result)
    }

    async fn set_task_error(&self, task_id: i64, error: &str) -> DbResult<()> {
        self.check_error()?;
        let mut tasks = self.tasks.write().unwrap();
        if let Some(t) = tasks.get_mut(&task_id) {
            t.status = TaskStatus::Failed;
            t.error_message = Some(error.to_string());
        }
        Ok(())
    }

    async fn complete_task(&self, task_id: i64) -> DbResult<()> {
        self.update_task_status(task_id, TaskStatus::Completed)
            .await?;
        let mut tasks = self.tasks.write().unwrap();
        if let Some(t) = tasks.get_mut(&task_id) {
            t.progress = 100;
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
                if t.is_terminal() {
                    return Ok(true);
                }
                self.should_stop_campaign(t.campaign_id).await
            }
            None => Ok(true),
        }
    }

    async fn increment_processed(&self, campaign_id: i32, count: i32) -> DbResult<()> {
        self.check_error()?;
        let mut campaigns = self.campaigns.write().unwrap();
        if let Some(c) = campaigns.get_mut(&campaign_id) {
            c.processed_comments += count;
        }
        Ok(())
    }

    async fn get_processed_count(&self, campaign_id: i32) -> DbResult<i32> {
        let campaigns = self.campaigns.read().unwrap();
        Ok(campaigns
            .get(&campaign_id)
            .map(|c| c.processed_comments)
            .unwrap_or(0))
    }

    async fn stop_campaign_gracefully(&self, campaign_id: i32) -> DbResult<CampaignStopResult> {
        self.check_error()?;

        // Check if there are any active tasks (before locking campaigns)
        let has_active_tasks = {
            let tasks = self.tasks.read().unwrap();
            tasks
                .values()
                .any(|t| t.campaign_id == campaign_id && !t.is_terminal())
        };

        let mut campaigns = self.campaigns.write().unwrap();

        if let Some(c) = campaigns.get_mut(&campaign_id) {
            use crate::ports::prompt_repository::CampaignStatus;

            if has_active_tasks {
                // Set to STOPPING state
                c.status = CampaignStatus::Stopping;
                return Ok(CampaignStopResult {
                    success: true,
                    immediate_stopped: false,
                    refunded_amount: 0.0,
                });
            } else {
                // Immediately complete (no Stopped variant, use Completed)
                c.status = CampaignStatus::Completed;
                return Ok(CampaignStopResult {
                    success: true,
                    immediate_stopped: true,
                    refunded_amount: 100.0, // Simulated refund
                });
            }
        }

        Ok(CampaignStopResult::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Engagement;
    use crate::ports::prompt_repository::CampaignStatus;

    #[tokio::test]
    async fn test_mock_repository_content() {
        let repo = MockRepository::new();

        let content = Content::new("tiktok", "v123")
            .with_author("testuser")
            .with_engagement(Engagement {
                likes: 100,
                comments: 10,
                shares: 5,
                views: 1000,
            });

        // Save content - first save should be new
        let result = repo.save_content(&content, Some(1), Some(1)).await.unwrap();
        assert!(result.id > 0);
        assert!(result.is_new);

        // Save same content again - should not be new
        let result2 = repo.save_content(&content, Some(1), Some(1)).await.unwrap();
        assert_eq!(result2.id, result.id);
        assert!(!result2.is_new);

        // Check exists
        assert!(repo.content_exists("tiktok", "v123").await.unwrap());
        assert!(!repo.content_exists("tiktok", "v456").await.unwrap());

        // Get content
        let stored = repo.get_content("tiktok", "v123").await.unwrap().unwrap();
        assert_eq!(stored.content_id, "v123");
        assert_eq!(stored.likes, Some(100));
    }

    #[tokio::test]
    async fn test_mock_repository_comments() {
        let repo = MockRepository::new();

        // First save a content
        let content = Content::new("tiktok", "v123");
        let content_result = repo.save_content(&content, Some(1), Some(1)).await.unwrap();
        let content_id = content_result.id;

        // Save comment
        let comment = Comment::new("tiktok", "c1", "v123")
            .with_author("commenter")
            .with_text("Great video!");
        let comment_id = repo.save_comment(&comment, content_id).await.unwrap();

        // Check
        assert!(repo.comment_exists("tiktok", "c1").await.unwrap());
        let stored = repo.get_comment_by_id(comment_id).await.unwrap().unwrap();
        assert_eq!(stored.comment_text, Some("Great video!".to_string()));
    }

    #[tokio::test]
    async fn test_mock_repository_campaign() {
        let repo = MockRepository::new();

        let campaign = CampaignConfig {
            id: 1,
            user_id: 1,
            name: "Test Campaign".to_string(),
            platform_id: 2,
            status: CampaignStatus::Active,
            target_audience: Some("Young adults".to_string()),
            product_prompt: Some("Fitness app".to_string()),
            reply_strategy: None,
            dm_strategy: None,
            reply_post_strategy: None,
            max_comments: Some(100),
            processed_comments: 0,
        };

        repo.add_campaign(campaign);

        let stored = repo.get_campaign(1).await.unwrap().unwrap();
        assert_eq!(stored.name, "Test Campaign");

        let ctx = repo.get_analysis_context(1).await.unwrap().unwrap();
        assert_eq!(ctx.product_prompt, Some("Fitness app".to_string()));
    }
}
