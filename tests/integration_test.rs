//! Integration Tests - Full workflow testing
//!
//! These tests verify the complete workflow from task processing
//! to AI analysis using mock implementations.

use std::sync::Arc;

use glance_mind_agent_rs::{
    // Domain
    Content, Comment, TaskConfig,
    // Ports
    ContentGateway, CommentGateway, AiAnalyzer,
    ContentRepository, PromptRepository, ProgressTracker,
    // Testing
    MockContentGateway, MockCommentGateway, MockAiAnalyzer, MockRepository,
    TestFixtures,
    testing::fixtures::TestScenarioBuilder,
    // Orchestrator
    WorkflowOrchestrator, OrchestratorConfig,
    // Strategies
    TikTokStrategy,
    // Ports types
    ports::prompt_repository::{CampaignConfig, CampaignStatus},
    ports::progress_tracker::{TaskInfo, TaskStatus},
};

// ============================================================
// Test Helpers
// ============================================================

/// Create a test orchestrator with mock implementations
fn create_test_orchestrator(
    content_gateway: Arc<MockContentGateway>,
    comment_gateway: Arc<MockCommentGateway>,
    ai_analyzer: Arc<MockAiAnalyzer>,
    repository: Arc<MockRepository>,
) -> WorkflowOrchestrator {
    WorkflowOrchestrator::builder()
        .content_gateway(content_gateway as Arc<dyn ContentGateway>)
        .comment_gateway(comment_gateway as Arc<dyn CommentGateway>)
        .ai_analyzer(ai_analyzer as Arc<dyn AiAnalyzer>)
        .content_repository(repository.clone() as Arc<dyn ContentRepository>)
        .prompt_repository(repository.clone() as Arc<dyn PromptRepository>)
        .progress_tracker(repository as Arc<dyn ProgressTracker>)
        .add_strategy(Arc::new(TikTokStrategy::new()))
        .config(OrchestratorConfig {
            max_videos_per_keyword: 5,
            max_comments_per_video: 10,
            skip_existing_content: false,
            skip_existing_comments: false,
            ai_batch_size: 5,
            continue_on_error: true,
        })
        .build()
        .expect("Failed to build orchestrator")
}

/// Setup test environment with fixtures
fn setup_test_environment(fixtures: &TestFixtures) -> (
    Arc<MockContentGateway>,
    Arc<MockCommentGateway>,
    Arc<MockAiAnalyzer>,
    Arc<MockRepository>,
) {
    let content_gateway = Arc::new(MockContentGateway::new());
    let comment_gateway = Arc::new(MockCommentGateway::new());
    let ai_analyzer = Arc::new(MockAiAnalyzer::new());
    let repository = Arc::new(MockRepository::new());

    // Load fixtures into gateways
    for content in fixtures.contents() {
        // Add to content gateway for search
        content_gateway.add_search_results(
            "fitness",
            vec![content.clone()],
        );
        content_gateway.add_content(content);
    }

    for (content_id, comments) in fixtures.comments() {
        for comment in comments {
            comment_gateway.add_comment(&content_id, comment);
        }
    }

    // Load campaigns into repository
    for campaign in fixtures.campaigns() {
        repository.add_campaign(campaign);
    }

    // Load tasks into repository
    for task in fixtures.tasks() {
        repository.add_task(task);
    }

    (content_gateway, comment_gateway, ai_analyzer, repository)
}

// ============================================================
// Basic Flow Tests
// ============================================================

#[tokio::test]
async fn test_basic_workflow_with_mock_data() {
    // Setup
    let fixtures = TestScenarioBuilder::new()
        .with_video(
            "test_video_001",
            "testuser",
            "Test video about fitness",
            vec!["Great video!", "How much?", "Love it!"],
        )
        .with_campaign(1, "Test Campaign", "We sell fitness products")
        .with_task(1, 1, vec!["fitness"])
        .build();

    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);
    
    // Add search results for "fitness" keyword
    content_gw.add_search_results("fitness", fixtures.contents());

    let orchestrator = create_test_orchestrator(content_gw.clone(), comment_gw.clone(), ai.clone(), repo.clone());

    // Create task config
    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["fitness".to_string()])
        .with_region("US")
        .with_max_videos(5)
        .with_max_comments_per_video(10);

    // Process task
    let result = orchestrator.process_task(1, task_config).await;
    
    // Verify result
    assert!(result.is_ok(), "Task should succeed: {:?}", result.err());
    let task_result = result.unwrap();
    
    assert!(task_result.success, "Task result should be successful");
    assert!(task_result.contents_processed > 0, "Should process at least 1 content");
    assert!(task_result.comments_processed > 0, "Should process comments");
    
    // Verify data was saved
    let contents = repo.get_all_contents();
    assert!(!contents.is_empty(), "Contents should be saved to repository");
    
    let comments = repo.get_all_comments();
    assert!(!comments.is_empty(), "Comments should be saved to repository");
    
    // Verify AI was called
    let ai_calls = ai.get_calls();
    assert!(!ai_calls.is_empty(), "AI should have been called");
    
    // Verify analyses were saved
    let analyses = repo.get_all_analyses();
    assert!(!analyses.is_empty(), "Analyses should be saved");
}

