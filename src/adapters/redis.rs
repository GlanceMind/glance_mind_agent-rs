//! Redis Adapter - Task queue consumer
//!
//! This adapter consumes tasks from a Redis queue and provides
//! them to the worker for processing.
//!
//! Task structures are defined in the protocol module (from glance_mind_protocol).
//! Platform mappings are loaded from the global registry (initialized at startup).

use redis::{aio::ConnectionManager, AsyncCommands, Client};
use serde::{de, Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::config::platform::{global_registry, PlatformLookup};
use crate::domain::errors::{QueueError, QueueResult};
use crate::protocol_gen::{
    CrawlerTask, CrawlerTaskMeta, CrawlerTaskSpec, Platform, TaskConfig as ProtocolTaskConfig,
    TaskFilters,
};

// ============================================================
// Search Options Parsing (from campaign.search_options JSON)
// ============================================================

/// TikTok-specific search options from campaign configuration
/// Example JSON: {"tiktok":{"region":"GLOBAL","sort_type":"0","publish_time":"0"}}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TikTokSearchOptions {
    /// Region code (GLOBAL means default US)
    #[serde(default)]
    pub region: Option<String>,

    /// Sort type: "0" = relevance, "1" = most_liked
    #[serde(default)]
    pub sort_type: Option<String>,

    /// Publish time filter: "0"=all, "1"=day, "7"=week, "30"=month, "90"=3months, "180"=6months
    #[serde(default)]
    pub publish_time: Option<String>,
}

impl TikTokSearchOptions {
    /// Parse sort_type string to u8
    pub fn sort_type_u8(&self) -> Option<u8> {
        self.sort_type.as_ref().and_then(|s| s.parse().ok())
    }

    /// Parse publish_time string to u8
    pub fn publish_time_u8(&self) -> Option<u8> {
        self.publish_time.as_ref().and_then(|s| s.parse().ok())
    }

    /// Get effective region (GLOBAL maps to US)
    pub fn effective_region(&self) -> Option<String> {
        self.region.as_ref().map(|r| {
            if r.eq_ignore_ascii_case("GLOBAL") {
                "US".to_string()
            } else {
                r.clone()
            }
        })
    }
}

fn deserialize_optional_boolish<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(serde_json::Value::Bool(value)) => Ok(Some(value)),
        Some(serde_json::Value::String(value)) => {
            match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(Some(true)),
                "false" | "0" | "no" => Ok(Some(false)),
                other => Err(de::Error::custom(format!(
                    "invalid boolean string for recent_posts: {other}"
                ))),
            }
        }
        Some(other) => Err(de::Error::custom(format!(
            "invalid boolean value for recent_posts: {other}"
        ))),
    }
}

/// Facebook-specific search options from campaign configuration
/// Example JSON: {"facebook":{"search_type":"posts","recent_posts":"true","location":"beijing,china"}}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FacebookSearchOptions {
    #[serde(default)]
    pub search_type: Option<String>,

    #[serde(default, deserialize_with = "deserialize_optional_boolish")]
    pub recent_posts: Option<bool>,

    #[serde(default)]
    pub location: Option<String>,

    #[serde(default)]
    pub start_date: Option<String>,

    #[serde(default)]
    pub end_date: Option<String>,
}

/// Twitter-specific search options from campaign configuration.
/// Example JSON: {"twitter":{"search_type":"Top"}}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TwitterSearchOptions {
    #[serde(default)]
    pub search_type: Option<String>,
}

/// Platform-keyed search options wrapper
/// Example: {"tiktok": {...}, "instagram": {...}}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchOptionsWrapper {
    #[serde(default)]
    pub tiktok: Option<TikTokSearchOptions>,
    #[serde(default)]
    pub facebook: Option<FacebookSearchOptions>,
    #[serde(default)]
    pub twitter: Option<TwitterSearchOptions>,
}

