//! Mock Adapters - Test implementations using fixtures
//!
//! These adapters use pre-generated fixture data for testing without
//! making real API calls or database connections.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::domain::{Content, Comment, KeywordType, SearchOptions, ReplySuggestion, Engagement};
use crate::domain::errors::{GatewayResult, AiResult, DbResult};
use crate::ports::{
    ContentGateway, CommentGateway, AiAnalyzer,
    ContentRepository, PromptRepository, ProgressTracker,
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    content_repository::{ContentSaveResult, StoredContent, StoredComment, StoredAnalysis, CommentStatus},
    prompt_repository::{CampaignConfig, CampaignStatus, PlatformConfig},
    progress_tracker::{CampaignStopResult, TaskInfo, TaskProgressUpdate, TaskStatus},
    ai_analyzer::AnalysisContext,
};
use crate::fixtures::FixtureLoader;

// ============================================================
// Fixture Mock Gateway - Content and Comments
// ============================================================

/// Mock gateway that loads data from fixtures
pub struct FixtureMockAdapter {
    loader: FixtureLoader,
    platform: String,
}

impl FixtureMockAdapter {
    /// Create a new fixture mock adapter
    pub fn new(loader: FixtureLoader) -> Self {
        Self {
            loader,
            platform: "tiktok".to_string(),
        }
    }

    /// Create with default test fixtures directory
    pub fn from_test_fixtures() -> Self {
        Self::new(FixtureLoader::from_cargo_test())
    }

    /// Convert fixture data to domain Content
    fn fixture_to_content(&self, aweme: &crate::tikhub::AwemeInfo) -> Content {
        Content {
            platform: self.platform.clone(),
            content_id: aweme.aweme_id.clone(),
            author: aweme.author_username().unwrap_or("").to_string(),
            author_name: aweme.author_name().map(|s| s.to_string()),
            description: aweme.description().to_string(),
            url: aweme.share_url.clone(),
            engagement: Engagement {
                likes: aweme.likes(),
                comments: aweme.comments(),
                shares: aweme.shares(),
                views: aweme.views(),
            },
            created_at: aweme.create_time,
            raw_data: serde_json::to_value(aweme).ok(),
        }
    }

    /// Convert fixture comment to domain Comment
    fn fixture_to_comment(&self, comment: &crate::tikhub::TikTokComment, content_id: &str) -> Comment {
        Comment {
            platform: self.platform.clone(),
            comment_id: comment.cid.clone(),
            content_id: content_id.to_string(),
            parent_id: comment.reply_id.clone().filter(|id| id != "0"),
            author: comment.username().unwrap_or("").to_string(),
            author_name: comment.nickname().map(|s| s.to_string()),
            author_uid: comment.user.as_ref().and_then(|u| u.uid.clone()),
            text: comment.content().to_string(),
            likes: comment.likes(),
            reply_count: comment.reply_comment_total.unwrap_or(0),
            created_at: comment.create_time,
            language: comment.comment_language.clone(),
            is_reply: comment.is_reply(),
            raw_data: serde_json::to_value(comment).ok(),
        }
    }
}

#[async_trait]
impl ContentGateway for FixtureMockAdapter {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        // Try to load a fixture matching the query
        let region = options.region.as_deref().unwrap_or("us");
        
        match self.loader.load_search_fixture(&options.query, region) {
            Ok(fixture) => {
                // Use parse_videos() to convert raw JSON to typed structs
                let videos = fixture.parse_videos();
                let contents: Vec<Content> = videos
                    .iter()
                    .take(options.count as usize)
                    .map(|v| self.fixture_to_content(v))
                    .collect();
                Ok(contents)
            }
            Err(_) => {
                // Return empty if no fixture found
                Ok(vec![])
            }
        }
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        match keyword {
            KeywordType::Search(query) | KeywordType::Hashtag(query) => {
                let mut opts = options.clone();
                opts.query = query.clone();
                self.search(&opts).await
            }
            KeywordType::UserId(user_id) => {
                self.fetch_user_content(user_id, options.count).await
            }
            KeywordType::SecUserId(sec_uid) => {
                self.fetch_user_content(sec_uid, options.count).await
            }
            KeywordType::ContentId(content_id) => {
                match self.fetch_by_id(content_id).await? {
                    Some(content) => Ok(vec![content]),
                    None => Ok(vec![]),
                }
            }
        }
    }

    async fn fetch_user_content(
        &self,
        user_id: &str,
        count: u32,
    ) -> GatewayResult<Vec<Content>> {
        // Try to load user videos fixture
        match self.loader.load_user_videos_fixture(user_id) {
            Ok(fixture) => {
                let videos = fixture.parse_videos();
                let contents: Vec<Content> = videos
                    .iter()
                    .take(count as usize)
                    .map(|v| self.fixture_to_content(v))
                    .collect();
                Ok(contents)
            }
            Err(_) => Ok(vec![]),
        }
    }

    async fn fetch_by_id(&self, _content_id: &str) -> GatewayResult<Option<Content>> {
        // Not implemented for fixtures
        Ok(None)
    }

    fn platform(&self) -> &str {
        &self.platform
    }
}

