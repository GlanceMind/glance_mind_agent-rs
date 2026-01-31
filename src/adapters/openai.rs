//! OpenAI Adapter - Implements AiAnalyzer for OpenAI/DeepSeek
//!
//! This adapter provides AI-powered comment analysis using OpenAI-compatible APIs.
//! The prompt structure is synchronized with the Python glance_mind_agent.
//!
//! ## Rate Limiting
//!
//! Supports optional rate limiting via `RateLimiter` to prevent overwhelming the API.
//! When a rate limiter is configured, all API calls will acquire a permit before executing.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, trace, warn};

use crate::concurrency::RateLimiter;
use crate::domain::errors::{AiError, AiResult};
use crate::domain::{Comment, CommentIntent, Content, ReplySuggestion, Sentiment};
use crate::ports::{ai_analyzer::AnalysisContext, AiAnalyzer};

// ============================================================
// Retry Configuration Constants
// ============================================================

/// Default max retry attempts for AI API calls
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Base delay between retries in milliseconds
const DEFAULT_RETRY_BASE_DELAY_MS: u64 = 1000;

/// Max delay between retries in milliseconds
const DEFAULT_RETRY_MAX_DELAY_MS: u64 = 30000;

/// Retry configuration for AI API
#[derive(Debug, Clone)]
pub struct AiRetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay between retries (exponential backoff)
    pub base_delay_ms: u64,
    /// Maximum delay between retries
    pub max_delay_ms: u64,
}

impl Default for AiRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_delay_ms: DEFAULT_RETRY_BASE_DELAY_MS,
            max_delay_ms: DEFAULT_RETRY_MAX_DELAY_MS,
        }
    }
}

// ============================================================
// Configuration Constants (matching Python version)
// ============================================================

/// Max characters per comment (truncated if longer)
const MAX_COMMENT_LENGTH: usize = 300;

/// Max comments per batch for AI analysis
const MAX_COMMENTS_PER_BATCH: usize = 150;

/// Max video description length
const MAX_VIDEO_DESC_LENGTH: usize = 500;

/// Max total characters per batch (~15k tokens, leaving room for system prompt)
const MAX_TOTAL_CHARS: usize = 60000;

/// OpenAI adapter implementing AiAnalyzer
pub struct OpenAiAdapter {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    // Note: Python agent does NOT set max_tokens (uses API default)
    max_tokens: Option<i32>,
    temperature: f32,
    /// Optional rate limiter for controlling API call concurrency
    rate_limiter: Option<Arc<RateLimiter>>,
    /// Retry configuration for API calls
    retry_config: AiRetryConfig,
}

