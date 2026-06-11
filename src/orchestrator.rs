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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures::stream::{self, StreamExt};
use tracing::{debug, error, info, warn};

use crate::domain::errors::{WorkflowError, WorkflowResult};
use crate::domain::{Comment, Content, KeywordType, TaskConfig, TaskResult};
use crate::ports::{
    ai_analyzer::AnalysisContext,
    content_gateway::{FetchOutcome, FetchShortfall},
    progress_tracker::{TaskStatus, TaskTerminalReason},
    AiAnalyzer, CommentGateway, ContentGateway, ContentRepository, ProgressTracker,
    PromptRepository,
};
use crate::strategies::{FacebookStrategy, PlatformStrategy};

/// Result of processing a single video
///
/// Used internally for parallel video processing to collect results
/// before aggregating counts and updating progress.
#[derive(Debug)]
struct KeywordProcessOutcome {
    contents: i32,
    comments: i32,
    analyses: i32,
    terminal_hint: Option<TaskTerminalReason>,
}

impl KeywordProcessOutcome {
    fn processed(contents: i32, comments: i32, analyses: i32) -> Self {
        Self {
            contents,
            comments,
            analyses,
            terminal_hint: None,
        }
    }

    fn no_more_possible_data() -> Self {
        Self {
            contents: 0,
            comments: 0,
            analyses: 0,
            terminal_hint: Some(TaskTerminalReason::no_more_possible_data()),
        }
    }

