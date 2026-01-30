//! Comment Gateway Port - Interface for fetching comments from platforms

use async_trait::async_trait;

use crate::domain::Comment;
use crate::domain::errors::GatewayResult;

/// Port for fetching comments from external platforms
#[async_trait]
pub trait CommentGateway: Send + Sync {
    /// Fetch comments for a specific content item
    ///
    /// # Arguments
    /// * `content_id` - Platform-specific content ID
    /// * `options` - Fetch options (count, cursor, etc.)
    ///
    /// # Returns
    /// * `GatewayResult<FetchCommentsResult>` - Comments and pagination info
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult>;

    /// Fetch all comments for a content item (auto-pagination)
    ///
    /// This will automatically paginate through all comments up to
    /// the specified limit.
    ///
    /// # Arguments
    /// * `content_id` - Platform-specific content ID
    /// * `max_count` - Maximum total comments to fetch
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Comment>>` - All fetched comments
    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>>;

    /// Fetch replies to a specific comment
    ///
    /// # Arguments
    /// * `content_id` - Platform-specific content ID
    /// * `comment_id` - Parent comment ID
    /// * `options` - Fetch options
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Comment>>` - Reply comments
    async fn fetch_replies(
        &self,
        content_id: &str,
        comment_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>>;

    /// Get the platform name this gateway handles
    fn platform(&self) -> &str;
}

/// Options for fetching comments
#[derive(Debug, Clone)]
pub struct FetchCommentsOptions {
    /// Number of comments to fetch per request
    pub count: u32,
    
    /// Pagination cursor
    pub cursor: Option<String>,
    
    /// Sort order
    pub sort: CommentSort,
    
    /// Whether to include replies
    pub include_replies: bool,
}

impl Default for FetchCommentsOptions {
    fn default() -> Self {
        Self {
            count: 50,
            cursor: None,
            sort: CommentSort::Relevance,
            include_replies: false,
        }
    }
}

impl FetchCommentsOptions {
    /// Create new options with specified count
    pub fn new(count: u32) -> Self {
        Self {
            count,
            ..Default::default()
        }
    }

    /// Set the cursor for pagination
    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    /// Set the sort order
    pub fn with_sort(mut self, sort: CommentSort) -> Self {
        self.sort = sort;
        self
    }

    /// Include replies in the result
    pub fn with_replies(mut self) -> Self {
        self.include_replies = true;
        self
    }
}

/// Comment sort order
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CommentSort {
    /// Sort by relevance (platform default)
    #[default]
    Relevance,
    /// Sort by newest first
    Newest,
    /// Sort by oldest first
    Oldest,
    /// Sort by most likes
    MostLiked,
}

/// Result of fetching comments
#[derive(Debug, Clone)]
pub struct FetchCommentsResult {
    /// Fetched comments
    pub comments: Vec<Comment>,
    
    /// Whether there are more comments available
    pub has_more: bool,
    
    /// Cursor for fetching next page
    pub next_cursor: Option<String>,
    
    /// Total number of comments (if known)
    pub total: Option<i64>,
}

impl FetchCommentsResult {
    /// Create an empty result
    pub fn empty() -> Self {
        Self {
            comments: Vec::new(),
            has_more: false,
            next_cursor: None,
            total: None,
        }
    }

    /// Create a result with comments
    pub fn new(comments: Vec<Comment>) -> Self {
        Self {
            comments,
            has_more: false,
            next_cursor: None,
            total: None,
        }
    }

    /// Set pagination info
    pub fn with_pagination(mut self, has_more: bool, cursor: Option<String>) -> Self {
        self.has_more = has_more;
        self.next_cursor = cursor;
        self
    }

    /// Set total count
    pub fn with_total(mut self, total: i64) -> Self {
        self.total = Some(total);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_comments_options() {
        let opts = FetchCommentsOptions::new(100)
            .with_cursor("abc123")
            .with_sort(CommentSort::Newest)
            .with_replies();

        assert_eq!(opts.count, 100);
        assert_eq!(opts.cursor, Some("abc123".to_string()));
        assert_eq!(opts.sort, CommentSort::Newest);
        assert!(opts.include_replies);
    }

    #[test]
    fn test_fetch_comments_result() {
        let result = FetchCommentsResult::empty()
            .with_pagination(true, Some("next".to_string()))
            .with_total(500);

        assert!(result.has_more);
        assert_eq!(result.next_cursor, Some("next".to_string()));
        assert_eq!(result.total, Some(500));
    }
}