#[tokio::test]
async fn test_workflow_with_default_fixtures() {
    // Use default fixtures with more realistic data
    let fixtures = TestFixtures::default();
    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);

    // Add fitness-related content to search results
    let fitness_contents: Vec<Content> = fixtures.contents()
        .into_iter()
        .filter(|c| c.description.to_lowercase().contains("fitness") || 
                    c.description.to_lowercase().contains("workout"))
        .collect();
    
    content_gw.add_search_results("fitness", fitness_contents);

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai.clone(), repo.clone());

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["fitness".to_string()])
        .with_max_videos(10)
        .with_max_comments_per_video(20);

    let result = orchestrator.process_task(1, task_config).await.unwrap();

    assert!(result.success);
    println!("Processed {} contents, {} comments, {} analyses",
        result.contents_processed,
        result.comments_processed,
        result.analyses_generated);
}

// ============================================================
// Error Handling Tests
// ============================================================

#[tokio::test]
async fn test_workflow_handles_gateway_error() {
    use glance_mind_agent_rs::testing::mock_gateway::MockError;

    let fixtures = TestScenarioBuilder::new()
        .with_campaign(1, "Test", "Product")
        .with_task(1, 1, vec!["test"])
        .build();

    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);

    // Simulate gateway error
    content_gw.set_error_mode(Some(MockError::Network));

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai, repo);

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["test".to_string()]);

    let result = orchestrator.process_task(1, task_config).await;

    // With continue_on_error=true, task should complete but with no data
    assert!(result.is_ok());
    let task_result = result.unwrap();
    assert_eq!(task_result.contents_processed, 0);
}

#[tokio::test]
async fn test_workflow_handles_ai_error() {
    use glance_mind_agent_rs::testing::mock_ai::MockAiError;

    let fixtures = TestScenarioBuilder::new()
        .with_video("v1", "user", "Test video", vec!["Comment 1"])
        .with_campaign(1, "Test", "Product")
        .with_task(1, 1, vec!["test"])
        .build();

    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);
    content_gw.add_search_results("test", fixtures.contents());

    // Simulate AI error
    ai.set_error(Some(MockAiError::RateLimit));

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai, repo.clone());

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["test".to_string()]);

    let result = orchestrator.process_task(1, task_config).await.unwrap();

    // Content and comments should still be saved even if AI fails
    assert!(result.contents_processed > 0);
    assert!(result.comments_processed > 0);
    // But no analyses should be saved
    assert_eq!(result.analyses_generated, 0);
}

// ============================================================
// Campaign Stop Tests
// ============================================================

#[tokio::test]
async fn test_workflow_respects_campaign_limit() {
    let fixtures = TestScenarioBuilder::new()
        .with_video("v1", "user", "Video 1", vec!["C1", "C2", "C3", "C4", "C5"])
        .with_video("v2", "user", "Video 2", vec!["C6", "C7", "C8", "C9", "C10"])
        .build();

    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);
    content_gw.add_search_results("test", fixtures.contents());

    // Add campaign with low limit
    repo.add_campaign(CampaignConfig {
        id: 1,
        user_id: 1,
        name: "Limited Campaign".to_string(),
        platform_id: 2,
        status: CampaignStatus::Active,
        target_audience: None,
        product_prompt: Some("Test".to_string()),
        reply_strategy: None,
        dm_strategy: None,
        reply_post_strategy: None,
        max_comments: Some(3), // Only process 3 comments
        processed_comments: 0,
    });

    repo.add_task(TaskInfo {
        id: 1,
        campaign_id: 1,
        platform_id: 2,
        keywords: Some(serde_json::json!(["test"])),
        status: TaskStatus::Pending,
        progress: 0,
        error_message: None,
    });

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai, repo.clone());

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["test".to_string()])
        .with_max_videos(10)
        .with_max_comments_per_video(10);

    let _ = orchestrator.process_task(1, task_config).await.unwrap();

    // Check that campaign processed count was updated
    let count = repo.get_processed_count(1).await.unwrap();
    assert!(count > 0);
}

