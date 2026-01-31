//! AI Analyzer Port - Interface for AI-powered analysis and suggestions

use async_trait::async_trait;

use crate::domain::errors::AiResult;
use crate::domain::{Comment, Content, ReplySuggestion};

/// Port for AI-powered comment analysis and reply generation
#[async_trait]
pub trait AiAnalyzer: Send + Sync {
    /// Analyze a single comment and generate reply suggestions
    ///
    /// # Arguments
    /// * `comment` - The comment to analyze
    /// * `content` - The content the comment belongs to
    /// * `context` - Additional context for analysis
    ///
    /// # Returns
    /// * `AiResult<ReplySuggestion>` - AI-generated reply suggestion
    async fn analyze_comment(
        &self,
        comment: &Comment,
        content: &Content,
        context: &AnalysisContext,
    ) -> AiResult<ReplySuggestion>;

    /// Analyze multiple comments in batch
    ///
    /// # Arguments
    /// * `comments` - Comments to analyze
    /// * `content` - The content the comments belong to
    /// * `context` - Additional context for analysis
    ///
    /// # Returns
    /// * `AiResult<Vec<ReplySuggestion>>` - AI-generated reply suggestions
    async fn analyze_batch(
        &self,
        comments: &[Comment],
        content: &Content,
        context: &AnalysisContext,
    ) -> AiResult<Vec<ReplySuggestion>>;

    /// Check if the analyzer is healthy and ready
    async fn health_check(&self) -> AiResult<bool>;

    /// Get the model name being used
    fn model_name(&self) -> &str;
}

/// Context for AI analysis
#[derive(Debug, Clone, Default)]
pub struct AnalysisContext {
    /// Product/service description
    pub product_prompt: Option<String>,

    /// Target audience description
    pub target_audience: Option<String>,

    /// Reply strategy instructions
    pub reply_strategy: Option<String>,

    /// DM strategy instructions
    pub dm_strategy: Option<String>,

    /// Reply post strategy instructions
    pub reply_post_strategy: Option<String>,

    /// Brand voice/tone guidelines
    pub brand_voice: Option<String>,

    /// Language preference for replies
    pub preferred_language: Option<String>,

    /// Maximum reply length
    pub max_reply_length: Option<i32>,

    /// Whether to generate DM suggestions
    pub generate_dm: bool,

    /// Whether to generate post reply suggestions
    pub generate_post_reply: bool,
}

impl AnalysisContext {
    /// Create a new analysis context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set product prompt
    pub fn with_product_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.product_prompt = Some(prompt.into());
        self
    }

    /// Set target audience
    pub fn with_target_audience(mut self, audience: impl Into<String>) -> Self {
        self.target_audience = Some(audience.into());
        self
    }

    /// Set reply strategy
    pub fn with_reply_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.reply_strategy = Some(strategy.into());
        self
    }

    /// Set DM strategy
    pub fn with_dm_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.dm_strategy = Some(strategy.into());
        self
    }

    /// Set reply post (viral comment) strategy
    pub fn with_reply_post_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.reply_post_strategy = Some(strategy.into());
        self
    }

    /// Enable DM generation
    pub fn enable_dm(mut self) -> Self {
        self.generate_dm = true;
        self
    }

    /// Enable post reply generation
    pub fn enable_post_reply(mut self) -> Self {
        self.generate_post_reply = true;
        self
    }

    /// Set preferred language
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.preferred_language = Some(lang.into());
        self
    }

    /// Build the system prompt from context
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        parts.push("You are an AI assistant that analyzes social media comments and generates appropriate replies.".to_string());

        if let Some(ref product) = self.product_prompt {
            parts.push(format!("\n\nProduct/Service Information:\n{}", product));
        }

        if let Some(ref audience) = self.target_audience {
            parts.push(format!("\n\nTarget Audience:\n{}", audience));
        }

        if let Some(ref strategy) = self.reply_strategy {
            parts.push(format!("\n\nReply Strategy:\n{}", strategy));
        }

        if let Some(ref voice) = self.brand_voice {
            parts.push(format!("\n\nBrand Voice:\n{}", voice));
        }

        if let Some(ref lang) = self.preferred_language {
            parts.push(format!("\n\nPreferred Response Language: {}", lang));
        }

        parts.join("")
    }
}

/// AI analysis request for batch processing
#[derive(Debug, Clone)]
pub struct AnalysisRequest {
    /// Comment to analyze
    pub comment: Comment,

    /// Content the comment belongs to
    pub content: Content,

    /// Campaign ID for tracking
    pub campaign_id: i32,
}

impl AnalysisRequest {
    /// Create a new analysis request
    pub fn new(comment: Comment, content: Content, campaign_id: i32) -> Self {
        Self {
            comment,
            content,
            campaign_id,
        }
    }
}

/// AI usage statistics
#[derive(Debug, Clone, Default)]
pub struct AiUsageStats {
    /// Total tokens used in prompts
    pub prompt_tokens: i32,

    /// Total tokens used in completions
    pub completion_tokens: i32,

    /// Total tokens used
    pub total_tokens: i32,

    /// Number of requests made
    pub request_count: i32,

    /// Number of successful requests
    pub success_count: i32,

    /// Number of failed requests
    pub error_count: i32,
}

impl AiUsageStats {
    /// Add tokens from a request
    pub fn add_tokens(&mut self, prompt: i32, completion: i32) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_tokens += prompt + completion;
    }

    /// Record a successful request
    pub fn record_success(&mut self, tokens: i32) {
        self.request_count += 1;
        self.success_count += 1;
        self.total_tokens += tokens;
    }

    /// Record a failed request
    pub fn record_error(&mut self) {
        self.request_count += 1;
        self.error_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_context_builder() {
        let ctx = AnalysisContext::new()
            .with_product_prompt("We sell fitness equipment")
            .with_target_audience("Health-conscious adults")
            .with_reply_strategy("Be friendly and helpful")
            .enable_dm();

        assert!(ctx.product_prompt.is_some());
        assert!(ctx.target_audience.is_some());
        assert!(ctx.generate_dm);
        assert!(!ctx.generate_post_reply);
    }

    #[test]
    fn test_system_prompt_building() {
        let ctx = AnalysisContext::new()
            .with_product_prompt("Fitness app")
            .with_language("English");

        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Fitness app"));
        assert!(prompt.contains("English"));
    }

    #[test]
    fn test_usage_stats() {
        let mut stats = AiUsageStats::default();
        stats.add_tokens(100, 50);
        stats.record_success(150);

        assert_eq!(stats.total_tokens, 300);
        assert_eq!(stats.success_count, 1);
    }
}