impl OpenAiAdapter {
    /// Create a new OpenAI adapter
    ///
    /// Note: Default base_url is SiliconFlow (matching Python glance_mind_agent)
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            // Default to SiliconFlow API (matching Python glance_mind_agent)
            base_url: base_url.unwrap_or_else(|| "https://api.siliconflow.cn/v1".to_string()),
            model: model.unwrap_or_else(|| "deepseek-ai/DeepSeek-V3".to_string()),
            // Note: Python agent does NOT set max_tokens (uses API default)
            // Setting to None to match Python behavior
            max_tokens: None,
            temperature: 0.7,
            rate_limiter: None,
            retry_config: AiRetryConfig::default(),
        }
    }

    /// Create from environment variables
    ///
    /// Environment variables (matching Python glance_mind_agent):
    /// - AGENT_API_KEY or OPENAI_API_KEY: API key for the AI service
    /// - AGENT_BASE_URL or OPENAI_BASE_URL: Base URL (default: https://api.siliconflow.cn/v1)
    /// - AI_MODEL: Model name (default: deepseek-ai/DeepSeek-V3)
    pub fn from_env() -> Result<Self, AiError> {
        // Try AGENT_API_KEY first (Python agent compatible), then OPENAI_API_KEY
        let api_key = std::env::var("AGENT_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map_err(|_| AiError::ServiceError("AGENT_API_KEY or OPENAI_API_KEY not set".into()))?;

        // Try AGENT_BASE_URL first (Python agent compatible), then OPENAI_BASE_URL
        // Default to SiliconFlow (matching Python agent)
        let base_url = std::env::var("AGENT_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .ok()
            .or_else(|| Some("https://api.siliconflow.cn/v1".to_string()));

        let model = std::env::var("AI_MODEL").ok();

        Ok(Self::new(api_key, base_url, model))
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set max tokens (None = use API default, matching Python agent behavior)
    pub fn with_max_tokens(mut self, max_tokens: Option<i32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set rate limiter for controlling API call concurrency
    ///
    /// When set, all API calls will acquire a permit from the limiter before executing.
    /// This helps prevent overwhelming the API with too many concurrent requests.
    pub fn with_rate_limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Check if rate limiter is configured
    pub fn has_rate_limiter(&self) -> bool {
        self.rate_limiter.is_some()
    }

    /// Set retry configuration
    pub fn with_retry_config(mut self, config: AiRetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Check if an error is retryable
    fn is_retryable_error(error: &AiError) -> bool {
        match error {
            AiError::RateLimited => true,
            AiError::Network(_) => true,
            AiError::ServiceError(msg) => {
                // Retry on 5xx errors and 403 (temporary rate limits)
                msg.contains("API error 5")
                    || msg.contains("API error 403")
                    || msg.contains("RPM limit")
                    || msg.contains("rate limit")
                    || msg.contains("temporarily")
            }
            _ => false,
        }
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, attempt: u32) -> u64 {
        let delay = self.retry_config.base_delay_ms * 2u64.pow(attempt);
        delay.min(self.retry_config.max_delay_ms)
    }

    /// Make API call with automatic retry on transient errors
    async fn call_api_with_retry(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> AiResult<String> {
        let mut last_error: Option<AiError> = None;

        for attempt in 0..=self.retry_config.max_retries {
            match self.call_api_once(system_prompt, user_prompt).await {
                Ok(result) => {
                    if attempt > 0 {
                        info!(attempt = attempt + 1, "AI API call succeeded after retry");
                    }
                    return Ok(result);
                }
                Err(e) => {
                    if !Self::is_retryable_error(&e) || attempt == self.retry_config.max_retries {
                        if attempt > 0 {
                            warn!(
                                attempt = attempt + 1,
                                max_attempts = self.retry_config.max_retries + 1,
                                error = %e,
                                "AI API call failed, not retrying"
                            );
                        }
                        return Err(e);
                    }

                    let delay_ms = self.calculate_retry_delay(attempt);
                    warn!(
                        attempt = attempt + 1,
                        max_attempts = self.retry_config.max_retries + 1,
                        delay_ms,
                        error = %e,
                        "AI API call failed, retrying..."
                    );

                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AiError::ServiceError("Unknown error".into())))
    }

    /// Build the system prompt (matching Python glance_mind_agent)
    fn build_system_prompt(&self, context: &AnalysisContext) -> String {
        let target_audience = context
            .target_audience
            .as_deref()
            .unwrap_or("General audience");
        let product_prompt = context
            .product_prompt
            .as_deref()
            .unwrap_or("Products/Services");
        let dm_strategy = context
            .dm_strategy
            .as_deref()
            .unwrap_or("Send personalized DM.");
        let reply_strategy = context
            .reply_strategy
            .as_deref()
            .unwrap_or("Be helpful and friendly.");
        let reply_post_strategy = context
            .reply_post_strategy
            .as_deref()
            .unwrap_or("Create engaging comment.");

        format!(
            r#"# Role
You are a senior social media growth and user conversion expert. Your core task is to analyze video comment sections and identify high-potential leads through a three-pronged parallel strategy: "Reply to Comments + Create Viral Comments + DM Conversion".

# Strategic Context
- **Target Audience:** {target_audience}
- **Product Prompt:** {product_prompt}
- **DM Strategy:** {dm_strategy}
- **Reply Strategy:** Public replies to specific comments, focusing on engagement and traffic. Strategy: {reply_strategy}
- **Viral Comment Strategy:** Create "top comments" to attract global attention. Strategy: {reply_post_strategy}

# Execution Logic
## Step 1: Lead Intent Identification
Filter users with clear needs, obvious pain points, or comments with "viral potential".

## Step 2: Conversion & Engagement Mechanism (for selected comments)
For selected comments, you **must** generate the following three items:

1. **Public Reply (suggested_reply):**
   - Follow the `Reply Strategy`. Be concise, friendly, and hint that DM has "treasures".

2. **DM Suggestion (suggested_dm):**
   - Follow the `DM Strategy`. **Use the commenter's real nickname** (user_nickname), no placeholders!
   - **Key Requirement:** Address the user by their nickname at the start, e.g., "Hey [real nickname]".

3. **Viral Comment Suggestion (suggested_reply_post):**
   - **Core Purpose:** Create a viral comment. Use relatable, provocative, or counter-intuitive views to attract likes and replies.
   - **Limit:** Only generate when the comment can resonate widely. **Max 3 non-null `suggested_reply_post` entries per response.**
   - **Writing Logic:** Use self-deprecating humor, one-line pain point summaries, or controversial questions.

# Language & Style Guidelines
- **Avoid AI Feel:** No robotic language. Use native social media language (e.g., "Slay/Mood/FR/no cap").
- **Create Tension:** Comments should be punchy and impactful.
- **Adaptive Logic:** For positive comments, "meme it up". For negative comments, use "elegant comeback" or "hard reversal".

# CRITICAL - Output Rules
- **ID Format:** Must return original numeric ID.
- **Username Usage:** In suggested_dm, **must use commenter's real nickname** (from User field in input), NO placeholders like {{{{user_name}}}}!
- **Binding Rule:** `suggested_reply` and `suggested_dm` must be generated together.
- **Quantity Limit:** Max 3 `suggested_reply_post` entries in entire JSON, extras should be `null`.

# Output Format (Strict JSON)
{{
  "suggestions": [
    {{
      "comment_id": "7327061675382260482",
      "reason": "User comment is highly representative, suitable as anchor for viral comment.",
      "suggested_reply": "This comment is exactly what I was thinking! Check your DMs for the full breakdown!",
      "suggested_dm": "Hey Sarah, saw your comment about [XX] and it's so relatable! I have a proven solution that not only solves [problem] but also [benefit]...",
      "suggested_reply_post": "Don't upvote this or the creator might see it and lose it... (just kidding, or am I?)"
    }},
    {{
      "comment_id": "7327061675382260483",
      "reason": "Standard inquiry, focus on DM conversion.",
      "suggested_reply": "Great question! Many people miss this detail. I'll DM you the comparison!",
      "suggested_dm": "Hi Mike, here's the comparison you asked for. Actually [product] has [XX optimization] here...",
      "suggested_reply_post": null
    }}
  ]
}}

If no suitable comments, return: {{"suggestions": []}}

# Input Data
"#
        )
    }

    /// Build the user prompt (matching Python glance_mind_agent format)
    fn build_user_prompt(
        &self,
        content: &Content,
        comments: &[Comment],
        batch_info: Option<&str>,
    ) -> String {
        // Truncate video description if too long (safely handle multi-byte UTF-8)
        let desc = if content.description.len() > MAX_VIDEO_DESC_LENGTH {
            let truncate_at = content
                .description
                .char_indices()
                .take_while(|(idx, _)| *idx < MAX_VIDEO_DESC_LENGTH)
                .last()
                .map(|(idx, c)| idx + c.len_utf8())
                .unwrap_or(0);
            format!("{}...", &content.description[..truncate_at])
        } else {
            content.description.clone()
        };

        let mut result = format!(
            "Video Description: {}\n\
            Author: {}\n",
            desc, content.author,
        );

        if let Some(info) = batch_info {
            result.push_str(&format!("Batch Info: {}\n", info));
        }

        result.push_str("\nComments:\n");

        let mut total_chars = result.len();
        let original_comment_count = comments.len();

        for (processed_count, comment) in comments.iter().enumerate() {
            // Check comment count limit
            if processed_count >= MAX_COMMENTS_PER_BATCH {
                let omitted = original_comment_count - processed_count;
                result.push_str(&format!(
                    "\n... ({} more comments omitted due to batch limit)\n",
                    omitted
                ));
                break;
            }

            // Truncate long comments (safely handle multi-byte UTF-8 characters)
            let content_text = if comment.text.len() > MAX_COMMENT_LENGTH {
                // Find a safe truncation point that doesn't split a multi-byte character
                let truncate_at = comment
                    .text
                    .char_indices()
                    .take_while(|(idx, _)| *idx < MAX_COMMENT_LENGTH)
                    .last()
                    .map(|(idx, c)| idx + c.len_utf8())
                    .unwrap_or(0);
                format!("{}...", &comment.text[..truncate_at])
            } else {
                comment.text.clone()
            };

            // Use author_name (nickname) if available, otherwise use author (unique_id)
            let user_nickname = comment.author_name.as_deref().unwrap_or(&comment.author);

            let comment_str = format!(
                "- ID: {}, User: {}, Content: {}\n",
                comment.comment_id, user_nickname, content_text,
            );

            // Check total character limit
            if total_chars + comment_str.len() > MAX_TOTAL_CHARS {
                let omitted = original_comment_count - processed_count;
                result.push_str(&format!(
                    "\n... ({} more comments omitted due to size limit)\n",
                    omitted
                ));
                debug!(
                    processed = processed_count,
                    total = original_comment_count,
                    "Reached character limit"
                );
                break;
            }

            result.push_str(&comment_str);
            total_chars += comment_str.len();
        }

        format!("Data:\n{}", result)
    }

    /// Parse the AI response (matching Python glance_mind_agent format)
    fn parse_response(&self, response: &str, _comments: &[Comment]) -> Vec<ReplySuggestion> {
        // Try to extract JSON from the response
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        debug!(json = %json_str, "Parsing AI response");

        // Parse the JSON (using "suggestions" key like Python version)
        match serde_json::from_str::<AgentOutput>(json_str) {
            Ok(parsed) => {
                parsed
                    .suggestions
                    .into_iter()
                    .map(|s| {
                        ReplySuggestion {
                            comment_id: s.comment_id,
                            reply_text: s.suggested_reply,
                            dm_text: s.suggested_dm,
                            post_reply_text: s.suggested_reply_post,
                            reason: s.reason,
                            confidence: None,
                            intent: None,    // Python version doesn't return intent
                            sentiment: None, // Python version doesn't return sentiment
                            tokens_used: None,
                            model: Some(self.model.clone()),
                        }
                    })
                    .collect()
            }
            Err(e) => {
                warn!(error = %e, response = %json_str, "Failed to parse AI response");
                // Return empty vec (matching Python behavior)
                vec![]
            }
        }
    }

    /// Make a single API call without retry (internal use)
    ///
    /// If a rate limiter is configured, acquires a permit before making the call.
    /// The permit is automatically released when the call completes (or fails).
    async fn call_api_once(&self, system_prompt: &str, user_prompt: &str) -> AiResult<String> {
        // Acquire rate limiter permit if configured
        // The _permit is held until end of function (RAII pattern)
        let _permit = if let Some(ref limiter) = self.rate_limiter {
            trace!("Acquiring AI rate limiter permit");
            Some(limiter.acquire().await)
        } else {
            None
        };

        let url = format!("{}/chat/completions", self.base_url);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            max_tokens: self.max_tokens,
            temperature: Some(self.temperature),
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
        };

        debug!(
            model = %self.model,
            has_rate_limiter = self.rate_limiter.is_some(),
            "Calling AI API"
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            if status.as_u16() == 429 {
                return Err(AiError::RateLimited);
            }

            return Err(AiError::ServiceError(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| AiError::ParseError(e.to_string()))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| AiError::ServiceError("Empty response from API".into()))
    }
}

#[async_trait]
impl AiAnalyzer for OpenAiAdapter {
    async fn analyze_comment(
        &self,
        comment: &Comment,
        content: &Content,
        context: &AnalysisContext,
    ) -> AiResult<ReplySuggestion> {
        let suggestions = self
            .analyze_batch(std::slice::from_ref(comment), content, context)
            .await?;
        suggestions
            .into_iter()
            .next()
            .ok_or_else(|| AiError::ServiceError("No suggestion generated".into()))
    }

    async fn analyze_batch(
        &self,
        comments: &[Comment],
        content: &Content,
        context: &AnalysisContext,
    ) -> AiResult<Vec<ReplySuggestion>> {
        if comments.is_empty() {
            return Ok(vec![]);
        }

        // Build prompts matching Python glance_mind_agent
        let system_prompt = self.build_system_prompt(context);
        let user_prompt = self.build_user_prompt(content, comments, None);

        debug!(
            comments_count = comments.len(),
            user_prompt_len = user_prompt.len(),
            "Sending request to AI"
        );

        let response = self
            .call_api_with_retry(&system_prompt, &user_prompt)
            .await?;

        debug!(response = %response, "Received AI response");

        let suggestions = self.parse_response(&response, comments);

        // Note: Python version doesn't require 1:1 mapping - AI selects high-potential comments
        debug!(
            input_comments = comments.len(),
            output_suggestions = suggestions.len(),
            "AI analysis complete"
        );

        Ok(suggestions)
    }

    async fn health_check(&self) -> AiResult<bool> {
        // Simple health check by making a minimal API call
        let test_request = ChatRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            max_tokens: Some(5),
            temperature: Some(0.0),
            response_format: None,
        };

        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&test_request)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        Ok(response.status().is_success())
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

// ============================================================
// API Types (matching Python glance_mind_agent)
// ============================================================

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    #[allow(dead_code)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Usage {
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
}

// ============================================================
// Output Types (matching Python glance_mind_agent models.py)
// ============================================================

/// Agent output format (matching Python AgentOutput)
#[derive(Debug, Deserialize)]
struct AgentOutput {
    suggestions: Vec<SuggestionItem>,
}

/// Reply suggestion format (matching Python ReplySuggestion)
#[derive(Debug, Deserialize)]
struct SuggestionItem {
    /// The ID of the potential customer's comment
    comment_id: String,

    /// Why this comment was selected as a potential customer
    reason: Option<String>,

    /// The suggested public reply content
    suggested_reply: Option<String>,

    /// Suggested direct message for follow-up (optional, AI may return null for some comments)
    suggested_dm: Option<String>,

    /// Suggested viral comment for hot reply (max 3 per task)
    suggested_reply_post: Option<String>,
}

// ============================================================
// Helper Functions
// ============================================================

#[allow(dead_code)]
fn parse_intent(s: &str) -> Option<CommentIntent> {
    match s.to_lowercase().as_str() {
        "question" => Some(CommentIntent::Question),
        "purchase_intent" => Some(CommentIntent::PurchaseIntent),
        "information_request" => Some(CommentIntent::InformationRequest),
        "feedback" => Some(CommentIntent::Feedback),
        "complaint" => Some(CommentIntent::Complaint),
        "praise" => Some(CommentIntent::Praise),
        "chitchat" => Some(CommentIntent::Chitchat),
        "spam" => Some(CommentIntent::Spam),
        _ => Some(CommentIntent::Unknown),
    }
}

#[allow(dead_code)]
fn parse_sentiment(s: &str) -> Option<Sentiment> {
    match s.to_lowercase().as_str() {
        "positive" => Some(Sentiment::Positive),
        "negative" => Some(Sentiment::Negative),
        "neutral" => Some(Sentiment::Neutral),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_intent() {
        assert_eq!(parse_intent("question"), Some(CommentIntent::Question));
        assert_eq!(parse_intent("PRAISE"), Some(CommentIntent::Praise));
        assert_eq!(
            parse_intent("purchase_intent"),
            Some(CommentIntent::PurchaseIntent)
        );
        assert_eq!(parse_intent("xyz"), Some(CommentIntent::Unknown));
    }

    #[test]
    fn test_parse_sentiment() {
        assert_eq!(parse_sentiment("positive"), Some(Sentiment::Positive));
        assert_eq!(parse_sentiment("NEGATIVE"), Some(Sentiment::Negative));
        assert_eq!(parse_sentiment("neutral"), Some(Sentiment::Neutral));
        assert_eq!(parse_sentiment("xyz"), None);
    }

    #[test]
    fn test_build_system_prompt() {
        let adapter = OpenAiAdapter::new("test-key", None, None);

        let context = AnalysisContext::new()
            .with_product_prompt("Fitness app")
            .with_target_audience("Young adults")
            .with_reply_strategy("Be friendly")
            .with_dm_strategy("Send personalized DM");

        let prompt = adapter.build_system_prompt(&context);

        // Check key sections from Python version
        assert!(prompt.contains("senior social media growth"));
        assert!(prompt.contains("Target Audience:"));
        assert!(prompt.contains("Young adults"));
        assert!(prompt.contains("Product Prompt:"));
        assert!(prompt.contains("Fitness app"));
        assert!(prompt.contains("DM Strategy:"));
        assert!(prompt.contains("Reply Strategy:"));
        assert!(prompt.contains("suggestions"));
        assert!(prompt.contains("suggested_reply"));
        assert!(prompt.contains("suggested_dm"));
        assert!(prompt.contains("suggested_reply_post"));
    }

    #[test]
    fn test_build_user_prompt() {
        let adapter = OpenAiAdapter::new("test-key", None, None);

        let content = Content::new("tiktok", "v123")
            .with_author("testuser")
            .with_description("Test video about fitness");

        let comments = vec![
            Comment::new("tiktok", "c1", "v123")
                .with_author("user1")
                .with_author_name("John Doe")
                .with_text("Great video!"),
            Comment::new("tiktok", "c2", "v123")
                .with_author("user2")
                .with_text("How much does it cost?"),
        ];

        let prompt = adapter.build_user_prompt(&content, &comments, None);

        // Check format matches Python version
        assert!(prompt.contains("Data:"));
        assert!(prompt.contains("Video Description:"));
        assert!(prompt.contains("Author: testuser"));
        assert!(prompt.contains("Comments:"));
        assert!(prompt.contains("ID: c1"));
        assert!(prompt.contains("User: John Doe")); // Uses nickname
        assert!(prompt.contains("Content: Great video!"));
        assert!(prompt.contains("ID: c2"));
        assert!(prompt.contains("User: user2")); // Falls back to author
    }

    #[test]
    fn test_build_user_prompt_with_batch_info() {
        let adapter = OpenAiAdapter::new("test-key", None, None);

        let content = Content::new("tiktok", "v123")
            .with_author("testuser")
            .with_description("Test video");

        let comments = vec![Comment::new("tiktok", "c1", "v123")
            .with_author("user1")
            .with_text("Comment")];

        let prompt = adapter.build_user_prompt(&content, &comments, Some("Batch 1/3"));

        assert!(prompt.contains("Batch Info: Batch 1/3"));
    }

    #[test]
    fn test_parse_response() {
        let adapter = OpenAiAdapter::new("test-key", None, None);

        let response = r#"{
            "suggestions": [
                {
                    "comment_id": "123",
                    "reason": "High potential lead",
                    "suggested_reply": "Check your DMs!",
                    "suggested_dm": "Hey John, I have something for you!",
                    "suggested_reply_post": null
                },
                {
                    "comment_id": "456",
                    "reason": "Viral potential",
                    "suggested_reply": "Great point!",
                    "suggested_dm": "Hi Sarah!",
                    "suggested_reply_post": "This is so relatable!"
                }
            ]
        }"#;

        let suggestions = adapter.parse_response(response, &[]);

        assert_eq!(suggestions.len(), 2);

        assert_eq!(suggestions[0].comment_id, "123");
        assert_eq!(
            suggestions[0].reply_text,
            Some("Check your DMs!".to_string())
        );
        assert_eq!(
            suggestions[0].dm_text,
            Some("Hey John, I have something for you!".to_string())
        );
        assert_eq!(suggestions[0].post_reply_text, None);

        assert_eq!(suggestions[1].comment_id, "456");
        assert_eq!(
            suggestions[1].post_reply_text,
            Some("This is so relatable!".to_string())
        );
    }

    #[test]
    fn test_parse_response_empty() {
        let adapter = OpenAiAdapter::new("test-key", None, None);

        let response = r#"{"suggestions": []}"#;
        let suggestions = adapter.parse_response(response, &[]);

        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_truncate_long_description() {
        let adapter = OpenAiAdapter::new("test-key", None, None);

        let long_desc = "a".repeat(600);
        let content = Content::new("tiktok", "v123")
            .with_author("user")
            .with_description(&long_desc);

        let prompt = adapter.build_user_prompt(&content, &[], None);

        // Should be truncated to MAX_VIDEO_DESC_LENGTH (500) + "..."
        assert!(prompt.contains(&"a".repeat(500)));
        assert!(prompt.contains("..."));
    }
}