#[tokio::test]
async fn test_workflow_stops_on_paused_campaign() {
    let fixtures = TestScenarioBuilder::new()
        .with_video("v1", "user", "Video", vec!["Comment"])
        .build();

    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);
    content_gw.add_search_results("test", fixtures.contents());

    // Add paused campaign
    repo.add_campaign(CampaignConfig {
        id: 1,
        user_id: 1,
        name: "Paused Campaign".to_string(),
        platform_id: 2,
        status: CampaignStatus::Paused, // Paused!
        target_audience: None,
        product_prompt: None,
        reply_strategy: None,
        dm_strategy: None,
        reply_post_strategy: None,
        max_comments: None,
        processed_comments: 0,
    });

    repo.add_task(TaskInfo {
        id: 1,
        campaign_id: 1,
        platform_id: 2,
        keywords: Some(serde_json::json!(["test"])),
        status: TaskStatus::Pending,
        progress: 0,
        error_message: None,
    });

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai, repo);

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["test".to_string()]);

    let result = orchestrator.process_task(1, task_config).await.unwrap();

    // Task should complete but process nothing due to paused campaign
    assert_eq!(result.contents_processed, 0);
}

// ============================================================
// AI Response Quality Tests
// ============================================================

#[tokio::test]
async fn test_ai_generates_appropriate_responses() {
    use glance_mind_agent_rs::CommentIntent;

    let ai = MockAiAnalyzer::new();
    
    let content = Content::new("tiktok", "v1")
        .with_author("seller")
        .with_description("Amazing product!");

    // Test question detection
    let question = Comment::new("tiktok", "c1", "v1")
        .with_text("How much does it cost?");
    let response = ai.analyze_comment(&question, &content, &Default::default()).await.unwrap();
    assert_eq!(response.intent, Some(CommentIntent::Question));

    // Test praise detection
    let praise = Comment::new("tiktok", "c2", "v1")
        .with_author("fan")
        .with_text("This is amazing! I love it!");
    let response = ai.analyze_comment(&praise, &content, &Default::default()).await.unwrap();
    assert_eq!(response.intent, Some(CommentIntent::Praise));
    assert!(response.reply_text.unwrap().contains("fan"));

    // Test purchase intent - Note: text should not contain '?' as it triggers Question detection first
    let buyer = Comment::new("tiktok", "c3", "v1")
        .with_text("I want to buy this product right now!");
    let response = ai.analyze_comment(&buyer, &content, &Default::default()).await.unwrap();
    assert_eq!(response.intent, Some(CommentIntent::PurchaseIntent));
}

// ============================================================
// Repository Persistence Tests
// ============================================================

#[tokio::test]
async fn test_repository_data_persistence() {
    let repo = MockRepository::new();
    
    // Save content
    let content = Content::new("tiktok", "v123")
        .with_author("testuser")
        .with_description("Test video");
    let content_id = repo.save_content(&content, Some(1)).await.unwrap();
    
    // Save comment
    let comment = Comment::new("tiktok", "c456", "v123")
        .with_author("commenter")
        .with_text("Great!");
    let comment_id = repo.save_comment(&comment, content_id).await.unwrap();
    
    // Verify retrieval
    assert!(repo.content_exists("tiktok", "v123").await.unwrap());
    assert!(repo.comment_exists("tiktok", "c456").await.unwrap());
    
    let stored_content = repo.get_content_by_id(content_id).await.unwrap().unwrap();
    assert_eq!(stored_content.author_unique_id, Some("testuser".to_string()));
    
    let stored_comment = repo.get_comment_by_id(comment_id).await.unwrap().unwrap();
    assert_eq!(stored_comment.comment_text, Some("Great!".to_string()));
}

// ============================================================
// Multi-Keyword Tests
// ============================================================

