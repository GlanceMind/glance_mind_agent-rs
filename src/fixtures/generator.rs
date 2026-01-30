//! Fixture Generator - Fetch real data from TikHub API and save as test fixtures

use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tracing::{info, warn};
use chrono::Utc;

use crate::tikhub::{TikHubClient, SearchParams, UserVideoParams};
use crate::error::{Error, Result};
use super::{TikTokSearchFixture, TikTokCommentsFixture, TikTokUserVideosFixture};

/// Fixture Generator
/// 
/// Generates test fixtures by calling real TikHub API and saving responses.
/// 
/// # Example
/// 
/// ```no_run
/// use glance_mind_agent_rs::fixtures::FixtureGenerator;
/// 
/// #[tokio::main]
/// async fn main() {
///     let generator = FixtureGenerator::from_env().unwrap();
///     
///     // Generate a search fixture
///     let path = generator.generate_search_fixture("travel", "US", 5).await.unwrap();
///     println!("Saved to: {}", path.display());
/// }
/// ```
pub struct FixtureGenerator {
    client: TikHubClient,
    output_dir: PathBuf,
}

impl FixtureGenerator {
    /// Create a new fixture generator
    pub fn new(client: TikHubClient, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            client,
            output_dir: output_dir.into(),
        }
    }

    /// Create a fixture generator from environment variables
    /// 
    /// Output directory defaults to `tests/fixtures/tiktok`
    pub fn from_env() -> Result<Self> {
        let client = TikHubClient::from_env()
            .map_err(|e| Error::Config(e.to_string()))?;
        
        Ok(Self::new(client, "tests/fixtures/tiktok"))
    }

    /// Create a fixture generator with custom output directory
    pub fn from_env_with_dir(output_dir: impl Into<PathBuf>) -> Result<Self> {
        let client = TikHubClient::from_env()
            .map_err(|e| Error::Config(e.to_string()))?;
        
        Ok(Self::new(client, output_dir))
    }

    /// Ensure output directory exists
    async fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.output_dir).await?;
        Ok(())
    }

    /// Generate a safe filename from keyword
    fn safe_filename(keyword: &str) -> String {
        keyword
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Generate a video search fixture
    /// 
    /// Calls TikHub search API and saves the response as a JSON fixture file.
    pub async fn generate_search_fixture(
        &self,
        keyword: &str,
        region: &str,
        count: u32,
    ) -> Result<PathBuf> {
        self.ensure_dir().await?;

        info!("🔍 Generating search fixture: keyword='{}', region={}, count={}", keyword, region, count);

        let params = SearchParams::new(keyword)
            .with_region(region)
            .with_count(count);

        let response = self.client.search_videos(&params).await
            .map_err(|e| Error::Config(format!("TikHub API error: {}", e)))?;

        // Extract videos
        let videos: Vec<serde_json::Value> = response.data
            .as_ref()
            .and_then(|d| d.search_item_list.as_ref())
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.aweme_info.as_ref())
                    .filter_map(|info| serde_json::to_value(info).ok())
                    .collect()
            })
            .unwrap_or_default();

        info!("✅ Found {} videos", videos.len());

        // Build fixture
        let fixture = TikTokSearchFixture {
            keyword: keyword.to_string(),
            region: region.to_string(),
            generated_at: Utc::now().to_rfc3339(),
            total_count: videos.len(),
            raw_response: serde_json::to_value(&response).unwrap_or_default(),
            videos,
        };

        // Save to file
        let safe_keyword = Self::safe_filename(keyword);
        let filename = format!("search_{}_{}.json", safe_keyword, region.to_lowercase());
        let filepath = self.output_dir.join(&filename);

        let json = serde_json::to_string_pretty(&fixture)?;
        fs::write(&filepath, &json).await?;

        info!("💾 Saved fixture to: {}", filepath.display());

        Ok(filepath)
    }

    /// Generate a comments fixture for a video
    /// 
    /// Fetches all comments (up to max_count) with automatic pagination.
    pub async fn generate_comments_fixture(
        &self,
        aweme_id: &str,
        max_count: u32,
    ) -> Result<PathBuf> {
        self.ensure_dir().await?;

        info!("💬 Generating comments fixture: aweme_id={}, max_count={}", aweme_id, max_count);

        let comments = self.client.fetch_all_comments(aweme_id, max_count).await
            .map_err(|e| Error::Config(format!("TikHub API error: {}", e)))?;

        info!("✅ Fetched {} comments", comments.len());

        // Convert to JSON values
        let comment_values: Vec<serde_json::Value> = comments
            .iter()
            .filter_map(|c| serde_json::to_value(c).ok())
            .collect();

        // Build fixture
        let fixture = TikTokCommentsFixture {
            aweme_id: aweme_id.to_string(),
            generated_at: Utc::now().to_rfc3339(),
            total_count: comment_values.len(),
            comments: comment_values,
        };

        // Save to file
        let filename = format!("comments_{}.json", aweme_id);
        let filepath = self.output_dir.join(&filename);

        let json = serde_json::to_string_pretty(&fixture)?;
        fs::write(&filepath, &json).await?;

        info!("💾 Saved fixture to: {}", filepath.display());

        Ok(filepath)
    }

    /// Generate a user videos fixture
    pub async fn generate_user_videos_fixture(
        &self,
        unique_id: &str,
        count: u32,
    ) -> Result<PathBuf> {
        self.ensure_dir().await?;

        info!("👤 Generating user videos fixture: @{}, count={}", unique_id, count);

        let params = UserVideoParams::by_unique_id(unique_id).with_count(count);

        let response = self.client.fetch_user_videos(&params).await
            .map_err(|e| Error::Config(format!("TikHub API error: {}", e)))?;

        // Extract videos
        let videos: Vec<serde_json::Value> = response.data
            .as_ref()
            .and_then(|d| d.aweme_list.as_ref())
            .map(|list| {
                list.iter()
                    .filter_map(|info| serde_json::to_value(info).ok())
                    .collect()
            })
            .unwrap_or_default();

        info!("✅ Found {} videos", videos.len());

        // Build fixture
        let fixture = TikTokUserVideosFixture {
            unique_id: unique_id.to_string(),
            generated_at: Utc::now().to_rfc3339(),
            total_count: videos.len(),
            raw_response: serde_json::to_value(&response).unwrap_or_default(),
            videos,
        };

        // Save to file
        let filename = format!("user_videos_{}.json", unique_id);
        let filepath = self.output_dir.join(&filename);

        let json = serde_json::to_string_pretty(&fixture)?;
        fs::write(&filepath, &json).await?;

        info!("💾 Saved fixture to: {}", filepath.display());

        Ok(filepath)
    }

    /// Generate a complete test dataset
    /// 
    /// This will:
    /// 1. Search for videos with multiple keywords
    /// 2. Fetch comments for some of those videos
    /// 3. Fetch videos from sample users
    pub async fn generate_full_dataset(&self) -> Result<()> {
        info!("🚀 Generating full TikTok test dataset...\n");

        // 1. Generate search fixtures
        let keywords = vec![
            ("travel", "US"),
            ("cooking", "US"),
            ("fitness", "GB"),
        ];

        let mut video_ids = Vec::new();

        for (keyword, region) in keywords {
            match self.generate_search_fixture(keyword, region, 5).await {
                Ok(path) => {
                    info!("  ✓ Generated: {}\n", path.display());
                    
                    // Extract some video IDs for comment fetching
                    if let Ok(content) = fs::read_to_string(&path).await {
                        if let Ok(fixture) = serde_json::from_str::<TikTokSearchFixture>(&content) {
                            for video in fixture.videos.iter().take(2) {
                                if let Some(id) = video.get("aweme_id").and_then(|v| v.as_str()) {
                                    video_ids.push(id.to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("  ✗ Failed for '{}': {}\n", keyword, e);
                }
            }

            // Small delay between requests
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // 2. Generate comments fixtures
        for video_id in video_ids.iter().take(3) {
            match self.generate_comments_fixture(video_id, 50).await {
                Ok(path) => info!("  ✓ Generated: {}\n", path.display()),
                Err(e) => warn!("  ✗ Failed for video {}: {}\n", video_id, e),
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // 3. Generate user videos fixtures
        let users = vec!["tiktok"];
        for user in users {
            match self.generate_user_videos_fixture(user, 5).await {
                Ok(path) => info!("  ✓ Generated: {}\n", path.display()),
                Err(e) => warn!("  ✗ Failed for @{}: {}\n", user, e),
            }
        }

        info!("\n✅ Dataset generation complete!");
        Ok(())
    }
}
