//! Content Gateway Port - Interface for fetching content from platforms

use async_trait::async_trait;

use crate::domain::errors::GatewayResult;
use crate::domain::{Content, KeywordType, SearchOptions};

/// 取数欠交付原因(per R-007;cursor 不过 trait,A005 drop)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchShortfall {
    /// 上游枯竭:has_more=false / cursor 缺失 / 空页上限 / 重复 cursor(F-003/F-004/F-005)
    Exhausted,
    /// 翻页中途失败但已有部分进展(F-002/F-006);message 仅用于 terminal_reason,入库前必经脱敏
    PartialFailure { message: String },
}

/// fetch 路径返回载体
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub contents: Vec<Content>,
    /// None = 足量交付 或 适配器未提供欠交付信息(未迁移平台的滚动兼容语义)
    pub shortfall: Option<FetchShortfall>,
}

/// Port for fetching content (videos, posts) from external platforms
#[async_trait]
pub trait ContentGateway: Send + Sync {
    /// Search for content based on options
    ///
    /// # Arguments
    /// * `options` - Search parameters (query, region, count, etc.)
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Content>>` - List of matching content
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>>;

    /// Fetch content by a specific keyword type
    ///
    /// This handles platform-specific keyword parsing:
    /// - `KeywordType::Search` -> regular search
    /// - `KeywordType::UserId` -> fetch user's content
    /// - `KeywordType::ContentId` -> fetch specific content
    ///
    /// # Arguments
    /// * `keyword` - Parsed keyword type
    /// * `options` - Additional search options
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Content>>` - List of matching content
    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>>;

    /// Fetch content from a specific user
    ///
    /// # Arguments
    /// * `user_id` - User identifier (username or platform-specific ID)
    /// * `count` - Maximum number of items to fetch
    ///
    /// # Returns
    /// * `GatewayResult<Vec<Content>>` - List of user's content
    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>>;

    /// Fetch a specific content item by ID
    ///
    /// # Arguments
    /// * `content_id` - Platform-specific content ID
    ///
    /// # Returns
    /// * `GatewayResult<Option<Content>>` - The content if found
    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>>;

    /// Get the platform name this gateway handles
    fn platform(&self) -> &str;
}

/// Options for fetching user content
#[derive(Debug, Clone, Default)]
pub struct UserContentOptions {
    /// User's unique ID (e.g., @username)
    pub user_id: Option<String>,

    /// User's secure/internal ID (platform-specific)
    pub sec_user_id: Option<String>,

    /// Maximum number of items to fetch
    pub count: u32,

    /// Pagination cursor
    pub cursor: Option<i64>,

    /// Sort type (platform-specific)
    pub sort_type: Option<u8>,
}

impl UserContentOptions {
    /// Create options for fetching by username
    pub fn by_username(username: impl Into<String>) -> Self {
        Self {
            user_id: Some(username.into()),
            count: 20,
            ..Default::default()
        }
    }

    /// Create options for fetching by secure user ID
    pub fn by_sec_user_id(sec_uid: impl Into<String>) -> Self {
        Self {
            sec_user_id: Some(sec_uid.into()),
            count: 20,
            ..Default::default()
        }
    }

    /// Set the count
    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_content_options() {
        let opts = UserContentOptions::by_username("testuser").with_count(10);
        assert_eq!(opts.user_id, Some("testuser".to_string()));
        assert_eq!(opts.count, 10);
    }
}