    /// 附加 shortfall 映射出的 terminal_hint(D3 六行映射;m1-pagination-core.md §2)。
    fn with_terminal_hint(mut self, hint: Option<TaskTerminalReason>) -> Self {
        self.terminal_hint = hint;
        self
    }
}

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
    /// Content gateways per platform (fetch videos/posts)
    content_gateways: HashMap<String, Arc<dyn ContentGateway>>,

    /// Comment gateways per platform (fetch comments)
    comment_gateways: HashMap<String, Arc<dyn CommentGateway>>,

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
    content_gateways: HashMap<String, Arc<dyn ContentGateway>>,
    comment_gateways: HashMap<String, Arc<dyn CommentGateway>>,
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
            content_gateways: HashMap::new(),
            comment_gateways: HashMap::new(),
            ai_analyzer: None,
            content_repo: None,
            prompt_repo: None,
            progress_tracker: None,
            strategies: HashMap::new(),
            config: OrchestratorConfig::default(),
        }
    }

    /// Add a content gateway for a specific platform
    pub fn add_content_gateway(
        mut self,
        platform: impl Into<String>,
        gateway: Arc<dyn ContentGateway>,
    ) -> Self {
        self.content_gateways.insert(platform.into(), gateway);
        self
    }

    /// Add a comment gateway for a specific platform
    pub fn add_comment_gateway(
        mut self,
        platform: impl Into<String>,
        gateway: Arc<dyn CommentGateway>,
    ) -> Self {
        self.comment_gateways.insert(platform.into(), gateway);
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
        self.strategies
            .insert(strategy.name().to_string(), strategy);
        self
    }

    /// Set the configuration
    pub fn config(mut self, config: OrchestratorConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the orchestrator
    pub fn build(self) -> Result<WorkflowOrchestrator, &'static str> {
        if self.content_gateways.is_empty() {
            return Err("at least one content_gateway is required");
        }
        if self.comment_gateways.is_empty() {
            return Err("at least one comment_gateway is required");
        }
        Ok(WorkflowOrchestrator {
            content_gateways: self.content_gateways,
            comment_gateways: self.comment_gateways,
            ai_analyzer: self.ai_analyzer.ok_or("ai_analyzer is required")?,
            content_repo: self.content_repo.ok_or("content_repository is required")?,
            prompt_repo: self.prompt_repo.ok_or("prompt_repository is required")?,
            progress_tracker: self
                .progress_tracker
                .ok_or("progress_tracker is required")?,
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

fn terminal_reason_for_error(error: &WorkflowError) -> TaskTerminalReason {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();

    if lower.contains("provider")
        || lower.contains("tikhub")
        || lower.contains("rate limit")
        || lower.contains("rate limited")
        || lower.contains("service unavailable")
        || lower.contains("429")
        || lower.contains("503")
        || lower.contains("payment")
        || lower.contains("unauthorized")
        || lower.contains("authentication failed")
        || lower.contains("auth")
        || matches!(
            error,
            WorkflowError::Gateway(crate::domain::errors::GatewayError::Network(_))
                | WorkflowError::Gateway(crate::domain::errors::GatewayError::Api { .. })
                | WorkflowError::Gateway(crate::domain::errors::GatewayError::RateLimited { .. })
                | WorkflowError::Gateway(crate::domain::errors::GatewayError::AuthFailed(_))
        )
    {
        TaskTerminalReason::provider_failure(text)
    } else if matches!(error, WorkflowError::Cancelled(_)) {
        TaskTerminalReason::cancelled(text)
    } else {
        TaskTerminalReason::internal_error(text)
    }
}

impl WorkflowOrchestrator {
    /// Create a new builder
    pub fn builder() -> OrchestratorBuilder {
        OrchestratorBuilder::new()
    }

    /// Process a crawler task
    pub async fn process_task(
        &self,
        task_id: i64,
        task_config: TaskConfig,
    ) -> WorkflowResult<TaskResult> {
        let start_time = Instant::now();
        info!(task_id, platform = %task_config.platform, "Starting task processing");

        // Get the platform strategy
        let strategy = self
            .strategies
            .get(&task_config.platform)
            .ok_or_else(|| WorkflowError::UnsupportedPlatform(task_config.platform.clone()))?;

        // Get platform-specific gateways
        let content_gateway = self
            .content_gateways
            .get(&task_config.platform)
            .ok_or_else(|| {
                WorkflowError::UnsupportedPlatform(format!(
                    "No content gateway for platform: {}",
                    task_config.platform
                ))
            })?;
        let comment_gateway = self
            .comment_gateways
            .get(&task_config.platform)
            .ok_or_else(|| {
                WorkflowError::UnsupportedPlatform(format!(
                    "No comment gateway for platform: {}",
                    task_config.platform
                ))
            })?;

        // Update task status to running
        self.progress_tracker
            .update_task_status(task_id, TaskStatus::Running)
            .await
            .map_err(WorkflowError::Database)?;

        // Get analysis context from campaign
        let analysis_context = self
            .prompt_repo
            .get_analysis_context(task_config.campaign_id)
            .await
            .map_err(WorkflowError::Database)?
            .unwrap_or_default();

        // Process each keyword
        let mut total_contents = 0;
        let mut total_comments = 0;
        let mut total_analyses = 0;
        let mut last_error: Option<WorkflowError> = None;
        let mut terminal_hint: Option<TaskTerminalReason> = None;

        let keywords = task_config.keywords.clone();
        let total_keywords = keywords.len();

        // D-13:任务级 max_count —— keyword 循环间累计已得 contents,
        // 后续 keyword 只取 remaining = max_videos − 累计(I-001 任务级)。
        let task_max_contents: Option<i64> = task_config.max_videos.map(|v| i64::from(v.max(0)));

        for (i, keyword) in keywords.iter().enumerate() {
            // D-13:remaining ≤ 0 时跳过剩余 keyword(任务总处理数 ≤ max_count)
            let remaining = task_max_contents.map(|max| max - i64::from(total_contents));
            if matches!(remaining, Some(r) if r <= 0) {
                info!(
                    task_id,
                    "Task-level max_videos quota reached, skipping remaining keywords"
                );
                break;
            }
            let remaining: Option<u32> = remaining.map(|r| u32::try_from(r).unwrap_or(u32::MAX));

            // Check if we should stop (campaign stopped or task cancelled)
            if self
                .progress_tracker
                .should_stop(task_id)
                .await
                .map_err(WorkflowError::Database)?
            {
                info!(task_id, "Task stop requested, stopping early");
                break;
            }

            // Log progress (percentage through keywords)
            let progress_pct = ((i as f32 / total_keywords as f32) * 100.0) as i32;
            debug!(task_id, progress = progress_pct, keyword = %keyword, "Processing keyword");

            // Parse and process the keyword
            let parsed_keyword = strategy.parse_keyword(keyword);

            match self
                .process_keyword(
                    task_id,
                    &task_config,
                    strategy.as_ref(),
                    &parsed_keyword,
                    &analysis_context,
                    content_gateway,
                    comment_gateway,
                    remaining,
                )
                .await
            {
                Ok(outcome) => {
                    total_contents += outcome.contents;
                    total_comments += outcome.comments;
                    total_analyses += outcome.analyses;
                    // DR-11 聚合优先级:PartialFailure 类 hint > Exhausted 类 hint
                    // (部分失败不得被枯竭标签掩盖,B2);其余沿 last-Some-wins 现状
                    // (None 不覆盖 Some)。code 字符串 = C-004 跨服务契约(M1-T6 pin)。
                    if let Some(hint) = outcome.terminal_hint {
                        let current_partial_wins = matches!(
                            terminal_hint.as_ref(),
                            Some(current)
                                if current.code == "COMPLETED_WITH_PARTIAL_ERRORS"
                                    && hint.code != "COMPLETED_WITH_PARTIAL_ERRORS"
                        );
                        if !current_partial_wins {
                            terminal_hint = Some(hint);
                        }
                    }
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
            let error_text = error.to_string();
            if total_contents == 0 && total_comments == 0 {
                // Complete failure
                let terminal_reason = terminal_reason_for_error(&error);
                self.progress_tracker
                    .fail_task(task_id, &error_text, &terminal_reason)
                    .await
                    .ok();
                TaskResult::failure(task_id, error_text).with_terminal_reason(terminal_reason)
            } else {
                // Partial success
                let terminal_reason = TaskTerminalReason::completed_with_partial_errors(error_text);
                self.progress_tracker
                    .complete_task(task_id, &terminal_reason)
                    .await
                    .ok();
                TaskResult::success(task_id)
                    .with_counts(total_contents, total_comments, total_analyses)
                    .with_terminal_reason(terminal_reason)
            }
        } else {
            // Complete success
            let terminal_reason = terminal_hint.unwrap_or_else(TaskTerminalReason::completed);
            self.progress_tracker
                .complete_task(task_id, &terminal_reason)
                .await
                .ok();
            TaskResult::success(task_id)
                .with_counts(total_contents, total_comments, total_analyses)
                .with_terminal_reason(terminal_reason)
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
    #[allow(clippy::too_many_arguments)]
    async fn process_keyword(
        &self,
        task_id: i64,
        config: &TaskConfig,
        strategy: &dyn PlatformStrategy,
        keyword: &KeywordType,
        analysis_context: &AnalysisContext,
        content_gateway: &Arc<dyn ContentGateway>,
        comment_gateway: &Arc<dyn CommentGateway>,
        remaining: Option<u32>,
    ) -> WorkflowResult<KeywordProcessOutcome> {
        // Fetch content based on keyword type (D3:经 FetchOutcome 携带欠交付原因)
        let FetchOutcome {
            contents,
            shortfall,
        } = self
            .fetch_content(config, strategy, keyword, content_gateway, remaining)
            .await?;
        info!(task_id, keyword = %keyword.value(), count = contents.len(), "Fetched content");

        // Check if search returned zero results (matching Python agent behavior)
        // For keyword searches, empty results trigger campaign end
        if contents.is_empty() {
            // D3 第六行(DR-01b 防御):「空 contents + PartialFailure」违例形状
            // 不得当作正常 partial —— 按零进展错误路径 fail_task(F-001 语义)。
            if let Some(FetchShortfall::PartialFailure { message }) = &shortfall {
                error!(
                    task_id,
                    keyword = %keyword.value(),
                    "Invalid FetchOutcome: empty contents with PartialFailure shortfall"
                );
                return Err(WorkflowError::InvalidTask(format!(
                    "invalid FetchOutcome: empty contents with PartialFailure shortfall ({message})"
                )));
            }

            warn!(task_id, keyword = %keyword.value(), "Keyword search returned zero results");

            // Only trigger campaign stop for keyword/hashtag searches (matching Python agent)
            // User profile fetches just skip silently
            if matches!(keyword, KeywordType::Search(_) | KeywordType::Hashtag(_)) {
                info!(
                    task_id,
                    campaign_id = config.campaign_id,
                    "🛑 Marking campaign as ENDED (search exhausted)"
                );

                // Call stop_campaign_gracefully (matching Python agent's mark_campaign_as_ended)
                match self
                    .progress_tracker
                    .stop_campaign_gracefully(config.campaign_id)
                    .await
                {
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
            return Ok(KeywordProcessOutcome::no_more_possible_data());
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
        let comment_gateway = comment_gateway.clone();
        let results: Vec<_> = stream::iter(contents.into_iter().enumerate())
            .map(|(idx, content)| {
                let config = config.clone();
                let ctx = analysis_context.clone();
                let stop_flag = stop_flag.clone();
                let comment_gateway = comment_gateway.clone();

                async move {
                    // Check stop flag before processing
                    if stop_flag.load(Ordering::Relaxed) {
                        debug!(
                            task_id,
                            content_idx = idx,
                            "Skipping video due to stop signal"
                        );
                        return VideoProcessResult::Skipped;
                    }

                    // Check database stop signal
                    match self.progress_tracker.should_stop(task_id).await {
                        Ok(true) => {
                            info!(
                                task_id,
                                content_idx = idx,
                                "Stopping due to campaign stop signal"
                            );
                            stop_flag.store(true, Ordering::Relaxed);
                            return VideoProcessResult::Stopped;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!(task_id, error = %e, "Failed to check stop status");
                        }
                    }

                    // Process the video
                    match self
                        .process_content(
                            task_id,
                            &config,
                            strategy,
                            &content,
                            &ctx,
                            &comment_gateway,
                        )
                        .await
                    {
                        Ok((is_new, comments, analyses)) => VideoProcessResult::Success {
                            content_id: content.content_id.clone(),
                            is_new,
                            comments,
                            analyses,
                        },
                        Err(e) => VideoProcessResult::Error {
                            content_id: content.content_id.clone(),
                            error: e,
                        },
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
                VideoProcessResult::Success {
                    content_id,
                    is_new,
                    comments,
                    analyses,
                } => {
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

        // D3 终态映射(contents 非空;m1-pagination-core.md §2 D3):
        // - None → 现状 COMPLETED 路径(terminal_hint 不设);
        // - Exhausted → NO_MORE_POSSIBLE_DATA(只记 terminal_reason,
        //   不调 stop_campaign_gracefully —— campaign 级完结交 M6,D3 设计裁决);
        // - PartialFailure → COMPLETED_WITH_PARTIAL_ERRORS(message 必经
        //   TaskTerminalReason 构造器脱敏,I-009;不得手拼字符串)。
        let terminal_hint = match shortfall {
            None => None,
            Some(FetchShortfall::Exhausted) => Some(TaskTerminalReason::no_more_possible_data()),
            Some(FetchShortfall::PartialFailure { message }) => {
                Some(TaskTerminalReason::completed_with_partial_errors(message))
            }
        };

        Ok(
            KeywordProcessOutcome::processed(contents_count, comments_count, analyses_count)
                .with_terminal_hint(terminal_hint),
        )
    }

    /// Fetch content based on keyword type
    ///
    /// 返回 `FetchOutcome`(D3:contents + 欠交付原因);`remaining` 为 D-13 任务级
    /// 剩余配额 —— 只下压、不抬高 strategy 的 count(保持平台 clamp 不变)。
    async fn fetch_content(
        &self,
        config: &TaskConfig,
        strategy: &dyn PlatformStrategy,
        keyword: &KeywordType,
        content_gateway: &Arc<dyn ContentGateway>,
        remaining: Option<u32>,
    ) -> WorkflowResult<FetchOutcome> {
        let mut search_options = strategy.build_search_options(config, keyword);
        if let Some(remaining) = remaining {
            search_options.count = search_options.count.min(remaining);
        }
        let outcome = content_gateway
            .fetch_by_keyword_with_outcome(keyword, &search_options)
            .await
            .map_err(WorkflowError::Gateway)?;

        Ok(outcome)
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
        _strategy: &dyn PlatformStrategy,
        content: &Content,
        analysis_context: &AnalysisContext,
        comment_gateway: &Arc<dyn CommentGateway>,
    ) -> WorkflowResult<(bool, i32, i32)> {
        // ========== Step 1: Save video metadata ==========
        // Save content with ON CONFLICT - database unique constraint (task_id, video_id) handles duplicates
        // Returns is_new flag based on whether INSERT or UPDATE occurred
        let save_result = self
            .content_repo
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

        let comment_lookup_id = Self::resolve_comment_lookup_id(content);

        let comments = match comment_gateway
            .fetch_all_comments(&comment_lookup_id, max_comments)
            .await
        {
            Ok(comments) => comments,
            Err(e) => {
                // Comment fetch failed but video is saved, count not affected (matching Python agent)
                warn!(
                    task_id,
                    content_id = %content.content_id,
                    comment_lookup_id = %comment_lookup_id,
                    error = %e,
                    "Failed to fetch comments, skipping AI analysis"
                );
                return Ok((true, 0, 0)); // is_new=true, but no comments processed
            }
        };

        debug!(
            task_id,
            content_id = %content.content_id,
            comment_lookup_id = %comment_lookup_id,
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
            return Ok((true, 0, 0)); // is_new=true, but no AI analysis needed
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

            match self
                .ai_analyzer
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
                            match self
                                .content_repo
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

    fn resolve_comment_lookup_id(content: &Content) -> String {
        if content.platform != "facebook" {
            return content.content_id.clone();
        }

        let raw_lookup = content.raw_data.as_ref().and_then(|raw| {
            ["url", "attached_post_url"]
                .iter()
                .find_map(|key| raw.get(*key).and_then(|value| value.as_str()))
        });

        raw_lookup
            .and_then(FacebookStrategy::extract_post_lookup_id)
            .or_else(|| {
                content
                    .url
                    .as_deref()
                    .and_then(FacebookStrategy::extract_post_lookup_id)
            })
            .unwrap_or_else(|| content.content_id.clone())
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
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::ports::{
        progress_tracker::{TaskInfo, TaskStatus},
        prompt_repository::{CampaignConfig, CampaignStatus},
    };
    use crate::testing::{MockAiAnalyzer, MockCommentGateway, MockRepository};
    use crate::{Comment, Content, FacebookStrategy};

    struct StaticFacebookContentGateway {
        content: Content,
    }

    #[async_trait]
    impl crate::ContentGateway for StaticFacebookContentGateway {
        async fn search(
            &self,
            _options: &crate::SearchOptions,
        ) -> crate::GatewayResult<Vec<Content>> {
            Ok(vec![self.content.clone()])
        }

        async fn fetch_by_keyword(
            &self,
            _keyword: &crate::KeywordType,
            _options: &crate::SearchOptions,
        ) -> crate::GatewayResult<Vec<Content>> {
            Ok(vec![self.content.clone()])
        }

        async fn fetch_user_content(
            &self,
            _user_id: &str,
            _count: u32,
        ) -> crate::GatewayResult<Vec<Content>> {
            Ok(vec![self.content.clone()])
        }

        async fn fetch_by_id(&self, _content_id: &str) -> crate::GatewayResult<Option<Content>> {
            Ok(Some(self.content.clone()))
        }

        fn platform(&self) -> &str {
            "facebook"
        }
    }

    #[test]
    fn test_orchestrator_config_defaults() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_videos_per_keyword, 10);
        assert_eq!(config.max_comments_per_video, 50);
        assert_eq!(config.ai_batch_size, 150);
        assert!(config.continue_on_error);
    }

    #[tokio::test]
    async fn test_process_task_uses_facebook_url_lookup_for_comments() {
        let comment_gateway = Arc::new(MockCommentGateway::new());
        let ai_analyzer = Arc::new(MockAiAnalyzer::simple());
        let repository = Arc::new(MockRepository::new());

        repository.add_campaign(CampaignConfig {
            id: 98,
            user_id: 1,
            name: "facebook photo comments".to_string(),
            platform_id: 3,
            status: CampaignStatus::Active,
            target_audience: None,
            product_prompt: None,
            reply_strategy: Some("Be helpful".to_string()),
            dm_strategy: None,
            reply_post_strategy: None,
            max_comments: Some(10),
            processed_comments: 0,
        });
        repository.add_task(TaskInfo {
            id: 3835,
            campaign_id: 98,
            platform_id: 3,
            keywords: Some(json!(["facebook_post_url:https://www.facebook.com/photo?fbid=928322816583907&set=a.235444075871788"])),
            status: TaskStatus::Pending,
            progress: 0,
            error_message: None,
            terminal_reason: None,
        });

        let content_gateway = Arc::new(StaticFacebookContentGateway {
            content: Content::new(
                "facebook",
                "928322816583907",
            )
            .with_author("100082185911689")
            .with_author_name("Gossip Harbor")
            .with_description("photo post")
            .with_url("https://www.facebook.com/GossipHarbor/posts/pfbid023XrzksHBkgtAN1ErALXUUrtAAHTfj9A8r3kDG6PqB8777auXLE1BAhUE93A9bKwel")
            .with_raw_data(json!({
                "post_id": "928322816583907",
                "url": "https://www.facebook.com/GossipHarbor/posts/pfbid023XrzksHBkgtAN1ErALXUUrtAAHTfj9A8r3kDG6PqB8777auXLE1BAhUE93A9bKwel"
            })),
        });

        for index in 0..5 {
            comment_gateway.add_comment(
                "pfbid023XrzksHBkgtAN1ErALXUUrtAAHTfj9A8r3kDG6PqB8777auXLE1BAhUE93A9bKwel",
                Comment::new(
                    "facebook",
                    format!("87122560596616{index}"),
                    "pfbid023XrzksHBkgtAN1ErALXUUrtAAHTfj9A8r3kDG6PqB8777auXLE1BAhUE93A9bKwel",
                )
                .with_author(format!("user-{index}"))
                .with_text(format!("comment-{index}")),
            );
        }

        let orchestrator = WorkflowOrchestrator::builder()
            .add_content_gateway("facebook", content_gateway)
            .add_comment_gateway("facebook", comment_gateway.clone())
            .ai_analyzer(ai_analyzer)
            .content_repository(repository.clone())
            .prompt_repository(repository.clone())
            .progress_tracker(repository)
            .add_strategy(Arc::new(FacebookStrategy::new()))
            .build()
            .expect("orchestrator should build");

        let result = orchestrator
            .process_task(
                3835,
                TaskConfig::new(98, "facebook")
                    .with_keywords(vec![
                        "facebook_post_url:https://www.facebook.com/photo?fbid=928322816583907&set=a.235444075871788".to_string(),
                    ])
                    .with_max_videos(1)
                    .with_max_comments_per_video(5),
            )
            .await
            .expect("facebook photo task should complete");

        assert!(result.success);
        assert_eq!(result.contents_processed, 1);
        assert_eq!(
            result.comments_processed, 5,
            "facebook photo URLs should resolve the pfbid lookup id before fetching comments"
        );
        assert_eq!(result.analyses_generated, 5);

        let calls = comment_gateway.get_calls();
        assert!(matches!(
            calls.as_slice(),
            [crate::testing::mock_gateway::GatewayCall::FetchAllComments { content_id, max: 5 }]
                if content_id == "pfbid023XrzksHBkgtAN1ErALXUUrtAAHTfj9A8r3kDG6PqB8777auXLE1BAhUE93A9bKwel"
        ));
    }

    // ===== M1-T4 测试载荷(m1-pagination-core.md §2 D3 + §3 M1-T4;断言 = 计划原文契约) =====
    //
    // 装配:复用既有 MockRepository/MockCommentGateway/MockAiAnalyzer 样板;
    // gateway = M1-T3 的分页注入 MockContentGateway;strategy = TikTokStrategy
    // (其 `build_search_options` 不 clamp count:options.count = max_videos,
    //  使任务级 max_count 经 fetch 路径可观测;facebook strategy 会 clamp 到 20,不适用)。
    // `stop_campaign_gracefully` 调用计数经 CountingProgressTracker(委托 MockRepository,
    // 仅计数,不 mock 被测对象,AG-005 合规)。

    use std::sync::atomic::AtomicUsize;

    use crate::domain::errors::DbResult;
    use crate::ports::content_gateway::{FetchOutcome, FetchShortfall};
    use crate::ports::progress_tracker::{CampaignStopResult, TaskProgressUpdate};
    use crate::strategies::TikTokStrategy;
    use crate::testing::MockContentGateway;
    use crate::TaskResult;

    const M1T4_TASK_ID: i64 = 9100;
    const M1T4_CAMPAIGN_ID: i32 = 910;

    /// ProgressTracker 委托包装:全量转发 MockRepository,仅对
    /// `stop_campaign_gracefully` 计数(D3 设计裁决断言载体;M1-T4 测试 1/6)。
    struct CountingProgressTracker {
        inner: Arc<MockRepository>,
        stop_campaign_calls: AtomicUsize,
    }

    impl CountingProgressTracker {
        fn new(inner: Arc<MockRepository>) -> Self {
            Self {
                inner,
                stop_campaign_calls: AtomicUsize::new(0),
            }
        }

        fn stop_campaign_call_count(&self) -> usize {
            self.stop_campaign_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ProgressTracker for CountingProgressTracker {
        async fn get_task(&self, task_id: i64) -> DbResult<Option<TaskInfo>> {
            self.inner.get_task(task_id).await
        }

        async fn update_task_status(&self, task_id: i64, status: TaskStatus) -> DbResult<()> {
            self.inner.update_task_status(task_id, status).await
        }

        async fn update_task_progress(
            &self,
            task_id: i64,
            increment: i32,
        ) -> DbResult<TaskProgressUpdate> {
            self.inner.update_task_progress(task_id, increment).await
        }

        async fn set_task_error(
            &self,
            task_id: i64,
            error: &str,
            terminal_reason: &TaskTerminalReason,
        ) -> DbResult<()> {
            self.inner
                .set_task_error(task_id, error, terminal_reason)
                .await
        }

        async fn complete_task(
            &self,
            task_id: i64,
            terminal_reason: &TaskTerminalReason,
        ) -> DbResult<()> {
            self.inner.complete_task(task_id, terminal_reason).await
        }

        async fn fail_task(
            &self,
            task_id: i64,
            error: &str,
            terminal_reason: &TaskTerminalReason,
        ) -> DbResult<()> {
            self.inner.fail_task(task_id, error, terminal_reason).await
        }

        async fn should_stop(&self, task_id: i64) -> DbResult<bool> {
            self.inner.should_stop(task_id).await
        }

        async fn increment_processed(&self, campaign_id: i32, count: i32) -> DbResult<()> {
            self.inner.increment_processed(campaign_id, count).await
        }

        async fn get_processed_count(&self, campaign_id: i32) -> DbResult<i32> {
            self.inner.get_processed_count(campaign_id).await
        }

        async fn stop_campaign_gracefully(&self, campaign_id: i32) -> DbResult<CampaignStopResult> {
            self.stop_campaign_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.stop_campaign_gracefully(campaign_id).await
        }
    }

    /// 生成 `n` 条 content_id 互异的测试 Content(沿 mock_gateway.rs M1-T3 样板)。
    fn page(prefix: &str, start: usize, n: usize) -> Vec<Content> {
        (start..start + n)
            .map(|i| Content::new("mock", format!("{prefix}{i}")))
            .collect()
    }

    struct PaginationHarness {
        gateway: Arc<MockContentGateway>,
        repo: Arc<MockRepository>,
        tracker: Arc<CountingProgressTracker>,
        orchestrator: WorkflowOrchestrator,
        config: TaskConfig,
    }

    impl PaginationHarness {
        fn new(keywords: &[&str], max_videos: i32) -> Self {
            Self::for_platform(
                "tiktok",
                2,
                Arc::new(TikTokStrategy::new()),
                keywords,
                max_videos,
            )
        }

        /// M2-T4 facebook 形状装配(m2-facebook-p0.md §4 M2-T4):同一 MockContentGateway
        /// 分页注入,但 strategy = FacebookStrategy(M2-T1 解截断后 options.count =
        /// max_videos)、platform = "facebook"(platform_id 3)。override 不在测试路径
        /// (mock gateway 直接产 shortfall;边界澄清原文)。
        fn facebook(keywords: &[&str], max_videos: i32) -> Self {
            Self::for_platform(
                "facebook",
                3,
                Arc::new(FacebookStrategy::new()),
                keywords,
                max_videos,
            )
        }

        fn for_platform(
            platform: &'static str,
            platform_id: i32,
            strategy: Arc<dyn PlatformStrategy>,
            keywords: &[&str],
            max_videos: i32,
        ) -> Self {
            let gateway = Arc::new(MockContentGateway::new());
            let repo = Arc::new(MockRepository::new());
            let tracker = Arc::new(CountingProgressTracker::new(repo.clone()));

            repo.add_campaign(CampaignConfig {
                id: M1T4_CAMPAIGN_ID,
                user_id: 1,
                name: "pagination mapping harness".to_string(),
                platform_id,
                status: CampaignStatus::Active,
                target_audience: None,
                product_prompt: None,
                reply_strategy: None,
                dm_strategy: None,
                reply_post_strategy: None,
                max_comments: Some(100_000),
                processed_comments: 0,
            });
            repo.add_task(TaskInfo {
                id: M1T4_TASK_ID,
                campaign_id: M1T4_CAMPAIGN_ID,
                platform_id,
                keywords: Some(json!(keywords)),
                status: TaskStatus::Pending,
                progress: 0,
                error_message: None,
                terminal_reason: None,
            });

            let orchestrator = WorkflowOrchestrator::builder()
                .add_content_gateway(platform, gateway.clone())
                .add_comment_gateway(platform, Arc::new(MockCommentGateway::new()))
                .ai_analyzer(Arc::new(MockAiAnalyzer::simple()))
                .content_repository(repo.clone())
                .prompt_repository(repo.clone())
                .progress_tracker(tracker.clone())
                .add_strategy(strategy)
                .build()
                .expect("orchestrator should build");

            let config = TaskConfig::new(M1T4_CAMPAIGN_ID, platform)
                .with_keywords(keywords.iter().map(|k| k.to_string()).collect())
                .with_max_videos(max_videos);

            Self {
                gateway,
                repo,
                tracker,
                orchestrator,
                config,
            }
        }

        async fn run(&self) -> TaskResult {
            self.orchestrator
                .process_task(M1T4_TASK_ID, self.config.clone())
                .await
                .expect("process_task must return a TaskResult")
        }

        async fn task_info(&self) -> TaskInfo {
            self.repo
                .get_task(M1T4_TASK_ID)
                .await
                .unwrap()
                .expect("task must exist")
        }

        async fn terminal_reason(&self) -> String {
            self.task_info()
                .await
                .terminal_reason
                .expect("terminal_reason must be set after process_task")
        }
    }

    /// M1-T4 测试 1(T-002a / F-003):mock 注入 2 页(20+10)、max_videos=50 →
    /// terminal_reason 以 "NO_MORE_POSSIBLE_DATA" 开头、task completed、
    /// contents_processed==30,且 `stop_campaign_gracefully` 未被调用(D3 设计裁决:
    /// contents 非空 + Exhausted 只记 terminal_reason,campaign 级完结交 M6)。
    #[tokio::test]
    async fn exhausted_maps_to_no_more_possible_data() {
        let h = PaginationHarness::new(&["kw"], 50);
        h.gateway
            .add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("NO_MORE_POSSIBLE_DATA"),
            "terminal_reason must start with \"NO_MORE_POSSIBLE_DATA\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
        assert_eq!(result.contents_processed, 30);
        assert_eq!(
            h.tracker.stop_campaign_call_count(),
            0,
            "contents 非空 + Exhausted 不得调用 stop_campaign_gracefully(D3 设计裁决)"
        );
    }

    /// M1-T4 测试 2(T-002b / F-002 / I-005):第 2 页注入 RateLimit、第 1 页 20 条 →
    /// terminal_reason 以 "COMPLETED_WITH_PARTIAL_ERRORS" 开头、task completed、
    /// contents_processed==20(已落进展保留)。
    #[tokio::test]
    async fn partial_failure_maps_to_completed_with_partial_errors() {
        let h = PaginationHarness::new(&["kw"], 50);
        h.gateway
            .add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        h.gateway
            .set_page_error_at(1, crate::testing::mock_gateway::MockError::RateLimit);

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("COMPLETED_WITH_PARTIAL_ERRORS"),
            "terminal_reason must start with \"COMPLETED_WITH_PARTIAL_ERRORS\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
        assert_eq!(result.contents_processed, 20);
    }

    /// M1-T4 测试 3(T-002c):3 页喂满 50 → terminal_reason 以 "COMPLETED" 开头
    /// 且不含 "PARTIAL"、不含 "NO_MORE"。
    #[tokio::test]
    async fn full_delivery_maps_to_completed() {
        let h = PaginationHarness::new(&["kw"], 50);
        h.gateway.add_search_pages(
            "kw",
            vec![page("a", 0, 20), page("a", 20, 20), page("a", 40, 20)],
        );

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("COMPLETED"),
            "terminal_reason must start with \"COMPLETED\", got {reason:?}"
        );
        assert!(
            !reason.contains("PARTIAL"),
            "terminal_reason must not contain \"PARTIAL\", got {reason:?}"
        );
        assert!(
            !reason.contains("NO_MORE"),
            "terminal_reason must not contain \"NO_MORE\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
        assert_eq!(result.contents_processed, 50);
    }

    /// M1-T4 测试 4(F-001 / R-008;**允许先绿**,AG-006 变异证明):第 1 页即
    /// 不可恢复错误 → task failed、terminal_reason 以 "PROVIDER_FAILURE" 开头
    /// (既有零进展失败路径回归保护)。
    ///
    /// 注入双轨编码同一语义「第 1 页即不可恢复错误」:
    /// - `set_error_mode(Network)`:RED 期 orchestrator 走旧 `fetch_by_keyword` 时报错;
    /// - `set_page_error_at(0, Network)`:GREEN 期 outcome 路径在第 0 页(零进展)报错。
    #[tokio::test]
    async fn zero_progress_failure_maps_to_failed() {
        let h = PaginationHarness::new(&["kw"], 50);
        h.gateway
            .add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        h.gateway
            .set_page_error_at(0, crate::testing::mock_gateway::MockError::Network);
        h.gateway
            .set_error_mode(Some(crate::testing::mock_gateway::MockError::Network));

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert_eq!(h.task_info().await.status, TaskStatus::Failed);
        assert!(
            reason.starts_with("PROVIDER_FAILURE"),
            "terminal_reason must start with \"PROVIDER_FAILURE\", got {reason:?}"
        );
        assert!(!result.success);
    }

    /// M1-T4 测试 5(T-004 / I-009):页失败 message 含 `api_key=sk-test-secret-123` →
    /// terminal_reason 含 "[REDACTED]"、不含明文(新 partial 路径必须走
    /// `TaskTerminalReason` 构造器,不得手拼字符串绕过脱敏)。
    /// 注入经 `set_raw_outcome`(合法 partial 形状:contents 非空)。
    #[tokio::test]
    async fn partial_failure_message_is_redacted() {
        let h = PaginationHarness::new(&["kw"], 50);
        h.gateway.set_raw_outcome(
            "kw",
            FetchOutcome {
                contents: page("a", 0, 20),
                shortfall: Some(FetchShortfall::PartialFailure {
                    message: "page 2 failed: api_key=sk-test-secret-123 rate limited".to_string(),
                }),
            },
        );

        let _result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.contains("[REDACTED]"),
            "terminal_reason must contain \"[REDACTED]\", got {reason:?}"
        );
        assert!(
            !reason.contains("sk-test-secret-123"),
            "terminal_reason must not leak the plaintext secret, got {reason:?}"
        );
    }

    /// M1-T4 测试 6(现状回归;**允许先绿**,AG-006):零结果搜索 →
    /// `stop_campaign_gracefully` 被调用、terminal_reason 为 NO_MORE_POSSIBLE_DATA。
    #[tokio::test]
    async fn empty_first_page_keeps_campaign_stop_behavior() {
        let h = PaginationHarness::new(&["kw"], 50);
        // 不注入任何页/结果 → 零结果搜索

        let _result = h.run().await;
        let reason = h.terminal_reason().await;

        assert_eq!(
            h.tracker.stop_campaign_call_count(),
            1,
            "零结果搜索必须调用 stop_campaign_gracefully(既有 campaign stop 行为)"
        );
        assert!(
            reason.starts_with("NO_MORE_POSSIBLE_DATA"),
            "terminal_reason must start with \"NO_MORE_POSSIBLE_DATA\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
    }

    /// M1-T4 测试 7(DR-11 聚合优先级):K=2,kw1=PartialFailure、kw2=Exhausted →
    /// terminal_reason 以 "COMPLETED_WITH_PARTIAL_ERRORS" 开头
    /// (D3 聚合优先级:PartialFailure > Exhausted,B2;None 仍不覆盖 Some)。
    ///
    /// RED 两段式预言(计划 §3 M1-T4 原文,以实跑输出为准记录):
    /// - 初始 RED(orchestrator 未消费 shortfall):got "COMPLETED: ...";
    /// - 映射接通后、优先级未实现的中途态:got "NO_MORE_POSSIBLE_DATA: ..."
    ///   (last-Some-wins 下 kw2 的 Exhausted 覆盖 kw1 的 PartialFailure)。
    #[tokio::test]
    async fn mixed_shortfall_partial_wins_over_exhausted() {
        let h = PaginationHarness::new(&["kw1", "kw2"], 50);
        // kw1 = PartialFailure 形状:2 页(20+10),第 2 页 RateLimit(第 1 页已有 20 条进展)
        h.gateway
            .add_search_pages("kw1", vec![page("a", 0, 20), page("a", 20, 10)]);
        h.gateway
            .set_page_error_at(1, crate::testing::mock_gateway::MockError::RateLimit);
        // kw2 = Exhausted 形状:单页 5 条(idx 0,不触发 page_error_at(1)),
        // 末页 cursor=None 且远少于剩余配额 → 上游枯竭
        h.gateway.add_search_pages("kw2", vec![page("b", 0, 5)]);

        let _result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("COMPLETED_WITH_PARTIAL_ERRORS"),
            "PartialFailure must win over Exhausted in aggregation (DR-11), got {reason:?}"
        );
    }

    /// M1-T4 测试 8(D-13 任务级 max_count):K=2、max_videos=50、kw1 注入 30 条、
    /// kw2 注入 30(≥30)条 → 任务总处理 50 且 kw2 实取 20
    /// (remaining 跨 keyword 传递;I-001 任务级)。
    /// 预期 RED(现状每 keyword 独立 count=50):`left: 60, right: 50`。
    #[tokio::test]
    async fn two_keywords_share_task_level_max_count() {
        let h = PaginationHarness::new(&["kw1", "kw2"], 50);
        h.gateway
            .add_search_pages("kw1", vec![page("a", 0, 20), page("a", 20, 10)]);
        h.gateway
            .add_search_pages("kw2", vec![page("b", 0, 20), page("b", 20, 10)]);

        let result = h.run().await;

        assert_eq!(result.contents_processed, 50);
        let kw2_taken = h
            .repo
            .get_all_contents()
            .iter()
            .filter(|c| c.content_id.starts_with('b'))
            .count();
        assert_eq!(
            kw2_taken, 20,
            "kw2 must only take the remaining task-level quota (50 - 30 = 20)"
        );
    }

    /// M1-T4 测试 9(DR-01b 防御):mock 直接回放违例形状
    /// 「空 contents + Some(PartialFailure)」(字面构造绕过 `FetchOutcome::partial`
    /// 构造器)→ task failed(`fail_task` 零进展错误路径),terminal_reason
    /// **不**以 "COMPLETED_WITH_PARTIAL_ERRORS" 开头(D3 第六行防御性兜底)。
    #[tokio::test]
    async fn defensive_empty_contents_partial_failure_fails_task() {
        let h = PaginationHarness::new(&["kw"], 50);
        h.gateway.set_raw_outcome(
            "kw",
            FetchOutcome {
                contents: vec![],
                shortfall: Some(FetchShortfall::PartialFailure {
                    message: "upstream failed mid-pagination".to_string(),
                }),
            },
        );

        let _result = h.run().await;
        let info = h.task_info().await;

        assert_eq!(
            info.status,
            TaskStatus::Failed,
            "task is failed; got {:?} with {:?}",
            info.status,
            info.terminal_reason
        );
        let reason = info.terminal_reason.unwrap_or_default();
        assert!(
            !reason.starts_with("COMPLETED_WITH_PARTIAL_ERRORS"),
            "violating shape must not be treated as a normal partial, got {reason:?}"
        );
    }

    // ===== M2-T4 测试载荷(m2-facebook-p0.md §4 M2-T4;facebook 形状实例化) =====
    //
    // DR-13 定位:strategy 解截断(M2-T1)+ adapter shortfall(M2-T2)+ orchestrator
    // D3 映射(M1-T4)的端到端贯通证据。映射逻辑归 M1-T4,不在此重写;真实 adapter
    // override 不在测试路径(MockContentGateway 直接产 shortfall)。
    // RED 取证点 = pre-M2 基线(1047b98,facebook strategy 仍 v.min(20) 截断):
    // 测试 1/2/4 预期 got COMPLETED + contents_processed==20。

    /// M2-T4 测试 1 `facebook_exhausted_maps_no_more_possible_data`(T-016 / F-003):
    /// facebook strategy、2 页(20+10,枯竭)、max_videos=50 → terminal_reason 以
    /// "NO_MORE_POSSIBLE_DATA" 开头、task completed、contents_processed==30、
    /// `stop_campaign_gracefully` 未调用(D3 设计裁决,M1 §6.1;campaign 级完结交 M6)。
    #[tokio::test]
    async fn facebook_exhausted_maps_no_more_possible_data() {
        let h = PaginationHarness::facebook(&["kw"], 50);
        h.gateway
            .add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("NO_MORE_POSSIBLE_DATA"),
            "terminal_reason must start with \"NO_MORE_POSSIBLE_DATA\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
        assert_eq!(result.contents_processed, 30);
        assert_eq!(
            h.tracker.stop_campaign_call_count(),
            0,
            "contents 非空 + Exhausted 不得调用 stop_campaign_gracefully(D3 设计裁决)"
        );
    }

    /// M2-T4 测试 2 `facebook_page2_failure_maps_partial_errors`(T-015 有进展 / F-002):
    /// 第 1 页 20 条、第 2 页注入失败(RateLimit,DR-10 触发集)、max_videos=50 →
    /// terminal_reason 以 "COMPLETED_WITH_PARTIAL_ERRORS" 开头、task completed、
    /// contents_processed==20(已落进展保留)。
    #[tokio::test]
    async fn facebook_page2_failure_maps_partial_errors() {
        let h = PaginationHarness::facebook(&["kw"], 50);
        h.gateway
            .add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        h.gateway
            .set_page_error_at(1, crate::testing::mock_gateway::MockError::RateLimit);

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("COMPLETED_WITH_PARTIAL_ERRORS"),
            "terminal_reason must start with \"COMPLETED_WITH_PARTIAL_ERRORS\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
        assert_eq!(result.contents_processed, 20);
    }

    /// M2-T4 测试 3 `facebook_page1_failure_maps_failed`(T-015 零进展 / F-001 / R-008;
    /// **允许先绿**,AG-006 变异证明):第 1 页即不可恢复错误 → task **failed**、
    /// terminal_reason 以 "PROVIDER_FAILURE" 开头(既有失败路径回归)。
    /// 注入双轨编码同一语义(沿 M1-T4 测试 4 样板):page_error_at(0) 走 outcome
    /// 路径,error_mode 兜底 legacy 路径。
    #[tokio::test]
    async fn facebook_page1_failure_maps_failed() {
        let h = PaginationHarness::facebook(&["kw"], 50);
        h.gateway
            .add_search_pages("kw", vec![page("a", 0, 20), page("a", 20, 10)]);
        h.gateway
            .set_page_error_at(0, crate::testing::mock_gateway::MockError::Network);
        h.gateway
            .set_error_mode(Some(crate::testing::mock_gateway::MockError::Network));

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert_eq!(h.task_info().await.status, TaskStatus::Failed);
        assert!(
            reason.starts_with("PROVIDER_FAILURE"),
            "terminal_reason must start with \"PROVIDER_FAILURE\", got {reason:?}"
        );
        assert!(!result.success);
    }

    /// M2-T4 测试 4 `facebook_full_delivery_maps_completed`(达量基线):3 页
    /// (20+20+10)满 50 → terminal_reason 以 "COMPLETED" 开头且不含 "PARTIAL"、
    /// 不含 "NO_MORE"、contents_processed==50。
    #[tokio::test]
    async fn facebook_full_delivery_maps_completed() {
        let h = PaginationHarness::facebook(&["kw"], 50);
        h.gateway.add_search_pages(
            "kw",
            vec![page("a", 0, 20), page("a", 20, 20), page("a", 40, 10)],
        );

        let result = h.run().await;
        let reason = h.terminal_reason().await;

        assert!(
            reason.starts_with("COMPLETED"),
            "terminal_reason must start with \"COMPLETED\", got {reason:?}"
        );
        assert!(
            !reason.contains("PARTIAL"),
            "terminal_reason must not contain \"PARTIAL\", got {reason:?}"
        );
        assert!(
            !reason.contains("NO_MORE"),
            "terminal_reason must not contain \"NO_MORE\", got {reason:?}"
        );
        assert_eq!(h.task_info().await.status, TaskStatus::Completed);
        assert_eq!(result.contents_processed, 50);
    }
}
