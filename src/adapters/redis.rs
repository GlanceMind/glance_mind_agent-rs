//! Redis Adapter - Task queue consumer
//!
//! This adapter consumes tasks from a Redis queue and provides
//! them to the worker for processing.
//!
//! Task structures are defined in the protocol module (from glance_mind_protocol).
//! Platform mappings are loaded from the global registry (initialized at startup).

use redis::{AsyncCommands, Client, aio::MultiplexedConnection};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config::platform::{global_registry, PlatformLookup};
use crate::domain::errors::{QueueError, QueueResult};
use crate::protocol_gen::{
    CrawlerTask, CrawlerTaskMeta, CrawlerTaskSpec, 
    TaskConfig as ProtocolTaskConfig, TaskFilters, Platform,
};

/// Redis task consumer for the agent
pub struct RedisTaskConsumer {
    client: Client,
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
            queue_name: queue_name.to_string(),
            result_queue: format!("{}_results", queue_name),
            timeout_secs: 30,
        })
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self, QueueError> {
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        
        let queue_name = std::env::var("AGENT_QUEUE_NAME")
            .unwrap_or_else(|_| "gm:agent:tasks".to_string());
        
        Self::new(&redis_url, &queue_name)
    }

    /// Set the blocking timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Get a connection
    async fn get_conn(&self) -> QueueResult<MultiplexedConnection> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| QueueError::Connection(e.to_string()))
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

    fn to_domain_task_config(&self, campaign_id: i32) -> crate::domain::TaskConfig {
        let max_count = self.max_count();
        
        crate::domain::TaskConfig::new(campaign_id, &self.platform_name())
            .with_keywords(self.keywords())
            .with_region(self.region().unwrap_or_else(|| "US".to_string()))
            .with_max_videos(max_count)
            .with_max_comments_per_video(50)
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
}