/// Parse search_options JSON string into SearchOptionsWrapper
fn parse_search_options(search_options: Option<&str>) -> SearchOptionsWrapper {
    match search_options {
        Some(s) if !s.is_empty() => {
            info!("Parsing search_options: {}", s);
            match serde_json::from_str::<SearchOptionsWrapper>(s) {
                Ok(opts) => {
                    if let Some(ref tiktok) = opts.tiktok {
                        info!(
                            "Parsed TikTok search_options: region={:?}, sort_type={:?}, publish_time={:?}",
                            tiktok.region, tiktok.sort_type, tiktok.publish_time
                        );
                    }
                    if let Some(ref facebook) = opts.facebook {
                        info!(
                            "Parsed Facebook search_options: search_type={:?}, recent_posts={:?}, location={:?}, start_date={:?}, end_date={:?}",
                            facebook.search_type,
                            facebook.recent_posts,
                            facebook.location,
                            facebook.start_date,
                            facebook.end_date
                        );
                    }
                    if let Some(ref twitter) = opts.twitter {
                        info!(
                            "Parsed Twitter search_options: search_type={:?}",
                            twitter.search_type
                        );
                    }
                    if opts.tiktok.is_none() && opts.facebook.is_none() && opts.twitter.is_none() {
                        info!("search_options parsed but no supported platform field found");
                    }
                    opts
                }
                Err(e) => {
                    warn!(
                        "Failed to parse search_options JSON: {} - input: {:?}",
                        e, s
                    );
                    SearchOptionsWrapper::default()
                }
            }
        }
        _ => {
            info!(
                "No search_options provided (value={:?}), using defaults",
                search_options
            );
            SearchOptionsWrapper::default()
        }
    }
}

/// Redis task consumer for the agent
///
/// Uses ConnectionManager for automatic reconnection on connection failures.
pub struct RedisTaskConsumer {
    client: Client,
    conn_manager: Option<ConnectionManager>,
    queue_name: String,
    result_queue: String,
    timeout_secs: u64,
}

