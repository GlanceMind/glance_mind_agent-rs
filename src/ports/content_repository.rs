//! Content Repository Port - Interface for content and comment persistence

use async_trait::async_trait;

use crate::domain::{Content, Comment, ReplySuggestion};
use crate::domain::errors::DbResult;

/// Result from content save operation (matching Python agent's ON CONFLICT behavior)
#[derive(Debug, Clone)]
pub struct ContentSaveResult {
    /// Database ID of the content
    pub id: i32,
    /// Whether this was a new record (true) or existing record was updated (false)
    pub is_new: bool,
}

/// Port for persisting and querying content data
#[async_trait]
pub trait ContentRepository: Send + Sync {
    // ============================================================
    // Content Operations
    // ============================================================

    /// Check if content already exists by platform content ID
    async fn content_exists(&self, platform: &str, content_id: &str) -> DbResult<bool>;

    /// Get content by platform content ID
    async fn get_content(&self, platform: &str, content_id: &str) -> DbResult<Option<StoredContent>>;

    /// Get content by internal database ID
    async fn get_content_by_id(&self, id: i32) -> DbResult<Option<StoredContent>>;

    /// Save content with ON CONFLICT handling (matching Python agent's atomic upsert)
    /// 
    /// Uses ON CONFLICT (task_id, video_id) DO UPDATE to:
    /// - Return existing record ID if already exists
    /// - Insert new record if not exists
    /// - Returns ContentSaveResult with is_new flag to indicate if this was a new insert
    /// 
    /// This matches Python agent's behavior where only new videos trigger progress updates.
    async fn save_content(&self, content: &Content, campaign_id: Option<i32>, task_id: Option<i32>) -> DbResult<ContentSaveResult>;

    /// Save multiple content items in batch
    async fn save_contents(&self, contents: &[Content], campaign_id: Option<i32>, task_id: Option<i32>) -> DbResult<Vec<ContentSaveResult>>;

    /// Update content engagement metrics
    async fn update_content_engagement(
        &self,
        id: i32,
        likes: i64,
        comments: i64,
        shares: i64,
        views: i64,
    ) -> DbResult<()>;

    // ============================================================
    // Comment Operations
    // ============================================================

    /// Check if comment already exists
    async fn comment_exists(&self, platform: &str, comment_id: &str) -> DbResult<bool>;

    /// Get comment by platform comment ID
    async fn get_comment(&self, platform: &str, comment_id: &str) -> DbResult<Option<StoredComment>>;

    /// Get comment by internal database ID
    async fn get_comment_by_id(&self, id: i32) -> DbResult<Option<StoredComment>>;

    /// Save a new comment (only for comments with AI suggestions, matching Python agent)
    async fn save_comment(&self, comment: &Comment, content_db_id: i32) -> DbResult<i32>;

    /// Save multiple comments in batch
    async fn save_comments(&self, comments: &[Comment], content_db_id: i32) -> DbResult<Vec<i32>>;

    /// Get pending comments for a campaign
    async fn get_pending_comments(&self, campaign_id: i32, limit: i32) -> DbResult<Vec<StoredComment>>;

    /// Update comment status
    async fn update_comment_status(&self, id: i32, status: CommentStatus) -> DbResult<()>;

    // ============================================================
    // AI Analysis Operations
    // ============================================================

    /// Save comment with AI analysis in one operation (matching Python agent's save_comments_and_analysis)
    /// 
    /// This method:
    /// 1. Checks if comment already exists (via video_db_id + comment_id)
    /// 2. If exists, updates the AI analysis fields
    /// 3. If not exists, inserts new record with comment data and AI analysis
    /// 
    /// This matches Python agent's behavior of only saving comments that have AI suggestions.
    async fn save_comment_with_analysis(
        &self,
        comment: &Comment,
        content_db_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32>;

    /// Save an AI analysis result (updates existing comment)
    async fn save_analysis(
        &self,
        comment_id: i32,
        campaign_id: i32,
        suggestion: &ReplySuggestion,
    ) -> DbResult<i32>;

    /// Get analysis for a comment
    async fn get_analysis(&self, comment_id: i32) -> DbResult<Option<StoredAnalysis>>;
}

/// Stored content record with database metadata
#[derive(Debug, Clone)]
pub struct StoredContent {
    /// Database ID
    pub id: i32,
    
    /// Platform ID (from database)
    pub platform_id: i32,
    
    /// Platform content ID
    pub content_id: String,
    
    /// Author username
    pub author_unique_id: Option<String>,
    
    /// Author display name
    pub author_nickname: Option<String>,
    
    /// Content description
    pub description: Option<String>,
    
    /// Content URL
    pub content_url: Option<String>,
    
    /// Engagement metrics
    pub likes: Option<i64>,
    pub comments: Option<i64>,
    pub shares: Option<i64>,
    pub views: Option<i64>,
    
    /// Content creation timestamp
    pub content_created_at: Option<i64>,
    
    /// Raw platform data
    pub raw_data: Option<serde_json::Value>,
    
