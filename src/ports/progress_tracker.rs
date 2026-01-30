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
    async fn update_task_progress(&self, task_id: i64, increment: i32) -> DbResult<TaskProgressUpdate>;

    /// Update task with error
    async fn set_task_error(&self, task_id: i64, error: &str) -> DbResult<()>;

    /// Mark task as completed
    async fn complete_task(&self, task_id: i64) -> DbResult<()>;

    /// Mark task as failed
    async fn fail_task(&self, task_id: i64, error: &str) -> DbResult<()>;

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
            "running" => TaskStatus::Running,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
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
        };

        let keywords = task.parse_keywords();
        assert_eq!(keywords.len(), 3);
        assert_eq!(keywords[0], "fitness");
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