impl RedisTaskConsumer {
    /// Create a new Redis task consumer
    pub fn new(redis_url: &str, queue_name: &str) -> Result<Self, QueueError> {
        let client = Client::open(redis_url).map_err(|e| QueueError::Connection(e.to_string()))?;

        Ok(Self {
            client,
            conn_manager: None,
            queue_name: queue_name.to_string(),
            result_queue: format!("{}_results", queue_name),
            timeout_secs: 30,
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, QueueError> {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        // Default queue name must match scheduler's queue: crawler:task_queue
        let queue_name =
            std::env::var("AGENT_QUEUE_NAME").unwrap_or_else(|_| "crawler:task_queue".to_string());

        Self::new(&redis_url, &queue_name)
    }

    /// Set the blocking timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Initialize the connection manager (call once before using)
    ///
    /// ConnectionManager provides automatic reconnection on failures.
    pub async fn init(&mut self) -> QueueResult<()> {
        info!("Initializing Redis connection manager...");
        let mut manager = ConnectionManager::new(self.client.clone())
            .await
            .map_err(|e| {
                QueueError::Connection(format!("Failed to create connection manager: {}", e))
            })?;

        // Verify the connection with a PING command
        let pong: String = redis::cmd("PING")
            .query_async(&mut manager)
            .await
            .map_err(|e| QueueError::Connection(format!("Redis PING failed: {}", e)))?;

        if pong != "PONG" {
            return Err(QueueError::Connection(format!(
                "Unexpected PING response: {}",
                pong
            )));
        }

        self.conn_manager = Some(manager);
        info!("Redis connection manager initialized and verified (PING OK)");
        Ok(())
    }

    /// Get a connection (uses ConnectionManager with auto-reconnect)
    async fn get_conn(&self) -> QueueResult<ConnectionManager> {
        self.conn_manager.clone().ok_or_else(|| {
            QueueError::Connection(
                "Connection manager not initialized. Call init() first.".to_string(),
            )
        })
    }

    /// Consume a task from the queue (blocking)
    ///
    /// This will block for up to `timeout_secs` waiting for a task.
    /// Returns `QueueError::Empty` if no task is available within the timeout.
    pub async fn consume(&self) -> QueueResult<CrawlerTask> {
        let mut conn = self.get_conn().await?;

        // Use BRPOP for blocking pop from the queue
        let result: Option<(String, String)> = redis::cmd("BRPOP")
            .arg(&self.queue_name)
            .arg(self.timeout_secs)
            .query_async(&mut conn)
            .await
            .map_err(|e| QueueError::Connection(e.to_string()))?;

        match result {
            Some((_queue, data)) => {
                let task: CrawlerTask = serde_json::from_str(&data)
                    .map_err(|e| QueueError::Deserialization(e.to_string()))?;

                let task_id = task.meta.as_ref().map(|m| m.task_id).unwrap_or(0);
                debug!(task_id, "Consumed task from queue");
                Ok(task)
            }
            None => Err(QueueError::Empty),
        }
    }

    /// Try to consume a task without blocking
    pub async fn try_consume(&self) -> QueueResult<Option<CrawlerTask>> {
        let mut conn = self.get_conn().await?;

        let data: Option<String> = conn
            .rpop(&self.queue_name, None)
            .await
            .map_err(|e| QueueError::Connection(e.to_string()))?;

        match data {
            Some(data) => {
                let task: CrawlerTask = serde_json::from_str(&data)
                    .map_err(|e| QueueError::Deserialization(e.to_string()))?;
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }

    /// Acknowledge task completion
    pub async fn ack(&self, task_id: i64, success: bool, message: Option<&str>) -> QueueResult<()> {
        let mut conn = self.get_conn().await?;

        let result = TaskResult {
            task_id,
            success,
            message: message.map(|s| s.to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let data =
            serde_json::to_string(&result).map_err(|e| QueueError::Serialization(e.to_string()))?;

        // Push result to result queue
        conn.lpush::<_, _, ()>(&self.result_queue, &data)
            .await
            .map_err(|e| QueueError::AckFailed(e.to_string()))?;

        debug!(task_id, success, "Task acknowledged");
        Ok(())
    }

    /// Get queue length
    pub async fn queue_length(&self) -> QueueResult<usize> {
        let mut conn = self.get_conn().await?;

        let len: usize = conn
            .llen(&self.queue_name)
            .await
            .map_err(|e| QueueError::Connection(e.to_string()))?;

        Ok(len)
    }

    /// Check if queue is healthy (can connect)
    pub async fn health_check(&self) -> bool {
        match self.get_conn().await {
            Ok(mut conn) => {
                let result: Result<String, _> = redis::cmd("PING").query_async(&mut conn).await;
                result.is_ok()
            }
            Err(_) => false,
        }
    }
}

// ============================================================
// Task Result (for acknowledgment)
// ============================================================

/// Task result to push back to Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: i64,
    pub success: bool,
    pub message: Option<String>,
    pub timestamp: String,
}

// ============================================================
// Protocol Type Extensions
// ============================================================

/// Extension trait for CrawlerTask
pub trait CrawlerTaskExt {
    /// Get task ID
    fn task_id(&self) -> i64;

    /// Get campaign ID
    fn campaign_id(&self) -> i32;

    /// Get platform ID
    fn platform_id(&self) -> i32;

    /// Get platform name as String (looked up from global registry)
    fn platform_name(&self) -> String;

    /// Get keywords
    fn keywords(&self) -> Vec<String>;

    /// Get region
    fn region(&self) -> Option<String>;

    /// Get max count
    fn max_count(&self) -> i32;

    /// Get raw search_options JSON string
    fn search_options(&self) -> Option<&str>;

    /// Convert to domain TaskConfig
    fn to_domain_task_config(&self, campaign_id: i32) -> crate::domain::TaskConfig;
}

impl CrawlerTaskExt for CrawlerTask {
    fn task_id(&self) -> i64 {
        self.meta.as_ref().map(|m| m.task_id).unwrap_or(0)
    }

    fn campaign_id(&self) -> i32 {
        self.meta.as_ref().map(|m| m.campaign_id).unwrap_or(0)
    }

    fn platform_id(&self) -> i32 {
        self.spec.as_ref().map(|s| s.platform).unwrap_or(0)
    }

    fn platform_name(&self) -> String {
        // Use global platform registry (loaded from database at startup)
        let registry = global_registry();
        registry
            .get_name(self.platform_id())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn keywords(&self) -> Vec<String> {
        self.config
            .as_ref()
            .map(|c| c.keywords.clone())
            .unwrap_or_default()
    }

    fn region(&self) -> Option<String> {
        self.config
            .as_ref()
            .and_then(|c| c.filters.as_ref())
            .and_then(|f| f.region.clone())
    }

    fn max_count(&self) -> i32 {
        self.config.as_ref().map(|c| c.max_count).unwrap_or(10)
    }

    /// Get raw search_options JSON string
    fn search_options(&self) -> Option<&str> {
        self.config
            .as_ref()
            .and_then(|c| c.search_options.as_deref())
    }

    fn to_domain_task_config(&self, campaign_id: i32) -> crate::domain::TaskConfig {
        let max_count = self.max_count();
        let platform_name = self.platform_name();

        info!(
            "Building TaskConfig: campaign_id={}, platform={}, max_count={}, raw_search_options={:?}",
            campaign_id, platform_name, max_count, self.search_options()
        );

        // Parse search_options JSON to extract platform-specific parameters
        let search_opts = parse_search_options(self.search_options());

        // Build base config
        let mut config = crate::domain::TaskConfig::new(campaign_id, &platform_name)
            .with_keywords(self.keywords())
            .with_max_videos(max_count)
            .with_max_comments_per_video(200);

        // Apply platform-specific search options
        if platform_name.eq_ignore_ascii_case("tiktok") {
            if let Some(tiktok_opts) = search_opts.tiktok {
                // Region: priority -> search_options > filters > default "US"
                let region = tiktok_opts
                    .effective_region()
                    .or_else(|| self.region())
                    .unwrap_or_else(|| "US".to_string());
                config = config.with_region(region);

                // Sort type from search_options
                if let Some(sort_type) = tiktok_opts.sort_type_u8() {
                    config = config.with_sort_type(sort_type);
                }

                // Publish time from search_options
                if let Some(publish_time) = tiktok_opts.publish_time_u8() {
                    config = config.with_publish_time(publish_time);
                }

                info!(
                    "Applied TikTok search_options: region={:?}, sort_type={:?}, publish_time={:?}",
                    config.region, config.sort_type, config.publish_time
                );
            } else {
                // No tiktok-specific options, use filters.region as fallback
                let region = self.region().unwrap_or_else(|| "US".to_string());
                config = config.with_region(region.clone());
                info!("No TikTok search_options, using filters.region={}", region);
            }
        } else {
            // Non-TikTok platforms: use filters.region as fallback
            let region = self.region().unwrap_or_else(|| "US".to_string());
            config = config.with_region(region.clone());
            debug!("Non-TikTok platform, using filters.region={}", region);

            if platform_name.eq_ignore_ascii_case("facebook") {
                if let Some(facebook_opts) = search_opts.facebook {
                    if let Some(search_type) = facebook_opts.search_type {
                        config.extra.insert(
                            "search_type".to_string(),
                            serde_json::Value::String(search_type),
                        );
                    }
                    if let Some(recent_posts) = facebook_opts.recent_posts {
                        config.extra.insert(
                            "recent_posts".to_string(),
                            serde_json::Value::Bool(recent_posts),
                        );
                    }
                    if let Some(location) = facebook_opts.location {
                        config
                            .extra
                            .insert("location".to_string(), serde_json::Value::String(location));
                    }
                    if let Some(start_date) = facebook_opts.start_date {
                        config.extra.insert(
                            "start_date".to_string(),
                            serde_json::Value::String(start_date),
                        );
                    }
                    if let Some(end_date) = facebook_opts.end_date {
                        config
                            .extra
                            .insert("end_date".to_string(), serde_json::Value::String(end_date));
                    }

                    debug!(extra = ?config.extra, "Applied Facebook search_options");
                }
            } else if platform_name.eq_ignore_ascii_case("twitter") {
                if let Some(twitter_opts) = search_opts.twitter {
                    if let Some(search_type) = twitter_opts.search_type {
                        config.extra.insert(
                            "search_type".to_string(),
                            serde_json::Value::String(search_type),
                        );
                    }

                    debug!(extra = ?config.extra, "Applied Twitter search_options");
                }
            }
        }

        config
    }
}

/// Builder for creating CrawlerTask
pub struct CrawlerTaskBuilder {
    task_id: i64,
    campaign_id: i32,
    platform: i32,
    data_type: i32,
    keywords: Vec<String>,
    region: Option<String>,
    max_count: i32,
}

impl CrawlerTaskBuilder {
    /// Create a new builder
    pub fn new(task_id: i64) -> Self {
        Self {
            task_id,
            campaign_id: 0,
            platform: Platform::Tiktok as i32,
            data_type: 0,
            keywords: Vec::new(),
            region: None,
            max_count: 10,
        }
    }

    /// Set campaign ID
    pub fn campaign_id(mut self, id: i32) -> Self {
        self.campaign_id = id;
        self
    }

    /// Set platform
    pub fn platform(mut self, platform: Platform) -> Self {
        self.platform = platform as i32;
        self
    }

    /// Set platform by ID
    pub fn platform_id(mut self, id: i32) -> Self {
        self.platform = id;
        self
    }

    /// Set keywords
    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Set region
    pub fn region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Set max count
    pub fn max_count(mut self, count: i32) -> Self {
        self.max_count = count;
        self
    }

    /// Build the CrawlerTask
    pub fn build(self) -> CrawlerTask {
        CrawlerTask {
            meta: Some(CrawlerTaskMeta {
                task_id: self.task_id,
                campaign_id: self.campaign_id,
                source: "agent".to_string(),
                timestamp: chrono::Utc::now().timestamp() as f64,
            }),
            spec: Some(CrawlerTaskSpec {
                platform: self.platform,
                data_type: self.data_type,
            }),
            config: Some(ProtocolTaskConfig {
                keywords: self.keywords,
                max_count: self.max_count,
                search_offset: 0,
                search_limit: self.max_count,
                filters: self.region.map(|r| TaskFilters {
                    time_range: None,
                    region: Some(r),
                }),
                search_options: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::platform::init_global_registry;

    fn init_test_registry() {
        // Initialize global registry with defaults for testing
        init_global_registry(crate::config::PlatformRegistry::with_defaults());
    }

    #[test]
    fn test_crawler_task_ext_methods() {
        init_test_registry();

        let task = CrawlerTaskBuilder::new(123)
            .platform(Platform::Tiktok)
            .keywords(vec!["fitness".to_string(), "workout".to_string()])
            .region("GB")
            .max_count(15)
            .build();

        assert_eq!(task.task_id(), 123);
        assert_eq!(task.platform_id(), 2);
        assert_eq!(task.platform_name(), "tiktok");
        assert_eq!(task.keywords().len(), 2);
        assert_eq!(task.region(), Some("GB".to_string()));
        assert_eq!(task.max_count(), 15);
    }

    #[test]
    fn test_crawler_task_to_domain_config() {
        init_test_registry();

        let task = CrawlerTaskBuilder::new(1)
            .platform(Platform::Tiktok)
            .keywords(vec!["fitness".to_string()])
            .region("US")
            .max_count(20)
            .build();

        let config = task.to_domain_task_config(100);
        assert_eq!(config.platform, "tiktok");
        assert_eq!(config.campaign_id, 100);
        assert_eq!(config.keywords.len(), 1);
        assert_eq!(config.region, Some("US".to_string()));
        assert_eq!(config.max_videos, Some(20));
    }

    #[test]
    fn test_crawler_task_deserialization() {
        init_test_registry();

        // Protocol format JSON
        let json = r#"{
            "meta": {
                "task_id": 123,
                "source": "scheduler",
                "timestamp": 1700000000.0
            },
            "spec": {
                "platform": 2,
                "data_type": 1
            },
            "config": {
                "keywords": ["test", "demo"],
                "max_count": 10,
                "search_offset": 0,
                "search_limit": 10,
                "filters": {
                    "region": "US"
                }
            }
        }"#;

        let task: CrawlerTask = serde_json::from_str(json).unwrap();
        assert_eq!(task.task_id(), 123);
        assert_eq!(task.platform_id(), 2);
        assert_eq!(task.platform_name(), "tiktok");
        assert_eq!(task.keywords(), vec!["test", "demo"]);
        assert_eq!(task.region(), Some("US".to_string()));
    }

    #[test]
    fn test_crawler_task_builder() {
        let task = CrawlerTaskBuilder::new(456)
            .platform(Platform::Instagram)
            .keywords(vec!["fashion".to_string()])
            .region("UK")
            .max_count(25)
            .build();

        assert!(task.meta.is_some());
        assert!(task.spec.is_some());
        assert!(task.config.is_some());

        assert_eq!(task.task_id(), 456);
        assert_eq!(task.platform_id(), Platform::Instagram as i32);
    }

    #[test]
    fn test_task_result_serialization() {
        let result = TaskResult {
            task_id: 123,
            success: true,
            message: Some("Completed".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("123"));
        assert!(json.contains("true"));
        assert!(json.contains("Completed"));
    }

    #[test]
    fn redis_ack_result_message_carries_terminal_reason() {
        let result = TaskResult {
            task_id: 123,
            success: true,
            message: Some("COMPLETED: Task completed successfully".to_string()),
            timestamp: "2026-05-13T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"message\":\"COMPLETED: Task completed successfully\""));
        assert!(!json.contains("terminal_reason"));
    }

    #[test]
    fn test_parse_search_options_tiktok() {
        // Test parsing TikTok search options from campaign configuration
        let json = r#"{"tiktok":{"region":"GLOBAL","sort_type":"1","publish_time":"7"}}"#;

        let opts = super::parse_search_options(Some(json));

        assert!(opts.tiktok.is_some());
        let tiktok = opts.tiktok.unwrap();

        // GLOBAL should map to US
        assert_eq!(tiktok.effective_region(), Some("US".to_string()));
        assert_eq!(tiktok.sort_type_u8(), Some(1)); // most_liked
        assert_eq!(tiktok.publish_time_u8(), Some(7)); // last week
    }

    #[test]
    fn test_parse_search_options_with_region() {
        let json = r#"{"tiktok":{"region":"JP","sort_type":"0","publish_time":"30"}}"#;

        let opts = super::parse_search_options(Some(json));
        let tiktok = opts.tiktok.unwrap();

        // Non-GLOBAL region should be preserved
        assert_eq!(tiktok.effective_region(), Some("JP".to_string()));
        assert_eq!(tiktok.sort_type_u8(), Some(0)); // relevance
        assert_eq!(tiktok.publish_time_u8(), Some(30)); // last month
    }

    #[test]
    fn test_parse_search_options_empty() {
        let opts = super::parse_search_options(None);
        assert!(opts.tiktok.is_none());
        assert!(opts.facebook.is_none());
        assert!(opts.twitter.is_none());

        let opts = super::parse_search_options(Some("{}"));
        assert!(opts.tiktok.is_none());
        assert!(opts.facebook.is_none());
        assert!(opts.twitter.is_none());
    }

    #[test]
    fn test_parse_search_options_invalid_json() {
        // Invalid JSON should return default
        let opts = super::parse_search_options(Some("not json"));
        assert!(opts.tiktok.is_none());
        assert!(opts.facebook.is_none());
        assert!(opts.twitter.is_none());
    }

    #[test]
    fn test_parse_search_options_facebook() {
        let json = r#"{"facebook":{"search_type":"places","recent_posts":"true","location":"beijing,china","start_date":"2026-01-01","end_date":"2026-01-31"}}"#;

        let opts = super::parse_search_options(Some(json));
        let facebook = opts.facebook.expect("facebook search options should exist");

        assert_eq!(facebook.search_type.as_deref(), Some("places"));
        assert_eq!(facebook.recent_posts, Some(true));
        assert_eq!(facebook.location.as_deref(), Some("beijing,china"));
        assert_eq!(facebook.start_date.as_deref(), Some("2026-01-01"));
        assert_eq!(facebook.end_date.as_deref(), Some("2026-01-31"));
    }

    #[test]
    fn test_crawler_task_with_search_options() {
        init_test_registry();

        // Task with search_options
        let json = r#"{
            "meta": {
                "task_id": 789,
                "campaign_id": 16,
                "source": "scheduler",
                "timestamp": 1700000000.0
            },
            "spec": {
                "platform": 2,
                "data_type": 1
            },
            "config": {
                "keywords": ["ai video"],
                "max_count": 10,
                "search_offset": 0,
                "search_limit": 10,
                "filters": {
                    "region": "US"
                },
                "search_options": "{\"tiktok\":{\"region\":\"GLOBAL\",\"sort_type\":\"1\",\"publish_time\":\"7\"}}"
            }
        }"#;

        let task: CrawlerTask = serde_json::from_str(json).unwrap();
        let config = task.to_domain_task_config(16);

        // Verify search options are parsed and applied
        assert_eq!(config.campaign_id, 16);
        assert_eq!(config.platform, "tiktok");
        assert_eq!(config.region, Some("US".to_string())); // GLOBAL -> US
        assert_eq!(config.sort_type, Some(1)); // most_liked
        assert_eq!(config.publish_time, Some(7)); // last week
        assert_eq!(config.max_videos, Some(10));
    }

    #[test]
    fn test_facebook_crawler_task_with_search_options() {
        init_test_registry();

        let json = r#"{
            "meta": {
                "task_id": 790,
                "campaign_id": 17,
                "source": "scheduler",
                "timestamp": 1700000000.0
            },
            "spec": {
                "platform": 3,
                "data_type": 1
            },
            "config": {
                "keywords": ["facebook_page:NatGeoMuseum"],
                "max_count": 5,
                "search_offset": 0,
                "search_limit": 10,
                "filters": {
                    "region": "GLOBAL"
                },
                "search_options": "{\"facebook\":{\"search_type\":\"pages\",\"recent_posts\":\"true\",\"location\":\"washington,usa\",\"start_date\":\"2026-01-01\",\"end_date\":\"2026-01-31\"}}"
            }
        }"#;

        let task: CrawlerTask = serde_json::from_str(json).unwrap();
        let config = task.to_domain_task_config(17);

        assert_eq!(config.campaign_id, 17);
        assert_eq!(config.platform, "facebook");
        assert_eq!(config.region, Some("GLOBAL".to_string()));
        assert_eq!(
            config.extra.get("search_type").and_then(|v| v.as_str()),
            Some("pages")
        );
        assert_eq!(
            config.extra.get("recent_posts").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            config.extra.get("location").and_then(|v| v.as_str()),
            Some("washington,usa")
        );
        assert_eq!(
            config.extra.get("start_date").and_then(|v| v.as_str()),
            Some("2026-01-01")
        );
        assert_eq!(
            config.extra.get("end_date").and_then(|v| v.as_str()),
            Some("2026-01-31")
        );
    }

    #[test]
    fn test_parse_search_options_twitter() {
        let json = r#"{"twitter":{"search_type":"Top"}}"#;

        let opts = super::parse_search_options(Some(json));
        let twitter = opts.twitter.expect("twitter search options should exist");

        assert_eq!(twitter.search_type.as_deref(), Some("Top"));
    }

    #[test]
    fn test_twitter_crawler_task_with_search_options() {
        init_test_registry();

        let json = r##"{
            "meta": {
                "task_id": 791,
                "campaign_id": 18,
                "source": "scheduler",
                "timestamp": 1700000000.0
            },
            "spec": {
                "platform": 5,
                "data_type": 1
            },
            "config": {
                "keywords": ["#rustlang"],
                "max_count": 5,
                "search_offset": 0,
                "search_limit": 10,
                "filters": {
                    "region": "GLOBAL"
                },
                "search_options": "{\"twitter\":{\"search_type\":\"Top\"}}"
            }
        }"##;

        let task: CrawlerTask = serde_json::from_str(json).unwrap();
        let config = task.to_domain_task_config(18);

        assert_eq!(config.campaign_id, 18);
        assert_eq!(config.platform, "twitter");
        assert_eq!(config.region, Some("GLOBAL".to_string()));
        assert_eq!(
            config
                .extra
                .get("search_type")
                .and_then(|value| value.as_str()),
            Some("Top")
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // M1-T5 redis 字段语义(T-003 / T-034 / I-008 / C-001 / C-002;
    // m1-pagination-core.md §2 D4 + §3 M1-T5)
    // ──────────────────────────────────────────────────────────────────

    use proptest::prelude::*;

    /// 构造仅 platform / search_limit / search_offset 可变的 task JSON
    /// (沿既有 task JSON 样板形状,redis.rs:709)。
    fn task_with_search_fields(
        platform_id: i32,
        search_limit: i32,
        search_offset: i32,
    ) -> CrawlerTask {
        let json = serde_json::json!({
            "meta": {
                "task_id": 900,
                "campaign_id": 42,
                "source": "scheduler",
                "timestamp": 1700000000.0
            },
            "spec": {
                "platform": platform_id,
                "data_type": 1
            },
            "config": {
                "keywords": ["pagination"],
                "max_count": 10,
                "search_offset": search_offset,
                "search_limit": search_limit,
                "filters": {
                    "region": "US"
                }
            }
        });
        serde_json::from_value(json).expect("task JSON 样板应可反序列化")
    }

    /// 测试 1(T-003a):tiktok task JSON `search_limit=7`
    /// → `to_domain_task_config(...).page_size_hint == Some(7)`。
    #[test]
    fn search_limit_becomes_page_size_hint() {
        init_test_registry();

        let task = task_with_search_fields(2, 7, 0); // platform 2 = tiktok
        let config = task.to_domain_task_config(42);
        assert_eq!(config.page_size_hint, Some(7));
    }

    /// 测试 2(T-003b):`search_limit=500` → clamp 到各平台页上限
    /// (D4 取值表,5 平台逐一断言)。
    #[test]
    fn search_limit_clamped_to_platform_cap() {
        init_test_registry();

        // (platform_id, platform_name, D4 cap)
        let table: &[(i32, &str, u32)] = &[
            (2, "tiktok", 20),
            (3, "facebook", 20),
            (1, "reddit", 100),
            (5, "twitter", 100),
            (4, "instagram", 50),
        ];
        for (platform_id, platform_name, cap) in table {
            let task = task_with_search_fields(*platform_id, 500, 0);
            let config = task.to_domain_task_config(42);
            assert_eq!(
                config.platform, *platform_name,
                "registry 应解析 platform {platform_id} 为 {platform_name}"
            );
            assert_eq!(
                config.page_size_hint,
                Some(*cap),
                "{platform_name}: search_limit=500 应 clamp 到 Some({cap})(D4 冻结表)"
            );
        }
    }

    /// 测试 3(T-003c):`search_limit <= 0`(0 与 -3)→ `page_size_hint == None`
    /// (老 task / 缺省语义,C-001 向后兼容:平台默认页大小)。
    #[test]
    fn non_positive_search_limit_falls_back_to_none() {
        init_test_registry();

        for limit in [0, -3] {
            let task = task_with_search_fields(2, limit, 0);
            let config = task.to_domain_task_config(42);
            assert_eq!(
                config.page_size_hint, None,
                "search_limit={limit} 应映射为 None(平台默认页大小)"
            );
        }
    }

    proptest! {
        /// 测试 4(T-034 / PT-5 / I-008):任意 i32 search_offset,同一 task JSON
        /// 仅 search_offset 不同 → 两次 `to_domain_task_config` 的
        /// `serde_json::to_value(..)` 全等(行为等价;TaskConfig 无 PartialEq,
        /// 以 JSON 全等代理)。`search_offset` 任何路径不读。
        ///
        /// **允许先绿**(现状已忽略 offset;本测试是 I-008 的回归钉子,AG-006)。
        /// 变异豁免预案(计划 §6.3 / M1-T5.4 原文):AG-012 预检中映射函数被注入
        /// 「读 search_offset」类变异时本测试须变红;若 cargo-mutants 不生成该类变异,
        /// 以「测试 3+4 联合钉死字段语义」为书面豁免记录(Test-Gate Reviewer 复核)。
        #[test]
        fn prop_search_offset_never_changes_task_config(offset in any::<i32>()) {
            init_test_registry();

            let baseline = task_with_search_fields(2, 7, 0)
                .to_domain_task_config(42);
            let with_offset = task_with_search_fields(2, 7, offset)
                .to_domain_task_config(42);

            prop_assert_eq!(
                serde_json::to_value(&baseline).unwrap(),
                serde_json::to_value(&with_offset).unwrap()
            );
        }
    }
}
