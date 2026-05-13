//! Progress Tracker Port - Interface for tracking task progress

use async_trait::async_trait;

use crate::domain::errors::DbResult;

/// Result from task progress update (matching Python agent's fn_update_task_progress)
#[derive(Debug, Clone)]
pub struct TaskProgressUpdate {
    /// Whether the update was successful
    pub success: bool,
    /// Whether the campaign is stopping (should stop processing)
    pub should_stop: bool,
    /// New process count after update
    pub new_process_count: i32,
    /// New actual consumption after update
    pub new_actual_consumption: f64,
}

impl Default for TaskProgressUpdate {
    fn default() -> Self {
        Self {
            success: true,
            should_stop: false,
            new_process_count: 0,
            new_actual_consumption: 0.0,
        }
    }
}

/// Result from stopping campaign gracefully (matching Python agent's fn_stop_campaign_gracefully)
#[derive(Debug, Clone)]
pub struct CampaignStopResult {
    /// Whether the stop was successful
    pub success: bool,
    /// Whether the campaign was immediately stopped (vs STOPPING state)
    pub immediate_stopped: bool,
    /// Amount refunded to wallet
    pub refunded_amount: f64,
}

impl Default for CampaignStopResult {
    fn default() -> Self {
        Self {
            success: false,
            immediate_stopped: false,
            refunded_amount: 0.0,
        }
    }
}

/// Durable terminal reason for a task status transition.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskTerminalReason {
    pub code: String,
    pub message: String,
}

impl TaskTerminalReason {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: code.into(),
            message: redact_terminal_reason_message(&message),
        }
    }

    pub fn completed() -> Self {
        Self::new("COMPLETED", "Task completed successfully")
    }

    pub fn completed_with_partial_errors(error: impl AsRef<str>) -> Self {
        Self::new(
            "COMPLETED_WITH_PARTIAL_ERRORS",
            format!("Task completed with partial errors: {}", error.as_ref()),
        )
    }

    pub fn no_more_possible_data() -> Self {
        Self::new(
            "NO_MORE_POSSIBLE_DATA",
            "No more possible data for keyword search",
        )
    }

    pub fn provider_failure(error: impl AsRef<str>) -> Self {
        Self::new("PROVIDER_FAILURE", error.as_ref())
    }

    pub fn cancelled(message: impl AsRef<str>) -> Self {
        Self::new("CANCELLED", message.as_ref())
    }

    pub fn internal_error(error: impl AsRef<str>) -> Self {
        Self::new("INTERNAL_ERROR", error.as_ref())
    }

    pub fn as_terminal_message(&self) -> String {
        format!("{}: {}", self.code, self.message)
    }
}

fn redact_terminal_reason_message(message: &str) -> String {
    let markers = [
        "api_key=",
        "apikey=",
        "access_token=",
        "token=",
        "secret=",
        "password=",
        "authorization: bearer ",
        "bearer ",
    ];

    let mut redacted = message.to_string();
    for marker in markers {
        let marker_lower = marker.to_ascii_lowercase();
        let mut search_from = 0;

        loop {
            let lower = redacted.to_ascii_lowercase();
            let Some(relative_start) = lower[search_from..].find(&marker_lower) else {
                break;
            };
            let start = search_from + relative_start;
            let value_start = start + marker.len();
            if value_start >= redacted.len() {
                break;
            }

            let value_end = redacted[value_start..]
                .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '&' | '"' | '\''))
                .map(|offset| value_start + offset)
                .unwrap_or_else(|| redacted.len());

            if value_end > value_start {
                redacted.replace_range(value_start..value_end, "[REDACTED]");
                search_from = value_start + "[REDACTED]".len();
            } else {
                search_from = value_start;
            }
        }
    }

    redacted.chars().take(500).collect()
}

/// Port for tracking task progress and status
#[async_trait]
pub trait ProgressTracker: Send + Sync {
    /// Get task by ID
    async fn get_task(&self, task_id: i64) -> DbResult<Option<TaskInfo>>;

    /// Update task status
    async fn update_task_status(&self, task_id: i64, status: TaskStatus) -> DbResult<()>;

    /// Update task progress - increment process_count and actual_consumption
    ///
    /// This calls the fn_update_task_progress stored procedure which:
    /// - Increments task.process_count by the given amount
    /// - Calculates and adds actual_consumption based on unit price
    /// - Updates campaign.actual_consumption
    ///
    /// Returns TaskProgressUpdate with should_stop flag indicating if campaign is stopping
    /// (matching Python agent behavior)
    async fn update_task_progress(
        &self,
        task_id: i64,
        increment: i32,
    ) -> DbResult<TaskProgressUpdate>;

    /// Update task with error
    async fn set_task_error(
        &self,
        task_id: i64,
        error: &str,
        terminal_reason: &TaskTerminalReason,
    ) -> DbResult<()>;

    /// Mark task as completed
    async fn complete_task(
        &self,
        task_id: i64,
        terminal_reason: &TaskTerminalReason,
    ) -> DbResult<()>;

    /// Mark task as failed
    async fn fail_task(
        &self,
        task_id: i64,
        error: &str,
        terminal_reason: &TaskTerminalReason,
    ) -> DbResult<()>;

    /// Check if task should stop (campaign stopped or task cancelled)
    async fn should_stop(&self, task_id: i64) -> DbResult<bool>;

    /// Increment processed count for a campaign
    async fn increment_processed(&self, campaign_id: i32, count: i32) -> DbResult<()>;

    /// Get campaign processed count
    async fn get_processed_count(&self, campaign_id: i32) -> DbResult<i32>;

