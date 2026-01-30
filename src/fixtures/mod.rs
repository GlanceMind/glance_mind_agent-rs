//! Fixture Generation and Loading
//!
//! Tools for generating test fixtures from real TikHub API responses
//! and loading them for testing.

mod generator;
mod loader;

pub use generator::FixtureGenerator;
pub use loader::FixtureLoader;

use serde::{Deserialize, Serialize};
use crate::tikhub::{AwemeInfo, TikTokComment};

// ============================================================
// Fixture Data Structures
// ============================================================

/// TikTok video search fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokSearchFixture {
    /// Search keyword used
    pub keyword: String,
    
    /// Region code
    pub region: String,
    
    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,
    
    /// Total videos in this fixture
    pub total_count: usize,
    
    /// Raw API response (for reference)
    pub raw_response: serde_json::Value,
    
    /// Extracted video list
    pub videos: Vec<serde_json::Value>,
}

/// TikTok comments fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokCommentsFixture {
    /// Video ID
    pub aweme_id: String,
    
    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,
    
    /// Total comments in this fixture
    pub total_count: usize,
    
    /// Comment list (raw JSON for flexibility)
    pub comments: Vec<serde_json::Value>,
}

/// TikTok user videos fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TikTokUserVideosFixture {
    /// Username (unique_id)
    pub unique_id: String,
    
    /// When this fixture was generated (ISO 8601)
    pub generated_at: String,
    
    /// Total videos in this fixture
    pub total_count: usize,
    
    /// Raw API response
    pub raw_response: serde_json::Value,
    
    /// Video list
    pub videos: Vec<serde_json::Value>,
}

impl TikTokSearchFixture {
    /// Parse videos into typed AwemeInfo structs
    pub fn parse_videos(&self) -> Vec<AwemeInfo> {
        self.videos
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect()
    }
}

impl TikTokCommentsFixture {
    /// Parse comments into typed TikTokComment structs
    pub fn parse_comments(&self) -> Vec<TikTokComment> {
        self.comments
            .iter()
            .filter_map(|c| serde_json::from_value(c.clone()).ok())
            .collect()
    }
}

impl TikTokUserVideosFixture {
    /// Parse videos into typed AwemeInfo structs
    pub fn parse_videos(&self) -> Vec<AwemeInfo> {
        self.videos
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect()
    }
}
