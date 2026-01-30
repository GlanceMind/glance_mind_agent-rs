//! Mock AI Analyzer for testing

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::domain::{Comment, Content, ReplySuggestion, CommentIntent, Sentiment};
use crate::domain::errors::{AiError, AiResult};
use crate::ports::{AiAnalyzer, ai_analyzer::AnalysisContext};

/// Mock AI analyzer for testing
pub struct MockAiAnalyzer {
    /// Pre-configured responses by comment ID
    responses: RwLock<HashMap<String, ReplySuggestion>>,
    /// Default response generator mode
    mode: RwLock<MockAiMode>,
    /// Simulated delay in milliseconds
    delay_ms: RwLock<u64>,
    /// Whether to simulate errors
    error_mode: RwLock<Option<MockAiError>>,
    /// Call tracking
    calls: RwLock<Vec<AiCall>>,
    /// Token usage tracking
    total_tokens: RwLock<i32>,
}

/// Mock AI response mode
#[derive(Debug, Clone)]
pub enum MockAiMode {
    /// Generate simple thank-you responses
    Simple,
    /// Generate responses based on detected intent
    Smart,
    /// Return empty responses
    Empty,
    /// Return custom template
    Template(String),
}

/// Simulated AI errors
#[derive(Debug, Clone)]
pub enum MockAiError {
    Network,
    RateLimit,
    TokenLimit,
    InvalidInput,
}

/// Tracked AI call
#[derive(Debug, Clone)]
pub struct AiCall {
    pub comment_ids: Vec<String>,
    pub content_id: String,
    pub timestamp: std::time::Instant,
}

impl MockAiAnalyzer {
    /// Create a new mock AI analyzer
    pub fn new() -> Self {
        Self {
            responses: RwLock::new(HashMap::new()),
            mode: RwLock::new(MockAiMode::Smart),
            delay_ms: RwLock::new(0),
            error_mode: RwLock::new(None),
            calls: RwLock::new(Vec::new()),
            total_tokens: RwLock::new(0),
        }
    }

    /// Create with simple mode
    pub fn simple() -> Self {
        let analyzer = Self::new();
        *analyzer.mode.write().unwrap() = MockAiMode::Simple;
        analyzer
    }

    /// Set a pre-configured response for a comment
    pub fn set_response(&self, comment_id: &str, response: ReplySuggestion) {
        let mut responses = self.responses.write().unwrap();
        responses.insert(comment_id.to_string(), response);
    }

    /// Set the response generation mode
    pub fn set_mode(&self, mode: MockAiMode) {
        *self.mode.write().unwrap() = mode;
    }

    /// Set simulated delay
    pub fn set_delay(&self, ms: u64) {
        *self.delay_ms.write().unwrap() = ms;
    }

    /// Set error mode
    pub fn set_error(&self, error: Option<MockAiError>) {
        *self.error_mode.write().unwrap() = error;
    }

    /// Get all tracked calls
    pub fn get_calls(&self) -> Vec<AiCall> {
        self.calls.read().unwrap().clone()
    }

    /// Get total tokens used
    pub fn get_total_tokens(&self) -> i32 {
        *self.total_tokens.read().unwrap()
    }

    /// Clear state
    pub fn clear(&self) {
        self.calls.write().unwrap().clear();
        *self.total_tokens.write().unwrap() = 0;
    }

    fn track_call(&self, comment_ids: Vec<String>, content_id: String) {
        self.calls.write().unwrap().push(AiCall {
            comment_ids,
            content_id,
            timestamp: std::time::Instant::now(),
        });
    }

    fn add_tokens(&self, tokens: i32) {
        *self.total_tokens.write().unwrap() += tokens;
    }

    fn check_error(&self) -> AiResult<()> {
        let mode = self.error_mode.read().unwrap();
        match mode.as_ref() {
            Some(MockAiError::Network) => Err(AiError::Network("Mock network error".into())),
            Some(MockAiError::RateLimit) => Err(AiError::RateLimited),
            Some(MockAiError::TokenLimit) => Err(AiError::TokenLimitExceeded { used: 10000, limit: 8000 }),
            Some(MockAiError::InvalidInput) => Err(AiError::InvalidInput("Mock invalid input".into())),
            None => Ok(()),
        }
    }