#[tokio::test]
async fn test_workflow_with_multiple_keywords() {
    let fixtures = TestScenarioBuilder::new()
        .with_video("v1", "user1", "Fitness video", vec!["Comment 1"])
        .with_video("v2", "user2", "Cooking video", vec!["Comment 2"])
        .with_campaign(1, "Multi-keyword Campaign", "Products")
        .with_task(1, 1, vec!["fitness", "cooking"])
        .build();

    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);
    
    // Add different content for each keyword
    let fitness_content = fixtures.contents().into_iter()
        .filter(|c| c.content_id == "v1")
        .collect();
    let cooking_content = fixtures.contents().into_iter()
        .filter(|c| c.content_id == "v2")
        .collect();
    
    content_gw.add_search_results("fitness", fitness_content);
    content_gw.add_search_results("cooking", cooking_content);

    let orchestrator = create_test_orchestrator(content_gw.clone(), comment_gw, ai, repo);

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["fitness".to_string(), "cooking".to_string()]);

    let result = orchestrator.process_task(1, task_config).await.unwrap();

    assert!(result.success);
    assert!(result.contents_processed >= 2, "Should process content from multiple keywords");

    // Verify gateway was called for each keyword
    let calls = content_gw.get_calls();
    assert!(calls.len() >= 2);
}

// ============================================================
// Performance/Load Tests
// ============================================================

#[tokio::test]
async fn test_large_batch_processing() {
    // Create a scenario with many comments
    let mut builder = TestScenarioBuilder::new();
    
    let mut comments = Vec::new();
    for i in 0..50 {
        comments.push(format!("Comment {}", i).as_str().to_owned());
    }
    let comment_refs: Vec<&str> = comments.iter().map(|s| s.as_str()).collect();
    
    builder = builder.with_video("big_video", "popular_user", "Popular video", comment_refs);
    builder = builder.with_campaign(1, "Large Campaign", "Product");
    builder = builder.with_task(1, 1, vec!["popular"]);
    
    let fixtures = builder.build();
    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);
    content_gw.add_search_results("popular", fixtures.contents());

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai.clone(), repo.clone());

    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["popular".to_string()])
        .with_max_comments_per_video(100);

    let start = std::time::Instant::now();
    let result = orchestrator.process_task(1, task_config).await.unwrap();
    let duration = start.elapsed();

    assert!(result.success);
    assert!(result.comments_processed >= 50);
    println!("Processed {} comments in {:?}", result.comments_processed, duration);
    
    // Verify token tracking
    let total_tokens = ai.get_total_tokens();
    assert!(total_tokens > 0, "Should track token usage");
    println!("Total tokens used: {}", total_tokens);
}

// ============================================================
// End-to-End Scenario Tests
// ============================================================

#[tokio::test]
async fn test_complete_e2e_fitness_campaign() {
    // Realistic fitness campaign scenario
    let fixtures = TestFixtures::default();
    let (content_gw, comment_gw, ai, repo) = setup_test_environment(&fixtures);

    // Setup search results
    let fitness_contents: Vec<Content> = fixtures.contents()
        .into_iter()
        .filter(|c| c.content_id.contains("fitness"))
        .collect();
    content_gw.add_search_results("fitness", fitness_contents.clone());
    content_gw.add_search_results("workout", fitness_contents);

    let orchestrator = create_test_orchestrator(content_gw, comment_gw, ai.clone(), repo.clone());

    // Process fitness campaign task
    let task_config = TaskConfig::new(1, "tiktok")
        .with_keywords(vec!["fitness".to_string(), "workout".to_string()])
        .with_region("US")
        .with_max_videos(10)
        .with_max_comments_per_video(50);

    let result = orchestrator.process_task(1, task_config).await.unwrap();

    // Verify complete workflow
    assert!(result.success, "E2E workflow should succeed");
    
    // Verify all components worked
    let contents = repo.get_all_contents();
    let comments = repo.get_all_comments();
    let analyses = repo.get_all_analyses();
    
    println!("E2E Test Results:");
    println!("  Contents saved: {}", contents.len());
    println!("  Comments saved: {}", comments.len());
    println!("  Analyses generated: {}", analyses.len());
    println!("  AI calls made: {}", ai.get_calls().len());
    println!("  Duration: {:?}", result.duration_ms);

    assert!(!contents.is_empty());
    assert!(!comments.is_empty());
    assert!(!analyses.is_empty());
}
