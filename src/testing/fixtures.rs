//! Test Fixtures - Pre-built test data for testing

use crate::domain::{Content, Comment, Engagement};
use crate::ports::prompt_repository::{CampaignConfig, CampaignStatus};
use crate::ports::progress_tracker::{TaskInfo, TaskStatus};

/// Pre-built test fixtures
#[derive(Debug, Clone)]
pub struct TestFixtures {
    contents: Vec<Content>,
    comments: Vec<(String, Vec<Comment>)>,
    campaigns: Vec<CampaignConfig>,
    tasks: Vec<TaskInfo>,
}

impl TestFixtures {
    /// Create empty fixtures
    pub fn empty() -> Self {
        Self {
            contents: Vec::new(),
            comments: Vec::new(),
            campaigns: Vec::new(),
            tasks: Vec::new(),
        }
    }

    /// Create default test fixtures
    pub fn default() -> Self {
        let mut fixtures = Self::empty();
        
        // Add sample videos
        fixtures.add_fitness_videos();
        fixtures.add_cooking_videos();
        
        // Add sample campaigns
        fixtures.add_sample_campaigns();
        
        // Add sample tasks
        fixtures.add_sample_tasks();
        
        fixtures
    }

    /// Get all contents
    pub fn contents(&self) -> Vec<Content> {
        self.contents.clone()
    }

    /// Get all comments by content ID
    pub fn comments(&self) -> Vec<(String, Vec<Comment>)> {
        self.comments.clone()
    }

    /// Get all campaigns
    pub fn campaigns(&self) -> Vec<CampaignConfig> {
        self.campaigns.clone()
    }

    /// Get all tasks
    pub fn tasks(&self) -> Vec<TaskInfo> {
        self.tasks.clone()
    }

    /// Add a content
    pub fn add_content(&mut self, content: Content) {
        self.contents.push(content);
    }

    /// Add comments for a content
    pub fn add_comments(&mut self, content_id: &str, comments: Vec<Comment>) {
        self.comments.push((content_id.to_string(), comments));
    }

    /// Add a campaign
    pub fn add_campaign(&mut self, campaign: CampaignConfig) {
        self.campaigns.push(campaign);
    }

    /// Add a task
    pub fn add_task(&mut self, task: TaskInfo) {
        self.tasks.push(task);
    }

    // ==================== Sample Data Builders ====================

    fn add_fitness_videos(&mut self) {
        // Fitness video 1
        let content1 = Content::new("tiktok", "fitness_video_001")
            .with_author("fitnessguru")
            .with_author_name("Fitness Guru")
            .with_description("5 minute morning workout routine! 💪 #fitness #workout #morning")
            .with_engagement(Engagement {
                likes: 15000,
                comments: 523,
                shares: 1200,
                views: 250000,
            })
            .with_created_at(1700000000);
        
        let comments1 = vec![
            Comment::new("tiktok", "fc_001", "fitness_video_001")
                .with_author("user123")
                .with_author_name("John Doe")
                .with_text("This is amazing! How many calories does this burn?")
                .with_likes(45),
            Comment::new("tiktok", "fc_002", "fitness_video_001")
                .with_author("fitnesslover")
                .with_author_name("Fit Lover")
                .with_text("I love your videos! Do you have a longer version?")
                .with_likes(32),
            Comment::new("tiktok", "fc_003", "fitness_video_001")
                .with_author("newbie2024")
                .with_author_name("Fitness Newbie")
                .with_text("Where can I buy those resistance bands?")
                .with_likes(28),
            Comment::new("tiktok", "fc_004", "fitness_video_001")
                .with_author("hater99")
                .with_author_name("Hater")
                .with_text("This is too easy, waste of time")
                .with_likes(5),
            Comment::new("tiktok", "fc_005", "fitness_video_001")
                .with_author("motivated_mom")
                .with_author_name("Motivated Mom")
                .with_text("Just did this with my kids! Great family workout 🏃‍♀️")
                .with_likes(120),
        ];
        
        self.add_content(content1);
        self.add_comments("fitness_video_001", comments1);

        // Fitness video 2
        let content2 = Content::new("tiktok", "fitness_video_002")
            .with_author("fitnessguru")
            .with_author_name("Fitness Guru")
            .with_description("No equipment home workout for beginners #fitness #noequipment")
            .with_engagement(Engagement {
                likes: 8500,
                comments: 234,
                shares: 650,
                views: 120000,
            });
        
        let comments2 = vec![
            Comment::new("tiktok", "fc_006", "fitness_video_002")
                .with_author("beginner_betty")
                .with_text("Perfect for someone just starting out! Thank you!")
                .with_likes(88),
            Comment::new("tiktok", "fc_007", "fitness_video_002")
                .with_author("curious_cat")
                .with_text("How often should I do this workout?")
                .with_likes(25),
        ];
        
        self.add_content(content2);
        self.add_comments("fitness_video_002", comments2);
    }

