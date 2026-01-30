//! Prompt Repository Port - Interface for querying prompts and campaign config

use async_trait::async_trait;

use crate::domain::errors::DbResult;
use crate::ports::ai_analyzer::AnalysisContext;

/// Port for querying prompt templates and campaign configuration
#[async_trait]
pub trait PromptRepository: Send + Sync {
    /// Get campaign configuration by ID
    async fn get_campaign(&self, campaign_id: i32) -> DbResult<Option<CampaignConfig>>;

    /// Get analysis context for a campaign
    ///
    /// This combines campaign settings into an AnalysisContext
    /// suitable for the AI analyzer.
    async fn get_analysis_context(&self, campaign_id: i32) -> DbResult<Option<AnalysisContext>>;

    /// Get platform configuration
    async fn get_platform(&self, platform_id: i32) -> DbResult<Option<PlatformConfig>>;

    /// Get platform by name
    async fn get_platform_by_name(&self, name: &str) -> DbResult<Option<PlatformConfig>>;

    /// Check if campaign should stop (status changed or max reached)
    async fn should_stop_campaign(&self, campaign_id: i32) -> DbResult<bool>;

    /// Update campaign processed count
    async fn update_processed_count(&self, campaign_id: i32, count: i32) -> DbResult<()>;
}

/// Campaign configuration
#[derive(Debug, Clone)]
pub struct CampaignConfig {
    /// Campaign ID
    pub id: i32,
    
    /// User ID who owns this campaign
    pub user_id: i32,
    
    /// Campaign name
    pub name: String,
    
    /// Platform ID
    pub platform_id: i32,
    
    /// Current status
    pub status: CampaignStatus,
    
    /// Target audience description
    pub target_audience: Option<String>,
    
    /// Product/service prompt
    pub product_prompt: Option<String>,
    
    /// Reply strategy instructions
    pub reply_strategy: Option<String>,
    
    /// DM strategy instructions
    pub dm_strategy: Option<String>,
    
    /// Reply post strategy instructions
    pub reply_post_strategy: Option<String>,
    
    /// Maximum comments to process
    pub max_comments: Option<i32>,
    
    /// Comments already processed
    pub processed_comments: i32,
}

impl CampaignConfig {
    /// Convert to AnalysisContext for AI
    pub fn to_analysis_context(&self) -> AnalysisContext {
        let mut ctx = AnalysisContext::new();
        
        if let Some(ref prompt) = self.product_prompt {
            ctx = ctx.with_product_prompt(prompt.clone());
        }
        
        if let Some(ref audience) = self.target_audience {
            ctx = ctx.with_target_audience(audience.clone());
        }
        
        if let Some(ref strategy) = self.reply_strategy {
            ctx = ctx.with_reply_strategy(strategy.clone());
        }
        
        if let Some(ref dm_strat) = self.dm_strategy {
            ctx = ctx.with_dm_strategy(dm_strat.clone());
            ctx = ctx.enable_dm();
        }
        
        if self.reply_post_strategy.is_some() {
            ctx = ctx.enable_post_reply();
        }
        
        ctx
    }

    /// Check if campaign has reached its limit
    pub fn is_at_limit(&self) -> bool {
        if let Some(max) = self.max_comments {
            self.processed_comments >= max
        } else {
            false
        }
    }

    /// Get remaining comments to process
    pub fn remaining_comments(&self) -> Option<i32> {
        self.max_comments.map(|max| (max - self.processed_comments).max(0))
    }
}

/// Campaign status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignStatus {
    /// Draft - not yet started
    Draft = 0,
    /// Active - currently running
    Active = 1,
    /// Paused - temporarily stopped
    Paused = 2,
    /// Completed - finished successfully
    Completed = 3,
    /// Stopping - in process of stopping
    Stopping = 4,
}

impl From<i16> for CampaignStatus {
    fn from(value: i16) -> Self {
        match value {
            0 => CampaignStatus::Draft,
            1 => CampaignStatus::Active,
            2 => CampaignStatus::Paused,
            3 => CampaignStatus::Completed,
            4 => CampaignStatus::Stopping,
            _ => CampaignStatus::Draft,
        }
    }
}

impl From<&str> for CampaignStatus {
    fn from(value: &str) -> Self {
        match value.to_uppercase().as_str() {
            "ACTIVE" => CampaignStatus::Active,
            "PAUSED" => CampaignStatus::Paused,
            "COMPLETED" => CampaignStatus::Completed,
            "STOPPING" => CampaignStatus::Stopping,
            _ => CampaignStatus::Draft,
        }
    }
}

impl From<String> for CampaignStatus {
    fn from(value: String) -> Self {
        CampaignStatus::from(value.as_str())
    }
}

impl CampaignStatus {
    /// Check if campaign should continue processing
    pub fn should_continue(&self) -> bool {
        matches!(self, CampaignStatus::Active)
    }
}

/// Platform configuration
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    /// Platform ID
    pub id: i32,
    
    /// Platform internal name (e.g., "tiktok")
    pub name: String,
    
    /// Platform display name (e.g., "TikTok")
    pub display_name: String,
    
    /// Whether the platform is enabled
    pub is_active: bool,
}

impl PlatformConfig {
    /// Common platform IDs (must match database)
    pub const REDDIT: i32 = 1;
    pub const TIKTOK: i32 = 2;
    pub const FACEBOOK: i32 = 3;
    pub const INSTAGRAM: i32 = 4;
    pub const TWITTER: i32 = 5;
    pub const YOUTUBE: i32 = 6;

    /// Get platform name from ID
    pub fn name_from_id(id: i32) -> &'static str {
        match id {
            1 => "reddit",
            2 => "tiktok",
            3 => "facebook",
            4 => "instagram",
            5 => "twitter",
            6 => "youtube",
            _ => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_campaign_status_conversion() {
        assert_eq!(CampaignStatus::from(1), CampaignStatus::Active);
        assert!(CampaignStatus::Active.should_continue());
        assert!(!CampaignStatus::Paused.should_continue());
    }

    #[test]
    fn test_campaign_limit() {
        let config = CampaignConfig {
            id: 1,
            user_id: 1,
            name: "Test".to_string(),
            platform_id: 2,
            status: CampaignStatus::Active,
            target_audience: None,
            product_prompt: None,
            reply_strategy: None,
            dm_strategy: None,
            reply_post_strategy: None,
            max_comments: Some(100),
            processed_comments: 50,
        };

        assert!(!config.is_at_limit());
        assert_eq!(config.remaining_comments(), Some(50));
    }

    #[test]
    fn test_to_analysis_context() {
        let config = CampaignConfig {
            id: 1,
            user_id: 1,
            name: "Test".to_string(),
            platform_id: 2,
            status: CampaignStatus::Active,
            target_audience: Some("Young adults".to_string()),
            product_prompt: Some("Fitness app".to_string()),
            reply_strategy: Some("Be helpful".to_string()),
            dm_strategy: Some("Send welcome message".to_string()),
            reply_post_strategy: None,
            max_comments: None,
            processed_comments: 0,
        };

        let ctx = config.to_analysis_context();
        assert_eq!(ctx.product_prompt, Some("Fitness app".to_string()));
        assert_eq!(ctx.target_audience, Some("Young adults".to_string()));
        assert!(ctx.generate_dm);
        assert!(!ctx.generate_post_reply);
    }
}