    /// Stop campaign gracefully (matching Python agent's fn_stop_campaign_gracefully)
    ///
    /// This sets campaign status to STOPPING or STOPPED and handles budget refunds.
    /// Used when keyword search returns zero results.
    async fn stop_campaign_gracefully(&self, campaign_id: i32) -> DbResult<CampaignStopResult>;
}

/// Task information
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// Task ID
    pub id: i64,

    /// Associated campaign ID
    pub campaign_id: i32,

    /// Platform ID
    pub platform_id: i32,

    /// Keywords for crawling (JSON array)
    pub keywords: Option<serde_json::Value>,

    /// Current status
    pub status: TaskStatus,

    /// Progress percentage (0-100)
    pub progress: i32,

    /// Error message if failed
    pub error_message: Option<String>,

    /// Terminal reason persisted for completed, failed, or cancelled tasks.
    pub terminal_reason: Option<String>,
}

impl TaskInfo {
    /// Check if task is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, TaskStatus::Completed | TaskStatus::Failed)
    }

    /// Check if task is running
    pub fn is_running(&self) -> bool {
        matches!(self.status, TaskStatus::Running)
    }

    /// Parse keywords from JSON
    pub fn parse_keywords(&self) -> Vec<String> {
        self.keywords
            .as_ref()
            .and_then(|v| {
                if let serde_json::Value::Array(arr) = v {
                    Some(
                        arr.iter()
                            .filter_map(|item| item.as_str().map(|s| s.to_string()))
                            .collect(),
                    )
                } else if let serde_json::Value::String(s) = v {
                    // Handle single string case
                    Some(vec![s.clone()])
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Waiting to be processed
    Pending = 0,
    /// Currently running
    Running = 1,
    /// Completed successfully
    Completed = 2,
    /// Failed with error
    Failed = 3,
}

impl From<i16> for TaskStatus {
    fn from(value: i16) -> Self {
        match value {
            0 => TaskStatus::Pending,
            1 => TaskStatus::Running,
            2 => TaskStatus::Completed,
            3 => TaskStatus::Failed,
            _ => TaskStatus::Pending,
        }
    }
}

impl From<&str> for TaskStatus {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "init" | "pending" => TaskStatus::Pending,
            "processing" | "running" => TaskStatus::Running, // Support both for compatibility
            "completed" => TaskStatus::Completed,
            "failed" | "cancelled" | "canceled" => TaskStatus::Failed,
            _ => TaskStatus::Pending,
        }
    }
}

impl From<String> for TaskStatus {
    fn from(value: String) -> Self {
        TaskStatus::from(value.as_str())
    }
}

impl From<TaskStatus> for i16 {
    fn from(status: TaskStatus) -> Self {
        status as i16
    }
}

impl TaskStatus {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Failed)
    }
}

/// Progress update for reporting
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    /// Task ID
    pub task_id: i64,

    /// Progress percentage (0-100)
    pub progress: i32,

    /// Status message
    pub message: Option<String>,

    /// Contents processed so far
    pub contents_processed: i32,

    /// Comments processed so far
    pub comments_processed: i32,

    /// Analyses generated so far
    pub analyses_generated: i32,
}

impl ProgressUpdate {
    /// Create a new progress update
    pub fn new(task_id: i64, progress: i32) -> Self {
        Self {
            task_id,
            progress,
            message: None,
            contents_processed: 0,
            comments_processed: 0,
            analyses_generated: 0,
        }
    }

    /// Set progress message
    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    /// Set processing counts
    pub fn with_counts(mut self, contents: i32, comments: i32, analyses: i32) -> Self {
        self.contents_processed = contents;
        self.comments_processed = comments;
        self.analyses_generated = analyses;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_conversion() {
        assert_eq!(TaskStatus::from(0), TaskStatus::Pending);
        assert_eq!(TaskStatus::from(1), TaskStatus::Running);
        assert_eq!(TaskStatus::from(2), TaskStatus::Completed);
        assert_eq!(i16::from(TaskStatus::Failed), 3);
    }

    #[test]
    fn test_task_status_terminal() {
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
    }

    #[test]
    fn test_task_info_keywords() {
        let task = TaskInfo {
            id: 1,
            campaign_id: 1,
            platform_id: 2,
            keywords: Some(serde_json::json!(["fitness", "workout", "health"])),
            status: TaskStatus::Pending,
            progress: 0,
            error_message: None,
            terminal_reason: None,
        };

        let keywords = task.parse_keywords();
        assert_eq!(keywords.len(), 3);
        assert_eq!(keywords[0], "fitness");
    }

    #[test]
    fn task_terminal_reason_formats_completed_reason() {
        let reason = TaskTerminalReason::completed();
        assert_eq!(reason.code, "COMPLETED");
        assert_eq!(
            reason.as_terminal_message(),
            "COMPLETED: Task completed successfully"
        );
    }

    #[test]
    fn task_terminal_reason_redacts_secret_like_values() {
        let reason = TaskTerminalReason::provider_failure(
            "request failed with api_key=sk-test-token Authorization: Bearer abc123",
        );
        let message = reason.as_terminal_message();
        assert!(message.starts_with("PROVIDER_FAILURE:"));
        assert!(!message.contains("sk-test-token"));
        assert!(!message.contains("Bearer abc123"));
        assert!(message.contains("api_key=[REDACTED]"));
    }

    #[test]
    fn task_terminal_reason_bounds_long_messages() {
        let reason = TaskTerminalReason::internal_error("x".repeat(600));
        assert_eq!(reason.message.chars().count(), 500);
    }

    #[test]
    fn test_progress_update() {
        let update = ProgressUpdate::new(1, 50)
            .with_message("Processing comments")
            .with_counts(5, 100, 50);

        assert_eq!(update.progress, 50);
        assert_eq!(update.comments_processed, 100);
    }
}