    fn add_cooking_videos(&mut self) {
        let content = Content::new("tiktok", "cooking_video_001")
            .with_author("chefmaster")
            .with_author_name("Chef Master")
            .with_description("Quick 15-minute pasta recipe 🍝 #cooking #pasta #easy")
            .with_engagement(Engagement {
                likes: 25000,
                comments: 890,
                shares: 3500,
                views: 500000,
            });
        
        let comments = vec![
            Comment::new("tiktok", "cc_001", "cooking_video_001")
                .with_author("foodie_fan")
                .with_text("This looks delicious! What's the full ingredient list?")
                .with_likes(156),
            Comment::new("tiktok", "cc_002", "cooking_video_001")
                .with_author("mom_of_3")
                .with_text("My kids loved this! Any kid-friendly variations?")
                .with_likes(89),
            Comment::new("tiktok", "cc_003", "cooking_video_001")
                .with_author("vegan_viewer")
                .with_text("Can you make a vegan version of this?")
                .with_likes(45),
        ];
        
        self.add_content(content);
        self.add_comments("cooking_video_001", comments);
    }

    fn add_sample_campaigns(&mut self) {
        // Active fitness campaign
        self.add_campaign(CampaignConfig {
            id: 1,
            user_id: 100,
            name: "Fitness App Launch".to_string(),
            platform_id: 2,
            status: CampaignStatus::Active,
            target_audience: Some("Health-conscious adults aged 18-45".to_string()),
            product_prompt: Some(
                "We are launching FitLife, a fitness tracking app that helps users \
                track workouts, count calories, and achieve their fitness goals. \
                The app is available on iOS and Android for $4.99/month.".to_string()
            ),
            reply_strategy: Some(
                "Be friendly and helpful. Answer questions about fitness. \
                When appropriate, mention our app as a solution.".to_string()
            ),
            dm_strategy: Some(
                "Send a personalized DM offering a free trial of the app.".to_string()
            ),
            reply_post_strategy: None,
            max_comments: Some(1000),
            processed_comments: 0,
        });

        // Cooking product campaign
        self.add_campaign(CampaignConfig {
            id: 2,
            user_id: 100,
            name: "Kitchen Tools Promo".to_string(),
            platform_id: 2,
            status: CampaignStatus::Active,
            target_audience: Some("Home cooks and cooking enthusiasts".to_string()),
            product_prompt: Some(
                "We sell premium kitchen tools including chef knives, cutting boards, \
                and cookware sets. Quality guaranteed with free shipping over $50.".to_string()
            ),
            reply_strategy: Some(
                "Engage with cooking content. Share tips and recommend our products \
                when relevant.".to_string()
            ),
            dm_strategy: None,
            reply_post_strategy: None,
            max_comments: Some(500),
            processed_comments: 0,
        });

        // Paused campaign
        self.add_campaign(CampaignConfig {
            id: 3,
            user_id: 100,
            name: "Paused Campaign".to_string(),
            platform_id: 2,
            status: CampaignStatus::Paused,
            target_audience: None,
            product_prompt: None,
            reply_strategy: None,
            dm_strategy: None,
            reply_post_strategy: None,
            max_comments: Some(100),
            processed_comments: 50,
        });
    }