    /// Associated campaign ID
    pub campaign_id: Option<i32>,
}

impl StoredContent {
    /// Convert to domain Content
    pub fn to_domain(&self, platform: &str) -> Content {
        Content {
            platform: platform.to_string(),
            content_id: self.content_id.clone(),
            author: self.author_unique_id.clone().unwrap_or_default(),
            author_name: self.author_nickname.clone(),
            description: self.description.clone().unwrap_or_default(),
            url: self.content_url.clone(),
            engagement: crate::domain::Engagement {
                likes: self.likes.unwrap_or(0),
                comments: self.comments.unwrap_or(0),
                shares: self.shares.unwrap_or(0),
                views: self.views.unwrap_or(0),
            },
            created_at: self.content_created_at,
            raw_data: self.raw_data.clone(),
        }
    }
}

/// Stored comment record with database metadata
#[derive(Debug, Clone)]
pub struct StoredComment {
    /// Database ID
    pub id: i32,
    
    /// Platform ID
    pub platform_id: i32,
    
    /// Content database ID
    pub content_id: i32,
    
    /// Platform comment ID
    pub comment_id: String,
    
    /// Parent comment ID
    pub parent_comment_id: Option<String>,
    
    /// Author user ID
    pub author_uid: Option<String>,
    
    /// Author username
    pub author_unique_id: Option<String>,
    
    /// Author display name
    pub author_nickname: Option<String>,
    
    /// Comment text
    pub comment_text: Option<String>,
    
    /// Like count
    pub likes: Option<i64>,
    
    /// Reply count
    pub reply_count: Option<i32>,
    
    /// Comment creation timestamp
    pub comment_created_at: Option<i64>,
    
    /// Whether this is a reply
    pub is_reply: bool,
    
    /// Raw platform data
    pub raw_data: Option<serde_json::Value>,
    
    /// Processing status
    pub status: i16,
}

impl StoredComment {
    /// Convert to domain Comment
    pub fn to_domain(&self, platform: &str, content_platform_id: &str) -> Comment {
        Comment {
            platform: platform.to_string(),
            comment_id: self.comment_id.clone(),
            content_id: content_platform_id.to_string(),
            parent_id: self.parent_comment_id.clone(),
            author: self.author_unique_id.clone().unwrap_or_default(),
            author_name: self.author_nickname.clone(),
            author_uid: self.author_uid.clone(),
            text: self.comment_text.clone().unwrap_or_default(),
            likes: self.likes.unwrap_or(0),
            reply_count: self.reply_count.unwrap_or(0),
            created_at: self.comment_created_at,
            language: None,
            is_reply: self.is_reply,
            raw_data: self.raw_data.clone(),
        }
    }
}

/// Stored AI analysis record
#[derive(Debug, Clone)]
pub struct StoredAnalysis {
    /// Database ID
    pub id: i32,
    
    /// Comment database ID
    pub comment_id: i32,
    
    /// Campaign ID
    pub campaign_id: i32,
    
    /// Suggested reply text
    pub suggested_reply: Option<String>,
    
    /// Suggested DM text
    pub suggested_dm: Option<String>,
    
    /// Suggested post reply
    pub suggested_reply_post: Option<String>,
    
    /// Reason/explanation
    pub reason: Option<String>,
    
    /// Tokens used
    pub tokens_used: Option<i32>,
    
    /// Model name
    pub model_name: Option<String>,
}

impl StoredAnalysis {
    /// Convert to domain ReplySuggestion
    pub fn to_domain(&self, comment_platform_id: &str) -> ReplySuggestion {
        ReplySuggestion {
            comment_id: comment_platform_id.to_string(),
            reply_text: self.suggested_reply.clone(),
            dm_text: self.suggested_dm.clone(),
            post_reply_text: self.suggested_reply_post.clone(),
            reason: self.reason.clone(),
            confidence: None,
            intent: None,
            sentiment: None,
            tokens_used: self.tokens_used,
            model: self.model_name.clone(),
        }
    }
}

/// Comment processing status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStatus {
    /// Waiting to be processed
    Pending = 0,
    /// Currently being processed
    Processing = 1,
    /// Successfully processed
    Completed = 2,
    /// Processing failed
    Failed = 3,
    /// Skipped (e.g., spam, duplicate)
    Skipped = 4,
}

impl From<i16> for CommentStatus {
    fn from(value: i16) -> Self {
        match value {
            0 => CommentStatus::Pending,
            1 => CommentStatus::Processing,
            2 => CommentStatus::Completed,
            3 => CommentStatus::Failed,
            4 => CommentStatus::Skipped,
            _ => CommentStatus::Pending,
        }
    }
}

impl From<CommentStatus> for i16 {
    fn from(status: CommentStatus) -> Self {
        status as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment_status_conversion() {
        assert_eq!(CommentStatus::from(0), CommentStatus::Pending);
        assert_eq!(CommentStatus::from(1), CommentStatus::Processing);
        assert_eq!(CommentStatus::from(2), CommentStatus::Completed);
        assert_eq!(i16::from(CommentStatus::Completed), 2);
    }
}
