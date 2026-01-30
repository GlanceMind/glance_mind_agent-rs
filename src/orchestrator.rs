//! Workflow Orchestrator - Coordinates task processing using ports
//!
//! The orchestrator is the heart of the hexagonal architecture. It:
//! - Receives tasks from the inbound adapter (Redis consumer)
//! - Uses strategies to handle platform-specific logic
//! - Calls outbound ports (gateways, repositories, AI) to do actual work
//! - Tracks progress and handles errors
//!
//! ## Concurrency Model
//!
//! Uses Tokio's async tasks (similar to Go goroutines) for lightweight concurrency:
//! - Videos within a keyword are processed in parallel (controlled by `max_concurrent_videos`)
//! - Progress updates happen after each video completes (sequential within result handling)
//! - Stop signals are propagated to cancel remaining work

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use futures::stream::{self, StreamExt};
use tracing::{debug, error, info, warn};

use crate::domain::{
    Content, Comment, TaskConfig, TaskResult, KeywordType,
};
use crate::domain::errors::{WorkflowError, WorkflowResult};
use crate::ports::{
    ContentGateway, CommentGateway, AiAnalyzer,
    ContentRepository, PromptRepository, ProgressTracker,
    ai_analyzer::AnalysisContext,
    progress_tracker::TaskStatus,
};
use crate::strategies::PlatformStrategy;

/// Result of processing a single video
/// 
/// Used internally for parallel video processing to collect results
/// before aggregating counts and updating progress.
#[derive(Debug)]
enum VideoProcessResult {
    /// Video processed successfully
    Success {
        content_id: String,
        is_new: bool,
        comments: i32,
        analyses: i32,
    },
    /// Video processing failed
    Error {
        content_id: String,
        error: WorkflowError,
    },
    /// Video processing stopped due to campaign stop signal
    Stopped,
    /// Video skipped due to stop flag already set
    Skipped,
}

/// Workflow orchestrator that coordinates task processing
pub struct WorkflowOrchestrator {
    /// Content gateway (fetch videos/posts)
    content_gateway: Arc<dyn ContentGateway>,
    
    /// Comment gateway (fetch comments)
    comment_gateway: Arc<dyn CommentGateway>,
    
    /// AI analyzer (generate reply suggestions)
    ai_analyzer: Arc<dyn AiAnalyzer>,
    
    /// Content repository (persist data)
    content_repo: Arc<dyn ContentRepository>,
    
    /// Prompt repository (campaign config)
    prompt_repo: Arc<dyn PromptRepository>,
    
    /// Progress tracker (task status)
    progress_tracker: Arc<dyn ProgressTracker>,
    
    /// Platform strategies
    strategies: HashMap<String, Arc<dyn PlatformStrategy>>,

    /// Processing configuration
    config: OrchestratorConfig,
}

/// Configuration for the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Default max videos per keyword
    pub max_videos_per_keyword: u32,
    
    /// Default max comments per video
    pub max_comments_per_video: u32,
    
    /// Batch size for AI analysis
    pub ai_batch_size: usize,
    
    /// Whether to continue on individual errors
    pub continue_on_error: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_videos_per_keyword: 10,
            max_comments_per_video: 50,
            // Matching Python agent: MAX_COMMENTS_PER_BATCH = 150
            ai_batch_size: 150,
            continue_on_error: true,
        }
    }
}

/// Builder for WorkflowOrchestrator
pub struct OrchestratorBuilder {
    content_gateway: Option<Arc<dyn ContentGateway>>,
    comment_gateway: Option<Arc<dyn CommentGateway>>,
    ai_analyzer: Option<Arc<dyn AiAnalyzer>>,
    content_repo: Option<Arc<dyn ContentRepository>>,
    prompt_repo: Option<Arc<dyn PromptRepository>>,
    progress_tracker: Option<Arc<dyn ProgressTracker>>,
    strategies: HashMap<String, Arc<dyn PlatformStrategy>>,
    config: OrchestratorConfig,
}

