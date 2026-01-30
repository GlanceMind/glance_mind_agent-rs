//! Redis Adapter - Task queue consumer
//!
//! This adapter consumes tasks from a Redis queue and provides
//! them to the worker for processing.
//!
//! Task structures are defined in the protocol module (from glance_mind_protocol).
//! Platform mappings are loaded from the global registry (initialized at startup).

use redis::{AsyncCommands, Client, aio::ConnectionManager};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::config::platform::{global_registry, PlatformLookup};
use crate::domain::errors::{QueueError, QueueResult};
use crate::protocol_gen::{
    CrawlerTask, CrawlerTaskMeta, CrawlerTaskSpec, 
    TaskConfig as ProtocolTaskConfig, TaskFilters, Platform,
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

/// Platform-keyed search options wrapper
/// Example: {"tiktok": {...}, "instagram": {...}}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchOptionsWrapper {
    #[serde(default)]
    pub tiktok: Option<TikTokSearchOptions>,
    // Future: add other platforms like instagram, facebook, etc.
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
                    } else {
                        info!("search_options parsed but no 'tiktok' field found");
                    }
                    opts
                }
                Err(e) => {
                    warn!("Failed to parse search_options JSON: {} - input: {:?}", e, s);
                    SearchOptionsWrapper::default()
                }
            }
        }
        _ => {
            info!("No search_options provided (value={:?}), using defaults", search_options);
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
        let client = Client::open(redis_url)
            .map_err(|e| QueueError::Connection(e.to_string()))?;
        
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
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        
        // Default queue name must match scheduler's queue: crawler:task_queue
        let queue_name = std::env::var("AGENT_QUEUE_NAME")
            .unwrap_or_else(|_| "crawler:task_queue".to_string());
        
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
            .map_err(|e| QueueError::Connection(format!("Failed to create connection manager: {}", e)))?;
        
        // Verify the connection with a PING command
        let pong: String = redis::cmd("PING")
            .query_async(&mut manager)
            .await
            .map_err(|e| QueueError::Connection(format!("Redis PING failed: {}", e)))?;
        
        if pong != "PONG" {
            return Err(QueueError::Connection(format!("Unexpected PING response: {}", pong)));
        }
        
        self.conn_manager = Some(manager);
        info!("Redis connection manager initialized and verified (PING OK)");
        Ok(())
    }

    /// Get a connection (uses ConnectionManager with auto-reconnect)
    async fn get_conn(&self) -> QueueResult<ConnectionManager> {
        self.conn_manager
            .clone()
            .ok_or_else(|| QueueError::Connection("Connection manager not initialized. Call init() first.".to_string()))
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

        let data: Option<String> = conn.rpop(&self.queue_name, None)
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

        let data = serde_json::to_string(&result)
            .map_err(|e| QueueError::Serialization(e.to_string()))?;

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
        
        let len: usize = conn.llen(&self.queue_name)
            .await
            .map_err(|e| QueueError::Connection(e.to_string()))?;
        
        Ok(len)
    }

    /// Check if queue is healthy (can connect)
    pub async fn health_check(&self) -> bool {
        match self.get_conn().await {
            Ok(mut conn) => {
                let result: Result<String, _> = redis::cmd("PING")
                    .query_async(&mut conn)
                    .await;
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
        self.config.as_ref()
            .map(|c| c.keywords.clone())
            .unwrap_or_default()
    }

    fn region(&self) -> Option<String> {
        self.config.as_ref()
            .and_then(|c| c.filters.as_ref())
            .and_then(|f| f.region.clone())
    }

    fn max_count(&self) -> i32 {
        self.config.as_ref()
            .map(|c| c.max_count)
            .unwrap_or(10)
    }
    
    /// Get raw search_options JSON string
    fn search_options(&self) -> Option<&str> {
        self.config.as_ref()
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
                let region = tiktok_opts.effective_region()
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
        
        let opts = super::parse_search_options(Some("{}"));
        assert!(opts.tiktok.is_none());
    }
    
    #[test]
    fn test_parse_search_options_invalid_json() {
        // Invalid JSON should return default
        let opts = super::parse_search_options(Some("not json"));
        assert!(opts.tiktok.is_none());
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
}
