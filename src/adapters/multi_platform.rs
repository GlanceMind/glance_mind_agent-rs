//! Multi-Platform Gateway - Routes requests to platform-specific adapters
//!
//! This adapter wraps multiple platform-specific adapters and routes requests
//! based on the platform field in SearchOptions or content metadata.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::errors::{GatewayError, GatewayResult};
use crate::domain::{Comment, Content, KeywordType, SearchOptions};
use crate::ports::{
    comment_gateway::{FetchCommentsOptions, FetchCommentsResult},
    CommentGateway, ContentGateway,
};

/// Multi-platform content gateway that routes to platform-specific adapters
pub struct MultiPlatformContentGateway {
    /// Platform name -> adapter mapping
    adapters: HashMap<String, Arc<dyn ContentGateway>>,
    /// Default platform (fallback)
    default_platform: String,
    /// Current platform context (set per-request)
    current_platform: std::sync::RwLock<String>,
}

impl MultiPlatformContentGateway {
    /// Create a new multi-platform gateway
    pub fn new(default_platform: impl Into<String>) -> Self {
        let default = default_platform.into();
        Self {
            adapters: HashMap::new(),
            default_platform: default.clone(),
            current_platform: std::sync::RwLock::new(default),
        }
    }

    /// Get the current platform
    fn current(&self) -> String {
        self.current_platform
            .read()
            .map(|p| p.clone())
            .unwrap_or_else(|_| self.default_platform.clone())
    }

    /// Add a platform adapter
    pub fn add_adapter(
        mut self,
        platform: impl Into<String>,
        adapter: Arc<dyn ContentGateway>,
    ) -> Self {
        self.adapters.insert(platform.into(), adapter);
        self
    }

    /// Get adapter for a platform
    fn get_adapter(&self, platform: &str) -> GatewayResult<&Arc<dyn ContentGateway>> {
        self.adapters
            .get(platform)
            .or_else(|| self.adapters.get(&self.default_platform))
            .ok_or_else(|| {
                GatewayError::InvalidParams(format!("No adapter for platform: {}", platform))
            })
    }
}

#[async_trait]
impl ContentGateway for MultiPlatformContentGateway {
    async fn search(&self, options: &SearchOptions) -> GatewayResult<Vec<Content>> {
        let platform = options
            .platform
            .as_deref()
            .unwrap_or(&self.default_platform);
        let adapter = self.get_adapter(platform)?;
        adapter.search(options).await
    }

    async fn fetch_by_keyword(
        &self,
        keyword: &KeywordType,
        options: &SearchOptions,
    ) -> GatewayResult<Vec<Content>> {
        let platform = options
            .platform
            .as_deref()
            .unwrap_or(&self.default_platform);
        let adapter = self.get_adapter(platform)?;
        adapter.fetch_by_keyword(keyword, options).await
    }

    async fn fetch_user_content(&self, user_id: &str, count: u32) -> GatewayResult<Vec<Content>> {
        // Use current platform for user content
        let platform = self.current();
        let adapter = self.get_adapter(&platform)?;
        adapter.fetch_user_content(user_id, count).await
    }

    async fn fetch_by_id(&self, content_id: &str) -> GatewayResult<Option<Content>> {
        // Use current platform for content by ID
        let platform = self.current();
        let adapter = self.get_adapter(&platform)?;
        adapter.fetch_by_id(content_id).await
    }

    fn platform(&self) -> &str {
        "multi"
    }
}

/// Multi-platform comment gateway that routes to platform-specific adapters
pub struct MultiPlatformCommentGateway {
    /// Platform name -> adapter mapping
    adapters: HashMap<String, Arc<dyn CommentGateway>>,
    /// Default platform (fallback)
    default_platform: String,
    /// Current platform context (set per-request)
    current_platform: std::sync::RwLock<String>,
}

impl MultiPlatformCommentGateway {
    /// Create a new multi-platform gateway
    pub fn new(default_platform: impl Into<String>) -> Self {
        let default = default_platform.into();
        Self {
            adapters: HashMap::new(),
            default_platform: default.clone(),
            current_platform: std::sync::RwLock::new(default),
        }
    }

    /// Add a platform adapter
    pub fn add_adapter(
        mut self,
        platform: impl Into<String>,
        adapter: Arc<dyn CommentGateway>,
    ) -> Self {
        self.adapters.insert(platform.into(), adapter);
        self
    }

    /// Set current platform context
    pub fn set_platform(&self, platform: &str) {
        if let Ok(mut current) = self.current_platform.write() {
            *current = platform.to_string();
        }
    }

    /// Get adapter for a platform
    fn get_adapter(&self, platform: &str) -> GatewayResult<&Arc<dyn CommentGateway>> {
        self.adapters
            .get(platform)
            .or_else(|| self.adapters.get(&self.default_platform))
            .ok_or_else(|| {
                GatewayError::InvalidParams(format!(
                    "No comment adapter for platform: {}",
                    platform
                ))
            })
    }

    /// Get current platform
    fn current(&self) -> String {
        self.current_platform
            .read()
            .map(|p| p.clone())
            .unwrap_or_else(|_| self.default_platform.clone())
    }
}

#[async_trait]
impl CommentGateway for MultiPlatformCommentGateway {
    async fn fetch_comments(
        &self,
        content_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<FetchCommentsResult> {
        let platform = self.current();
        let adapter = self.get_adapter(&platform)?;
        adapter.fetch_comments(content_id, options).await
    }

    async fn fetch_all_comments(
        &self,
        content_id: &str,
        max_count: u32,
    ) -> GatewayResult<Vec<Comment>> {
        let platform = self.current();
        tracing::debug!(platform = %platform, content_id = %content_id, "MultiPlatformCommentGateway: fetch_all_comments");
        let adapter = self.get_adapter(&platform)?;
        adapter.fetch_all_comments(content_id, max_count).await
    }

    async fn fetch_replies(
        &self,
        content_id: &str,
        comment_id: &str,
        options: &FetchCommentsOptions,
    ) -> GatewayResult<Vec<Comment>> {
        let platform = self.current();
        let adapter = self.get_adapter(&platform)?;
        adapter.fetch_replies(content_id, comment_id, options).await
    }

    fn platform(&self) -> &str {
        "multi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_platform_gateway_creation() {
        let gateway = MultiPlatformContentGateway::new("tiktok");
        assert_eq!(gateway.platform(), "multi");
    }
}