    fn add_sample_tasks(&mut self) {
        // Pending task for fitness campaign
        self.add_task(TaskInfo {
            id: 1,
            campaign_id: 1,
            platform_id: 2,
            keywords: Some(serde_json::json!(["fitness", "workout", "@fitnessguru"])),
            status: TaskStatus::Pending,
            progress: 0,
            error_message: None,
        });

        // Running task
        self.add_task(TaskInfo {
            id: 2,
            campaign_id: 1,
            platform_id: 2,
            keywords: Some(serde_json::json!(["gym", "exercise"])),
            status: TaskStatus::Running,
            progress: 45,
            error_message: None,
        });

        // Completed task
        self.add_task(TaskInfo {
            id: 3,
            campaign_id: 2,
            platform_id: 2,
            keywords: Some(serde_json::json!(["cooking", "recipe"])),
            status: TaskStatus::Completed,
            progress: 100,
            error_message: None,
        });

        // Failed task
        self.add_task(TaskInfo {
            id: 4,
            campaign_id: 3,
            platform_id: 2,
            keywords: Some(serde_json::json!(["test"])),
            status: TaskStatus::Failed,
            progress: 20,
            error_message: Some("Rate limit exceeded".to_string()),
        });
    }
}

/// Builder for creating custom test scenarios
pub struct TestScenarioBuilder {
    fixtures: TestFixtures,
}

impl TestScenarioBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            fixtures: TestFixtures::empty(),
        }
    }

    /// Add a video with comments
    pub fn with_video(
        mut self,
        content_id: &str,
        author: &str,
        description: &str,
        comment_texts: Vec<&str>,
    ) -> Self {
        let content = Content::new("tiktok", content_id)
            .with_author(author)
            .with_description(description)
            .with_engagement(Engagement {
                likes: 1000,
                comments: comment_texts.len() as i64,
                shares: 50,
                views: 10000,
            });
        
        let comments: Vec<Comment> = comment_texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                Comment::new("tiktok", &format!("{}_{}", content_id, i), content_id)
                    .with_author(&format!("user_{}", i))
                    .with_text(*text)
                    .with_likes((i * 10) as i64)
            })
            .collect();
        
        self.fixtures.add_content(content);
        self.fixtures.add_comments(content_id, comments);
        self
    }

    /// Add a campaign
    pub fn with_campaign(
        mut self,
        id: i32,
        name: &str,
        product_prompt: &str,
    ) -> Self {
        self.fixtures.add_campaign(CampaignConfig {
            id,
            user_id: 1,
            name: name.to_string(),
            platform_id: 2,
            status: CampaignStatus::Active,
            target_audience: None,
            product_prompt: Some(product_prompt.to_string()),
            reply_strategy: None,
            dm_strategy: None,
            reply_post_strategy: None,
            max_comments: Some(1000),
            processed_comments: 0,
        });
        self
    }

    /// Add a task
    pub fn with_task(mut self, task_id: i64, campaign_id: i32, keywords: Vec<&str>) -> Self {
        self.fixtures.add_task(TaskInfo {
            id: task_id,
            campaign_id,
            platform_id: 2,
            keywords: Some(serde_json::json!(keywords)),
            status: TaskStatus::Pending,
            progress: 0,
            error_message: None,
        });
        self
    }

    /// Build the fixtures
    pub fn build(self) -> TestFixtures {
        self.fixtures
    }
}

impl Default for TestScenarioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fixtures() {
        let fixtures = TestFixtures::default();
        
        assert!(!fixtures.contents().is_empty());
        assert!(!fixtures.comments().is_empty());
        assert!(!fixtures.campaigns().is_empty());
        assert!(!fixtures.tasks().is_empty());
    }

    #[test]
    fn test_scenario_builder() {
        let fixtures = TestScenarioBuilder::new()
            .with_video(
                "test_video",
                "testuser",
                "Test video",
                vec!["Comment 1", "Comment 2"],
            )
            .with_campaign(1, "Test Campaign", "Test product")
            .with_task(1, 1, vec!["test"])
            .build();

        assert_eq!(fixtures.contents().len(), 1);
        assert_eq!(fixtures.comments().len(), 1);
        assert_eq!(fixtures.comments()[0].1.len(), 2);
        assert_eq!(fixtures.campaigns().len(), 1);
        assert_eq!(fixtures.tasks().len(), 1);
    }
}