impl OrchestratorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            content_gateway: None,
            comment_gateway: None,
            ai_analyzer: None,
            content_repo: None,
            prompt_repo: None,
            progress_tracker: None,
            strategies: HashMap::new(),
            config: OrchestratorConfig::default(),
        }
    }

    /// Set the content gateway
    pub fn content_gateway(mut self, gateway: Arc<dyn ContentGateway>) -> Self {
        self.content_gateway = Some(gateway);
        self
    }

    /// Set the comment gateway
    pub fn comment_gateway(mut self, gateway: Arc<dyn CommentGateway>) -> Self {
        self.comment_gateway = Some(gateway);
        self
    }

    /// Set the AI analyzer
    pub fn ai_analyzer(mut self, analyzer: Arc<dyn AiAnalyzer>) -> Self {
        self.ai_analyzer = Some(analyzer);
        self
    }

    /// Set the content repository
    pub fn content_repository(mut self, repo: Arc<dyn ContentRepository>) -> Self {
        self.content_repo = Some(repo);
        self
    }

    /// Set the prompt repository
    pub fn prompt_repository(mut self, repo: Arc<dyn PromptRepository>) -> Self {
        self.prompt_repo = Some(repo);
        self
    }

    /// Set the progress tracker
    pub fn progress_tracker(mut self, tracker: Arc<dyn ProgressTracker>) -> Self {
        self.progress_tracker = Some(tracker);
        self
    }

    /// Add a platform strategy
    pub fn add_strategy(mut self, strategy: Arc<dyn PlatformStrategy>) -> Self {
        self.strategies.insert(strategy.name().to_string(), strategy);
        self
    }

    /// Set the configuration
    pub fn config(mut self, config: OrchestratorConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the orchestrator
    pub fn build(self) -> Result<WorkflowOrchestrator, &'static str> {
        Ok(WorkflowOrchestrator {
            content_gateway: self.content_gateway.ok_or("content_gateway is required")?,
            comment_gateway: self.comment_gateway.ok_or("comment_gateway is required")?,
            ai_analyzer: self.ai_analyzer.ok_or("ai_analyzer is required")?,
            content_repo: self.content_repo.ok_or("content_repository is required")?,
            prompt_repo: self.prompt_repo.ok_or("prompt_repository is required")?,
            progress_tracker: self.progress_tracker.ok_or("progress_tracker is required")?,
            strategies: self.strategies,
            config: self.config,
        })
    }
}

impl Default for OrchestratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowOrchestrator {
    /// Create a new builder
    pub fn builder() -> OrchestratorBuilder {
        OrchestratorBuilder::new()
    }

    /// Process a crawler task
    pub async fn process_task(&self, task_id: i64, task_config: TaskConfig) -> WorkflowResult<TaskResult> {
        let start_time = Instant::now();
        info!(task_id, platform = %task_config.platform, "Starting task processing");

        // Get the platform strategy
        let strategy = self.strategies
            .get(&task_config.platform)
            .ok_or_else(|| WorkflowError::UnsupportedPlatform(task_config.platform.clone()))?;

        // Update task status to running
        self.progress_tracker
            .update_task_status(task_id, TaskStatus::Running)
            .await
            .map_err(WorkflowError::Database)?;

        // Get analysis context from campaign
        let analysis_context = self.prompt_repo
            .get_analysis_context(task_config.campaign_id)
            .await
            .map_err(WorkflowError::Database)?
            .unwrap_or_default();

        // Process each keyword
        let mut total_contents = 0;
        let mut total_comments = 0;
        let mut total_analyses = 0;
        let mut last_error: Option<WorkflowError> = None;

        let keywords = task_config.keywords.clone();
        let total_keywords = keywords.len();

        for (i, keyword) in keywords.iter().enumerate() {
            // Check if we should stop (campaign stopped or task cancelled)
            if self.progress_tracker.should_stop(task_id).await.map_err(WorkflowError::Database)? {
                info!(task_id, "Task stop requested, stopping early");
                break;
            }

            // Log progress (percentage through keywords)
            let progress_pct = ((i as f32 / total_keywords as f32) * 100.0) as i32;
            debug!(task_id, progress = progress_pct, keyword = %keyword, "Processing keyword");

            // Parse and process the keyword
            let parsed_keyword = strategy.parse_keyword(keyword);

            match self.process_keyword(
                task_id,
                &task_config,
                strategy.as_ref(),
                &parsed_keyword,
                &analysis_context,
            ).await {
                Ok((contents, comments, analyses)) => {
                    total_contents += contents;
                    total_comments += comments;
                    total_analyses += analyses;
                }
                Err(e) => {
                    error!(task_id, keyword = %keyword, error = %e, "Error processing keyword");
                    last_error = Some(e);
                    if !self.config.continue_on_error {
                        break;
                    }
                }
            }
        }

        // Build result
        let duration_ms = start_time.elapsed().as_millis() as u64;
        let result = if let Some(error) = last_error {
            if total_contents == 0 && total_comments == 0 {
                // Complete failure
                self.progress_tracker
                    .fail_task(task_id, &error.to_string())
                    .await
                    .ok();
                TaskResult::failure(task_id, error.to_string())
            } else {
                // Partial success
                self.progress_tracker.complete_task(task_id).await.ok();
                TaskResult::success(task_id)
                    .with_counts(total_contents, total_comments, total_analyses)
            }
        } else {
            // Complete success
            self.progress_tracker.complete_task(task_id).await.ok();
            TaskResult::success(task_id)
                .with_counts(total_contents, total_comments, total_analyses)
        };

        let result = result.with_duration(duration_ms);
        info!(
            task_id,
            success = result.success,
            contents = result.contents_processed,
            comments = result.comments_processed,
            analyses = result.analyses_generated,
            duration_ms,
            "Task processing completed"
        );

        Ok(result)
    }

