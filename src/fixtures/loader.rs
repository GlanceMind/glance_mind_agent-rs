//! Fixture Loader - Load test fixtures from JSON files

use std::path::{Path, PathBuf};
use tracing::debug;

use super::{TikTokCommentsFixture, TikTokSearchFixture, TikTokUserVideosFixture};
use crate::error::{Error, Result};

/// Fixture Loader
///
/// Loads test fixtures from JSON files for use in tests.
///
/// # Example
///
/// ```no_run
/// use glance_mind_agent_rs::fixtures::FixtureLoader;
///
/// let loader = FixtureLoader::from_cargo_test();
///
/// let fixture = loader.load_search_fixture("travel", "us").unwrap();
/// println!("Loaded {} videos", fixture.videos.len());
/// ```
pub struct FixtureLoader {
    base_dir: PathBuf,
}

impl FixtureLoader {
    /// Create a new fixture loader with a specific directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Create a fixture loader that works from Cargo test environment
    ///
    /// Uses CARGO_MANIFEST_DIR to locate the project root.
    pub fn from_cargo_test() -> Self {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        Self::new(format!("{}/tests/fixtures/tiktok", manifest_dir))
    }

    /// Create a fixture loader with a custom directory
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self::new(dir)
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Generate safe filename (same logic as generator)
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

    /// Load a search fixture
    ///
    /// # Arguments
    /// * `keyword` - The search keyword used to generate the fixture
    /// * `region` - The region code (e.g., "us", "gb")
    pub fn load_search_fixture(&self, keyword: &str, region: &str) -> Result<TikTokSearchFixture> {
        let safe_keyword = Self::safe_filename(keyword);
        let filename = format!("search_{}_{}.json", safe_keyword, region.to_lowercase());
        let path = self.base_dir.join(&filename);

        debug!("Loading search fixture from: {}", path.display());

        let content = std::fs::read_to_string(&path)
            .map_err(|_| Error::FixtureNotFound(path.display().to_string()))?;

        let fixture: TikTokSearchFixture = serde_json::from_str(&content)?;

        Ok(fixture)
    }

    /// Load a comments fixture
    ///
    /// # Arguments
    /// * `aweme_id` - The video ID
    pub fn load_comments_fixture(&self, aweme_id: &str) -> Result<TikTokCommentsFixture> {
        let filename = format!("comments_{}.json", aweme_id);
        let path = self.base_dir.join(&filename);

        debug!("Loading comments fixture from: {}", path.display());

        let content = std::fs::read_to_string(&path)
            .map_err(|_| Error::FixtureNotFound(path.display().to_string()))?;

        let fixture: TikTokCommentsFixture = serde_json::from_str(&content)?;

        Ok(fixture)
    }

    /// Load a user videos fixture
    ///
    /// # Arguments
    /// * `unique_id` - The username (e.g., "tiktok")
    pub fn load_user_videos_fixture(&self, unique_id: &str) -> Result<TikTokUserVideosFixture> {
        let filename = format!("user_videos_{}.json", unique_id);
        let path = self.base_dir.join(&filename);

        debug!("Loading user videos fixture from: {}", path.display());

        let content = std::fs::read_to_string(&path)
            .map_err(|_| Error::FixtureNotFound(path.display().to_string()))?;

        let fixture: TikTokUserVideosFixture = serde_json::from_str(&content)?;

        Ok(fixture)
    }

    /// List all available fixture files
    pub fn list_fixtures(&self) -> Vec<String> {
        std::fs::read_dir(&self.base_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|name| name.ends_with(".json"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if a fixture exists
    pub fn fixture_exists(&self, filename: &str) -> bool {
        self.base_dir.join(filename).exists()
    }

    /// Get full path to a fixture file
    pub fn fixture_path(&self, filename: &str) -> PathBuf {
        self.base_dir.join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_filename() {
        assert_eq!(FixtureLoader::safe_filename("hello world"), "hello_world");
        assert_eq!(FixtureLoader::safe_filename("test/path"), "test_path");
        assert_eq!(
            FixtureLoader::safe_filename("normal-text_123"),
            "normal-text_123"
        );
    }
}