#[async_trait]
impl CommentGateway for FixtureMockAdapter {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        match self.loader.load_comments_fixture(content_id) {
            Ok(fixture) => {
                let parsed_comments = fixture.parse_comments();
                let total = parsed_comments.len();
                let comments: Vec<Comment> = parsed_comments
                    .iter()
                    .take(options.count as usize)
                    .map(|c| self.fixture_to_comment(c, content_id))
                    .collect();
                
                let has_more = total > options.count as usize;
                
                Ok(FetchCommentsResult {
                    comments,
                    has_more,
                    next_cursor: None,
                    total: Some(total as i64),
                })
            }
            Err(_) => Ok(FetchCommentsResult::empty()),
        }
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        let result = self.fetch_comments(
            content_id,
            &FetchCommentsOptions::new(max_count),
        ).await?;
        Ok(result.comments)
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
        &self.platform
    }
}

// ============================================================
// Mock AI Analyzer
// ============================================================

/// Mock AI analyzer for testing
pub struct MockAiAnalyzer {
    model: String,
    /// Pre-configured responses
    responses: Arc<RwLock<HashMap<String, ReplySuggestion>>>,
}

impl MockAiAnalyzer {
    /// Create a new mock analyzer
    pub fn new() -> Self {
        Self {
            model: "mock-model".to_string(),
            responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a pre-configured response for a comment
    pub fn set_response(&self, comment_id: &str, suggestion: ReplySuggestion) {
        let mut responses = self.responses.write().unwrap();
        responses.insert(comment_id.to_string(), suggestion);
    }

    /// Generate a default mock response
    fn generate_mock_response(&self, comment: &Comment) -> ReplySuggestion {
        ReplySuggestion::new(&comment.comment_id)
            .with_reply(format!("Thank you for your comment: {}", &comment.text[..comment.text.len().min(20)]))
            .with_reason("Auto-generated mock response")
            .with_model_info(&self.model, 100)
    }
}

impl Default for MockAiAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AiAnalyzer for MockAiAnalyzer {
    async fn analyze_comment(
        &self,
        comment: &Comment,
        _content: &Content,
        _context: &AnalysisContext,
    ) -> AiResult<ReplySuggestion> {
        let responses = self.responses.read().unwrap();
        if let Some(response) = responses.get(&comment.comment_id) {
            return Ok(response.clone());
        }
        Ok(self.generate_mock_response(comment))
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
        &self.model
    }
}

// ============================================================
// In-Memory Repository
// ============================================================

/// In-memory repository for testing
pub struct InMemoryRepository {
    contents: Arc<RwLock<HashMap<i32, StoredContent>>>,
    comments: Arc<RwLock<HashMap<i32, StoredComment>>>,
    analyses: Arc<RwLock<HashMap<i32, StoredAnalysis>>>,
    campaigns: Arc<RwLock<HashMap<i32, CampaignConfig>>>,
    tasks: Arc<RwLock<HashMap<i64, TaskInfo>>>,
    next_content_id: Arc<RwLock<i32>>,
    next_comment_id: Arc<RwLock<i32>>,
    next_analysis_id: Arc<RwLock<i32>>,
}

impl InMemoryRepository {
    /// Create a new in-memory repository
    pub fn new() -> Self {
        Self {
            contents: Arc::new(RwLock::new(HashMap::new())),
            comments: Arc::new(RwLock::new(HashMap::new())),
            analyses: Arc::new(RwLock::new(HashMap::new())),
            campaigns: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            next_content_id: Arc::new(RwLock::new(1)),
            next_comment_id: Arc::new(RwLock::new(1)),
            next_analysis_id: Arc::new(RwLock::new(1)),
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

    fn get_next_content_id(&self) -> i32 {
        let mut id = self.next_content_id.write().unwrap();
        let current = *id;
        *id += 1;
        current
    }

    fn get_next_comment_id(&self) -> i32 {
        let mut id = self.next_comment_id.write().unwrap();
        let current = *id;
        *id += 1;
        current
    }

    fn get_next_analysis_id(&self) -> i32 {
        let mut id = self.next_analysis_id.write().unwrap();
        let current = *id;
        *id += 1;
        current
    }

    fn platform_id(&self, platform: &str) -> i32 {
        match platform {
            "tiktok" => 2,
            "instagram" => 4,
            _ => 0,
        }
    }
}

impl Default for InMemoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentRepository for InMemoryRepository {
    async fn content_exists(&self, platform: &str, content_id: &str) -> DbResult<bool> {
        let contents = self.contents.read().unwrap();
        let platform_id = self.platform_id(platform);
        Ok(contents.values().any(|c| c.platform_id == platform_id && c.content_id == content_id))
    }

    async fn get_content(&self, platform: &str, content_id: &str) -> DbResult<Option<StoredContent>> {
        let contents = self.contents.read().unwrap();
        let platform_id = self.platform_id(platform);
        Ok(contents.values()
            .find(|c| c.platform_id == platform_id && c.content_id == content_id)
            .cloned())
    }

    async fn get_content_by_id(&self, id: i32) -> DbResult<Option<StoredContent>> {
        let contents = self.contents.read().unwrap();
        Ok(contents.get(&id).cloned())
    }

    async fn save_content(&self, content: &Content, campaign_id: Option<i32>, _task_id: Option<i32>) -> DbResult<ContentSaveResult> {
        let platform_id = self.platform_id(&content.platform);
        
        // Check if content already exists
        let existing_id = {
            let contents = self.contents.read().unwrap();
            contents.values()
                .find(|c| c.platform_id == platform_id && c.content_id == content.content_id)
                .map(|c| c.id)
        };
        
        if let Some(existing_id) = existing_id {
            // Update existing record
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
            return Ok(ContentSaveResult {
                id: existing_id,
                is_new: false,
            });
        }
        
        // Insert new record
        let id = self.get_next_content_id();
        let stored = StoredContent {
            id,
            platform_id,
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
        
        let mut contents = self.contents.write().unwrap();
        contents.insert(id, stored);
        Ok(ContentSaveResult {
            id,
            is_new: true,
        })
    }

    async fn save_contents(&self, contents: &[Content], campaign_id: Option<i32>, task_id: Option<i32>) -> DbResult<Vec<ContentSaveResult>> {
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
        let mut contents = self.contents.write().unwrap();
        if let Some(content) = contents.get_mut(&id) {
            content.likes = Some(likes);
            content.comments = Some(comments);
            content.shares = Some(shares);
            content.views = Some(views);
        }
        Ok(())
    }

    async fn comment_exists(&self, platform: &str, comment_id: &str) -> DbResult<bool> {
        let comments = self.comments.read().unwrap();
        let platform_id = self.platform_id(platform);
        Ok(comments.values().any(|c| c.platform_id == platform_id && c.comment_id == comment_id))
    }

    async fn get_comment(&self, platform: &str, comment_id: &str) -> DbResult<Option<StoredComment>> {
        let comments = self.comments.read().unwrap();
        let platform_id = self.platform_id(platform);
        Ok(comments.values()
            .find(|c| c.platform_id == platform_id && c.comment_id == comment_id)
            .cloned())
    }

    async fn get_comment_by_id(&self, id: i32) -> DbResult<Option<StoredComment>> {
        let comments = self.comments.read().unwrap();
        Ok(comments.get(&id).cloned())
    }

    async fn save_comment(&self, comment: &Comment, content_db_id: i32) -> DbResult<i32> {
        let id = self.get_next_comment_id();
        let stored = StoredComment {
            id,
            platform_id: self.platform_id(&comment.platform),
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
        
        let mut comments = self.comments.write().unwrap();
        comments.insert(id, stored);
        Ok(id)
    }

    async fn save_comments(&self, comments: &[Comment], content_db_id: i32) -> DbResult<Vec<i32>> {
        let mut ids = Vec::with_capacity(comments.len());
        for comment in comments {
            ids.push(self.save_comment(comment, content_db_id).await?);
        }
        Ok(ids)
    }

    async fn get_pending_comments(&self, campaign_id: i32, limit: i32) -> DbResult<Vec<StoredComment>> {
        let contents = self.contents.read().unwrap();
        let comments = self.comments.read().unwrap();
        
        let campaign_content_ids: Vec<i32> = contents.values()
            .filter(|c| c.campaign_id == Some(campaign_id))
            .map(|c| c.id)
            .collect();
        
        Ok(comments.values()
            .filter(|c| {
                campaign_content_ids.contains(&c.content_id) &&
                c.status == CommentStatus::Pending as i16
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn update_comment_status(&self, id: i32, status: CommentStatus) -> DbResult<()> {
        let mut comments = self.comments.write().unwrap();
        if let Some(comment) = comments.get_mut(&id) {
            comment.status = status as i16;
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
        let platform_id = self.platform_id(&comment.platform);
        
        // Check if comment already exists
        let existing_id = {
            let comments = self.comments.read().unwrap();
            comments.values()
                .find(|c| c.platform_id == platform_id && c.comment_id == comment.comment_id)
                .map(|c| c.id)
        };
        
        if let Some(existing_id) = existing_id {
            // Update with analysis
            let _ = self.save_analysis(existing_id, campaign_id, suggestion).await?;
            return Ok(existing_id);
        }
        
        // Insert new comment with analysis
        let id = self.get_next_comment_id();
        let stored = StoredComment {
            id,
            platform_id,
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
        let id = self.get_next_analysis_id();
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
        
        // Insert the analysis - drop lock before await
        {
            let mut analyses = self.analyses.write().unwrap();
            analyses.insert(id, stored);
        }
        
        self.update_comment_status(comment_id, CommentStatus::Completed).await?;
        Ok(id)
    }

    async fn get_analysis(&self, comment_id: i32) -> DbResult<Option<StoredAnalysis>> {
        let analyses = self.analyses.read().unwrap();
        Ok(analyses.values()
            .find(|a| a.comment_id == comment_id)
            .cloned())
    }
}

#[async_trait]
impl PromptRepository for InMemoryRepository {
    async fn get_campaign(&self, campaign_id: i32) -> DbResult<Option<CampaignConfig>> {
        let campaigns = self.campaigns.read().unwrap();
        Ok(campaigns.get(&campaign_id).cloned())
    }

    async fn get_analysis_context(&self, campaign_id: i32) -> DbResult<Option<AnalysisContext>> {
        let campaign = self.get_campaign(campaign_id).await?;
        Ok(campaign.map(|c| c.to_analysis_context()))
    }

    async fn get_platform(&self, platform_id: i32) -> DbResult<Option<PlatformConfig>> {
        Ok(Some(PlatformConfig {
            id: platform_id,
            name: PlatformConfig::name_from_id(platform_id).to_string(),
            display_name: PlatformConfig::name_from_id(platform_id).to_string(),
            is_active: true,
        }))
    }

    async fn get_platform_by_name(&self, name: &str) -> DbResult<Option<PlatformConfig>> {
        let id = match name {
            "tiktok" => 2,
            "instagram" => 4,
            _ => return Ok(None),
        };
        self.get_platform(id).await
    }

    async fn should_stop_campaign(&self, campaign_id: i32) -> DbResult<bool> {
        let campaign = self.get_campaign(campaign_id).await?;
        match campaign {
            Some(c) => Ok(!c.status.should_continue() || c.is_at_limit()),
            None => Ok(true),
        }
    }

    async fn update_processed_count(&self, campaign_id: i32, count: i32) -> DbResult<()> {
        let mut campaigns = self.campaigns.write().unwrap();
        if let Some(campaign) = campaigns.get_mut(&campaign_id) {
            campaign.processed_comments = count;
        }
        Ok(())
    }
}

#[async_trait]
impl ProgressTracker for InMemoryRepository {
    async fn get_task(&self, task_id: i64) -> DbResult<Option<TaskInfo>> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks.get(&task_id).cloned())
    }

    async fn update_task_status(&self, task_id: i64, status: TaskStatus) -> DbResult<()> {
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            task.status = status;
        }
        Ok(())
    }

    async fn update_task_progress(&self, task_id: i64, increment: i32) -> DbResult<TaskProgressUpdate> {
        let campaign_id;
        {
            let mut tasks = self.tasks.write().unwrap();
            if let Some(task) = tasks.get_mut(&task_id) {
                task.progress += increment;
                campaign_id = task.campaign_id;
            } else {
                return Ok(TaskProgressUpdate::default());
            }
        }
        
        // Check if campaign should stop
        let should_stop = self.should_stop_campaign(campaign_id).await.unwrap_or(false);
        
        let tasks = self.tasks.read().unwrap();
        let progress = tasks.get(&task_id).map(|t| t.progress).unwrap_or(0);
        
        Ok(TaskProgressUpdate {
            success: true,
            should_stop,
            new_process_count: progress,
            new_actual_consumption: progress as f64 * 1.5,
        })
    }

    async fn set_task_error(&self, task_id: i64, error: &str) -> DbResult<()> {
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            task.status = TaskStatus::Failed;
            task.error_message = Some(error.to_string());
        }
        Ok(())
    }

    async fn complete_task(&self, task_id: i64) -> DbResult<()> {
        self.update_task_status(task_id, TaskStatus::Completed).await?;
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            task.progress = 100;
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
        let mut campaigns = self.campaigns.write().unwrap();
        if let Some(campaign) = campaigns.get_mut(&campaign_id) {
            campaign.processed_comments += count;
        }
        Ok(())
    }

    async fn get_processed_count(&self, campaign_id: i32) -> DbResult<i32> {
        let campaigns = self.campaigns.read().unwrap();
        Ok(campaigns.get(&campaign_id).map(|c| c.processed_comments).unwrap_or(0))
    }
    
    async fn stop_campaign_gracefully(&self, campaign_id: i32) -> DbResult<CampaignStopResult> {
        // Check if there are active tasks
        let has_active_tasks = {
            let tasks = self.tasks.read().unwrap();
            tasks.values().any(|t| t.campaign_id == campaign_id && !t.is_terminal())
        };
        
        let mut campaigns = self.campaigns.write().unwrap();
        if let Some(campaign) = campaigns.get_mut(&campaign_id) {
            if has_active_tasks {
                campaign.status = CampaignStatus::Stopping;
                return Ok(CampaignStopResult {
                    success: true,
                    immediate_stopped: false,
                    refunded_amount: 0.0,
                });
            } else {
                // Use Completed status (no Stopped status in the enum)
                campaign.status = CampaignStatus::Completed;
                return Ok(CampaignStopResult {
                    success: true,
                    immediate_stopped: true,
                    refunded_amount: 100.0,
                });
            }
        }
        
        Ok(CampaignStopResult::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_content_repository() {
        let repo = InMemoryRepository::new();
        
        let content = Content::new("tiktok", "v123")
            .with_author("testuser")
            .with_description("Test video");
        
        // Save content - first save should be new
        let result = repo.save_content(&content, Some(1), Some(1)).await.unwrap();
        assert!(result.id > 0);
        assert!(result.is_new);
        
        // Save same content again - should not be new
        let result2 = repo.save_content(&content, Some(1), Some(1)).await.unwrap();
        assert_eq!(result2.id, result.id);
        assert!(!result2.is_new);
        
        // Check exists
        let exists = repo.content_exists("tiktok", "v123").await.unwrap();
        assert!(exists);
        
        // Get content
        let stored = repo.get_content("tiktok", "v123").await.unwrap();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().content_id, "v123");
    }

    #[tokio::test]
    async fn test_mock_ai_analyzer() {
        let analyzer = MockAiAnalyzer::new();
        
        let content = Content::new("tiktok", "v123");
        let comment = Comment::new("tiktok", "c1", "v123")
            .with_text("Great video!");
        let context = AnalysisContext::new();
        
        let suggestion = analyzer.analyze_comment(&comment, &content, &context).await.unwrap();
        assert!(suggestion.reply_text.is_some());
    }
}