    /// Process a single keyword
    /// 
    /// Videos are processed in parallel using Tokio async tasks (similar to Go goroutines).
    /// Concurrency is controlled by `max_concurrent_videos` in the task config.
    async fn process_keyword(
        &self,
        task_id: i64,
        config: &TaskConfig,
        strategy: &dyn PlatformStrategy,
        keyword: &KeywordType,
        analysis_context: &AnalysisContext,
    ) -> WorkflowResult<(i32, i32, i32)> {
        // Fetch content based on keyword type
        let contents = self.fetch_content(config, strategy, keyword).await?;
        info!(task_id, keyword = %keyword.value(), count = contents.len(), "Fetched content");
        
        // Check if search returned zero results (matching Python agent behavior)
        // For keyword searches, empty results trigger campaign end
        if contents.is_empty() {
            warn!(task_id, keyword = %keyword.value(), "Keyword search returned zero results");
            
            // Only trigger campaign stop for keyword/hashtag searches (matching Python agent)
            // User profile fetches just skip silently
            if matches!(keyword, KeywordType::Search(_) | KeywordType::Hashtag(_)) {
                info!(task_id, campaign_id = config.campaign_id, "🛑 Marking campaign as ENDED (search exhausted)");
                
                // Call stop_campaign_gracefully (matching Python agent's mark_campaign_as_ended)
                match self.progress_tracker.stop_campaign_gracefully(config.campaign_id).await {
                    Ok(result) => {
                        if result.success {
                            if result.immediate_stopped {
                                info!(
                                    task_id,
                                    campaign_id = config.campaign_id,
                                    refunded_amount = result.refunded_amount,
                                    "✅ Campaign marked as STOPPED, refund processed"
                                );
                            } else {
                                info!(
                                    task_id,
                                    campaign_id = config.campaign_id,
                                    "✅ Campaign marked as STOPPING (waiting for active tasks)"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(task_id, campaign_id = config.campaign_id, error = %e, "Failed to stop campaign gracefully");
                    }
                }
            }
            
            // Return early with zero counts
            return Ok((0, 0, 0));
        }

        // Process videos in parallel using buffer_unordered
        let concurrency = config.effective_concurrency();
        let max_concurrent_videos = concurrency.max_concurrent_videos;
        
        info!(
            task_id,
            keyword = %keyword.value(),
            video_count = contents.len(),
            max_concurrent_videos,
            "Processing videos with parallel execution"
        );

        // Shared stop flag for cancelling remaining work
        let stop_flag = Arc::new(AtomicBool::new(false));
        
        // Process videos concurrently
        let results: Vec<_> = stream::iter(contents.into_iter().enumerate())
            .map(|(idx, content)| {
                let config = config.clone();
                let ctx = analysis_context.clone();
                let stop_flag = stop_flag.clone();
                
                async move {
                    // Check stop flag before processing
                    if stop_flag.load(Ordering::Relaxed) {
                        debug!(task_id, content_idx = idx, "Skipping video due to stop signal");
                        return VideoProcessResult::Skipped;
                    }
                    
                    // Check database stop signal
                    match self.progress_tracker.should_stop(task_id).await {
                        Ok(true) => {
                            info!(task_id, content_idx = idx, "Stopping due to campaign stop signal");
                            stop_flag.store(true, Ordering::Relaxed);
                            return VideoProcessResult::Stopped;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!(task_id, error = %e, "Failed to check stop status");
                        }
                    }
                    
                    // Process the video
                    match self.process_content(task_id, &config, strategy, &content, &ctx).await {
                        Ok((is_new, comments, analyses)) => {
                            VideoProcessResult::Success {
                                content_id: content.content_id.clone(),
                                is_new,
                                comments,
                                analyses,
                            }
                        }
                        Err(e) => {
                            VideoProcessResult::Error {
                                content_id: content.content_id.clone(),
                                error: e,
                            }
                        }
                    }
                }
            })
            .buffer_unordered(max_concurrent_videos)
            .collect()
            .await;
        
        // Aggregate results and update progress sequentially
        // (Progress updates must be sequential to maintain correct counts)
        let mut contents_count = 0;
        let mut comments_count = 0;
        let mut analyses_count = 0;
        let mut stopped = false;
        
        for result in results {
            match result {
                VideoProcessResult::Success { content_id, is_new, comments, analyses } => {
                    if is_new {
                        contents_count += 1;
                        
                        // Update progress for new videos
                        match self.progress_tracker.update_task_progress(task_id, 1).await {
                            Ok(progress_update) => {
                                debug!(
                                    task_id,
                                    content_id = %content_id,
                                    new_process_count = progress_update.new_process_count,
                                    new_consumption = progress_update.new_actual_consumption,
                                    "Task progress updated"
                                );
                                
                                if progress_update.should_stop {
                                    warn!(task_id, "⚠️ Campaign is STOPPING");
                                    stopped = true;
                                }
                            }
                            Err(e) => {
                                warn!(task_id, error = %e, "Failed to update task progress");
                            }
                        }
                    }
                    comments_count += comments;
                    analyses_count += analyses;
                }
                VideoProcessResult::Error { content_id, error } => {
                    warn!(
                        task_id,
                        content_id = %content_id,
                        error = %error,
                        "Error processing content"
                    );
                    if !self.config.continue_on_error {
                        return Err(error);
                    }
                }
                VideoProcessResult::Stopped | VideoProcessResult::Skipped => {
                    stopped = true;
                }
            }
        }
        
        if stopped {
            info!(task_id, keyword = %keyword.value(), "Video processing stopped early");
        }

        Ok((contents_count, comments_count, analyses_count))
    }

    /// Fetch content based on keyword type
    async fn fetch_content(
        &self,
        config: &TaskConfig,
        strategy: &dyn PlatformStrategy,
        keyword: &KeywordType,
    ) -> WorkflowResult<Vec<Content>> {
        let search_options = strategy.build_search_options(config, keyword);

        let contents = match keyword {
            KeywordType::UserId(user_id) | KeywordType::SecUserId(user_id) => {
                self.content_gateway
                    .fetch_user_content(user_id, search_options.count)
                    .await
                    .map_err(WorkflowError::Gateway)?
            }
            KeywordType::ContentId(content_id) => {
                match self.content_gateway.fetch_by_id(content_id).await {
                    Ok(Some(content)) => vec![content],
                    Ok(None) => {
                        warn!(content_id = %content_id, "Content not found");
                        vec![]
                    }
                    Err(e) => return Err(WorkflowError::Gateway(e)),
                }
            }
            KeywordType::Search(_) | KeywordType::Hashtag(_) => {
                self.content_gateway
                    .search(&search_options)
                    .await
                    .map_err(WorkflowError::Gateway)?
            }
        };

        Ok(contents)
    }

    /// Process a single content item (video/post)
    /// 
    /// Returns (is_new, comments_saved, analyses_count) matching Python agent behavior:
    /// - is_new: Whether this was a new video (for progress tracking)
    /// - comments_saved: Number of comments with AI suggestions saved
    /// - analyses_count: Number of AI analyses generated
    async fn process_content(
        &self,
        task_id: i64,
        config: &TaskConfig,
        strategy: &dyn PlatformStrategy,
        content: &Content,
        analysis_context: &AnalysisContext,
    ) -> WorkflowResult<(bool, i32, i32)> {
        // ========== Step 1: Save video metadata ==========
        // Save content with ON CONFLICT - database unique constraint (task_id, video_id) handles duplicates
        // Returns is_new flag based on whether INSERT or UPDATE occurred
        let save_result = self.content_repo
            .save_content(content, Some(config.campaign_id), Some(task_id as i32))
            .await
            .map_err(WorkflowError::Database)?;
        
        let content_db_id = save_result.id;
        let is_new = save_result.is_new;
        
        if is_new {
            info!(
                task_id,
                content_id = %content.content_id,
                db_id = content_db_id,
                "Saved new video"
            );
        } else {
            // Video already exists (ON CONFLICT triggered UPDATE), skip comments and AI processing
            debug!(
                task_id,
                content_id = %content.content_id,
                db_id = content_db_id,
                "Video already exists, skipping comments and AI processing"
            );
            return Ok((false, 0, 0));
        }

        // ========== Step 2: Fetch comments (non-critical) ==========
        let max_comments = config
            .max_comments_per_video
            .map(|m| m as u32)
            .unwrap_or(self.config.max_comments_per_video);

        let comments = match self.comment_gateway
            .fetch_all_comments(&content.content_id, max_comments)
            .await
        {
            Ok(comments) => comments,
            Err(e) => {
                // Comment fetch failed but video is saved, count not affected (matching Python agent)
                warn!(
                    task_id,
                    content_id = %content.content_id,
                    error = %e,
                    "Failed to fetch comments, skipping AI analysis"
                );
                return Ok((true, 0, 0));  // is_new=true, but no comments processed
            }
        };

        debug!(
            task_id,
            content_id = %content.content_id,
            count = comments.len(),
            "Fetched comments"
        );
        
        // Check minimum comment count (matching Python agent: "if len(comments_raw) < 5: continue")
        if comments.len() < 5 {
            info!(
                task_id,
                content_id = %content.content_id,
                count = comments.len(),
                "Insufficient comments (< 5), skipping AI analysis"
            );
            return Ok((true, 0, 0));  // is_new=true, but no AI analysis needed
        }

        // Create comment lookup map (matching Python agent)
        let comment_map: std::collections::HashMap<String, Comment> = comments
            .iter()
            .map(|c| (c.comment_id.clone(), c.clone()))
            .collect();

        // ========== Step 3: AI Analysis (non-critical, supports batching) ==========
        // Analyze comments in batches and save ONLY comments with AI suggestions
        // (matching Python agent's save_comments_and_analysis behavior)
        let mut analyses_count = 0;
        let mut comments_saved = 0;
        
        for chunk in comments.chunks(self.config.ai_batch_size) {
            let comments_to_analyze: Vec<Comment> = chunk.to_vec();

            match self.ai_analyzer
                .analyze_batch(&comments_to_analyze, content, analysis_context)
                .await
            {
                Ok(suggestions) => {
                    // ========== Step 4: Save comments with AI analysis (non-critical) ==========
                    // Save only comments that have AI suggestions (matching Python agent)
                    for suggestion in &suggestions {
                        // Match suggestion to original comment by comment_id
                        if let Some(comment) = comment_map.get(&suggestion.comment_id) {
                            // Save comment with analysis in one operation
                            match self.content_repo
                                .save_comment_with_analysis(
                                    comment,
                                    content_db_id,
                                    config.campaign_id,
                                    suggestion,
                                )
                                .await
                            {
                                Ok(_) => {
                                    comments_saved += 1;
                                    analyses_count += 1;
                                }
                                Err(e) => {
                                    warn!(
                                        comment_id = %comment.comment_id,
                                        error = %e,
                                        "Failed to save comment with analysis"
                                    );
                                }
                            }
                        } else {
                            warn!(
                                suggestion_comment_id = %suggestion.comment_id,
                                "AI suggestion comment_id not found in original comments"
                            );
                        }
                    }
                }
                Err(e) => {
                    // AI failed but video saved, count not affected (matching Python agent)
                    warn!(
                        task_id,
                        content_id = %content.content_id,
                        error = %e,
                        "AI analysis failed for batch, continuing"
                    );
                    // Continue processing next batch
                }
            }
        }
        
        info!(
            task_id,
            content_id = %content.content_id,
            comments_saved,
            analyses_count,
            "Saved {} comments with AI analysis",
            comments_saved
        );

        Ok((is_new, comments_saved, analyses_count))
    }

    /// Get a strategy by platform name
    pub fn get_strategy(&self, platform: &str) -> Option<&Arc<dyn PlatformStrategy>> {
        self.strategies.get(platform)
    }

    /// List supported platforms
    pub fn supported_platforms(&self) -> Vec<&str> {
        self.strategies.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_config_defaults() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_videos_per_keyword, 10);
        assert_eq!(config.max_comments_per_video, 50);
        assert_eq!(config.ai_batch_size, 150);
        assert!(config.continue_on_error);
    }
}