    fn generate_response(&self, comment: &Comment, _content: &Content) -> ReplySuggestion {
        // Check for pre-configured response
        let responses = self.responses.read().unwrap();
        if let Some(resp) = responses.get(&comment.comment_id) {
            return resp.clone();
        }

        let mode = self.mode.read().unwrap();
        match &*mode {
            MockAiMode::Simple => {
                ReplySuggestion::new(&comment.comment_id)
                    .with_reply("Thank you for your comment!")
                    .with_reason("Generic response")
                    .with_model_info("mock-simple", 50)
            }
            MockAiMode::Smart => {
                self.generate_smart_response(comment)
            }
            MockAiMode::Empty => {
                ReplySuggestion::new(&comment.comment_id)
            }
            MockAiMode::Template(template) => {
                let reply = template
                    .replace("{author}", &comment.author)
                    .replace("{text}", &comment.text);
                ReplySuggestion::new(&comment.comment_id)
                    .with_reply(reply)
                    .with_model_info("mock-template", 30)
            }
        }
    }

    fn generate_smart_response(&self, comment: &Comment) -> ReplySuggestion {
        let text_lower = comment.text.to_lowercase();
        
        // Detect intent
        let (intent, sentiment, reply) = if text_lower.contains('?') || text_lower.contains("how") || text_lower.contains("what") {
            (
                CommentIntent::Question,
                Sentiment::Neutral,
                format!("Great question! Let me help you with that. {}", self.answer_question(&text_lower)),
            )
        } else if text_lower.contains("buy") || text_lower.contains("price") || text_lower.contains("purchase") {
            (
                CommentIntent::PurchaseIntent,
                Sentiment::Positive,
                "Thanks for your interest! Check out our link in bio for more details and pricing.".to_string(),
            )
        } else if text_lower.contains("love") || text_lower.contains("great") || text_lower.contains("amazing") || text_lower.contains("awesome") {
            (
                CommentIntent::Praise,
                Sentiment::Positive,
                format!("Thank you so much, @{}! We really appreciate your support! 🙏", comment.author),
            )
        } else if text_lower.contains("bad") || text_lower.contains("hate") || text_lower.contains("terrible") {
            (
                CommentIntent::Complaint,
                Sentiment::Negative,
                "We're sorry to hear that. We'd love to make it right - please DM us!".to_string(),
            )
        } else {
            (
                CommentIntent::Chitchat,
                Sentiment::Neutral,
                format!("Thanks for stopping by, @{}! 😊", comment.author),
            )
        };

        ReplySuggestion::new(&comment.comment_id)
            .with_reply(reply)
            .with_intent(intent)
            .with_sentiment(sentiment)
            .with_reason(format!("Detected {} intent with {} sentiment", 
                format!("{:?}", intent).to_lowercase(),
                format!("{:?}", sentiment).to_lowercase()))
            .with_model_info("mock-smart", 100)
    }

    fn answer_question(&self, text: &str) -> &'static str {
        if text.contains("price") || text.contains("cost") {
            "Check out our pricing page linked in bio!"
        } else if text.contains("ship") || text.contains("delivery") {
            "We offer worldwide shipping! Usually 3-5 business days."
        } else if text.contains("size") {
            "We have sizes from XS to XXL. Check our size guide!"
        } else {
            "Feel free to DM us for more details!"
        }
    }
}

impl Default for MockAiAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AiAnalyzer for MockAiAnalyzer {
    async fn analyze_comment(
        &self,
        comment: &Comment,
        content: &Content,
        _context: &AnalysisContext,
    ) -> AiResult<ReplySuggestion> {
        self.check_error()?;
        
        // Simulate delay
        let delay = *self.delay_ms.read().unwrap();
        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        self.track_call(vec![comment.comment_id.clone()], content.content_id.clone());
        
        let response = self.generate_response(comment, content);
        self.add_tokens(response.tokens_used.unwrap_or(50));
        
        Ok(response)
    }

    async fn analyze_batch(
        &self,
        comments: &[Comment],
        content: &Content,
        _context: &AnalysisContext,
    ) -> AiResult<Vec<ReplySuggestion>> {
        self.check_error()?;

        // Simulate delay
        let delay = *self.delay_ms.read().unwrap();
        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        let comment_ids: Vec<String> = comments.iter().map(|c| c.comment_id.clone()).collect();
        self.track_call(comment_ids, content.content_id.clone());

        let mut results = Vec::with_capacity(comments.len());
        for comment in comments {
            let response = self.generate_response(comment, content);
            self.add_tokens(response.tokens_used.unwrap_or(50));
            results.push(response);
        }

        Ok(results)
    }

    async fn health_check(&self) -> AiResult<bool> {
        self.check_error()?;
        Ok(true)
    }

    fn model_name(&self) -> &str {
        "mock-ai"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_ai_simple() {
        let analyzer = MockAiAnalyzer::simple();
        
        let content = Content::new("mock", "v123");
        let comment = Comment::new("mock", "c1", "v123").with_text("Hello!");
        let context = AnalysisContext::new();

        let result = analyzer.analyze_comment(&comment, &content, &context).await.unwrap();
        
        assert!(result.reply_text.is_some());
        assert!(result.reply_text.unwrap().contains("Thank you"));
    }

    #[tokio::test]
    async fn test_mock_ai_smart_question() {
        let analyzer = MockAiAnalyzer::new();
        
        let content = Content::new("mock", "v123");
        let comment = Comment::new("mock", "c1", "v123").with_text("How much does it cost?");
        let context = AnalysisContext::new();

        let result = analyzer.analyze_comment(&comment, &content, &context).await.unwrap();
        
        assert_eq!(result.intent, Some(CommentIntent::Question));
        assert!(result.reply_text.unwrap().contains("question"));
    }

    #[tokio::test]
    async fn test_mock_ai_smart_praise() {
        let analyzer = MockAiAnalyzer::new();
        
        let content = Content::new("mock", "v123");
        let comment = Comment::new("mock", "c1", "v123")
            .with_author("fan123")
            .with_text("This is amazing!");
        let context = AnalysisContext::new();

        let result = analyzer.analyze_comment(&comment, &content, &context).await.unwrap();
        
        assert_eq!(result.intent, Some(CommentIntent::Praise));
        assert_eq!(result.sentiment, Some(Sentiment::Positive));
        assert!(result.reply_text.unwrap().contains("fan123"));
    }

    #[tokio::test]
    async fn test_mock_ai_custom_response() {
        let analyzer = MockAiAnalyzer::new();
        
        let custom = ReplySuggestion::new("c1")
            .with_reply("Custom response!")
            .with_intent(CommentIntent::PurchaseIntent);
        analyzer.set_response("c1", custom);

        let content = Content::new("mock", "v123");
        let comment = Comment::new("mock", "c1", "v123").with_text("Hello!");
        let context = AnalysisContext::new();

        let result = analyzer.analyze_comment(&comment, &content, &context).await.unwrap();
        
        assert_eq!(result.reply_text, Some("Custom response!".to_string()));
        assert_eq!(result.intent, Some(CommentIntent::PurchaseIntent));
    }

    #[tokio::test]
    async fn test_mock_ai_error() {
        let analyzer = MockAiAnalyzer::new();
        analyzer.set_error(Some(MockAiError::RateLimit));

        let content = Content::new("mock", "v123");
        let comment = Comment::new("mock", "c1", "v123").with_text("Test");
        let context = AnalysisContext::new();

        let result = analyzer.analyze_comment(&comment, &content, &context).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AiError::RateLimited));
    }

    #[tokio::test]
    async fn test_mock_ai_batch() {
        let analyzer = MockAiAnalyzer::new();
        
        let content = Content::new("mock", "v123");
        let comments = vec![
            Comment::new("mock", "c1", "v123").with_text("Great!"),
            Comment::new("mock", "c2", "v123").with_text("How much?"),
        ];
        let context = AnalysisContext::new();

        let results = analyzer.analyze_batch(&comments, &content, &context).await.unwrap();
        
        assert_eq!(results.len(), 2);
        assert!(analyzer.get_total_tokens() > 0);
    }
}
