//! Worker - Main task processing loop
//!
//! The worker consumes tasks from Redis and processes them using the orchestrator.
//! Task structures are defined in the protocol module.
//!
//! ## Concurrency Model
//!
//! Uses Tokio's async tasks (similar to Go goroutines) for lightweight concurrency:
//! - Task-level concurrency controlled by `max_concurrent_tasks`
//! - Video-level concurrency controlled by `max_concurrent_videos` (per task)
//! - AI API calls rate-limited by global `AiRateLimiter`

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::adapters::redis::{RedisTaskConsumer, CrawlerTaskExt};
use crate::concurrency::GlobalRateLimiters;
use crate::domain::ConcurrencyConfig;
use crate::domain::errors::QueueError;
use crate::orchestrator::WorkflowOrchestrator;
use crate::protocol_gen::CrawlerTask;

/// Multi-platform worker that processes tasks from Redis
pub struct MultiPlatformWorker {
    /// Task consumer (Redis)
    task_consumer: Arc<RedisTaskConsumer>,
    
    /// Workflow orchestrator
    orchestrator: Arc<WorkflowOrchestrator>,
    
    /// Configuration
    config: WorkerConfig,
    
    /// Global rate limiters for API calls
    rate_limiters: GlobalRateLimiters,
    
    /// Shutdown signal
    shutdown: tokio::sync::watch::Receiver<bool>,
}

/// Worker configuration
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Number of concurrent tasks (legacy, use concurrency_config.max_concurrent_tasks)
    /// Kept for backward compatibility
    pub concurrency: usize,
    
    /// Delay between poll attempts when queue is empty
    pub poll_delay_ms: u64,
    
    /// Maximum retries for failed tasks
    pub max_retries: u32,
    
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
    
    /// Whether to continue on individual task errors
    pub continue_on_error: bool,
    
    /// Concurrency configuration for parallel processing
    pub concurrency_config: ConcurrencyConfig,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        let cc = ConcurrencyConfig::default();
        Self {
            concurrency: cc.max_concurrent_tasks,
            poll_delay_ms: 5000,
            max_retries: 3,
            retry_delay_ms: 1000,
            continue_on_error: true,
            concurrency_config: cc,
        }
    }
}

impl WorkerConfig {
    /// Create from environment variables
    pub fn from_env() -> Self {
        let cc = ConcurrencyConfig::from_env();
        Self {
            concurrency: cc.max_concurrent_tasks,
            concurrency_config: cc,
            ..Default::default()
        }
    }
    
    /// Set concurrency config
    pub fn with_concurrency_config(mut self, config: ConcurrencyConfig) -> Self {
        self.concurrency = config.max_concurrent_tasks;
        self.concurrency_config = config;
        self
    }
}

impl MultiPlatformWorker {
    /// Create a new worker
    pub fn new(
        task_consumer: Arc<RedisTaskConsumer>,
        orchestrator: Arc<WorkflowOrchestrator>,
        config: WorkerConfig,
    ) -> (Self, tokio::sync::watch::Sender<bool>) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        
        // Create global rate limiters from concurrency config
        let rate_limiters = GlobalRateLimiters::from_config(&config.concurrency_config);
        
        info!(
            max_concurrent_tasks = config.concurrency_config.max_concurrent_tasks,
            max_concurrent_videos = config.concurrency_config.max_concurrent_videos,
            ai_concurrency = config.concurrency_config.ai_concurrency,
            tikhub_concurrency = config.concurrency_config.tikhub_concurrency,
            "Initialized worker with concurrency configuration"
        );
        
        let worker = Self {
            task_consumer,
            orchestrator,
            config,
            rate_limiters,
            shutdown: shutdown_rx,
        };
        
        (worker, shutdown_tx)
    }
    
    /// Get reference to global rate limiters
    pub fn rate_limiters(&self) -> &GlobalRateLimiters {
        &self.rate_limiters
    }

    /// Run the worker
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let max_tasks = self.config.concurrency_config.max_concurrent_tasks;
        info!(
            max_concurrent_tasks = max_tasks,
            max_concurrent_videos = self.config.concurrency_config.max_concurrent_videos,
            ai_concurrency = self.config.concurrency_config.ai_concurrency,
            "Starting worker with parallel processing"
        );

        // Create a semaphore to limit task-level concurrency
        let semaphore = Arc::new(Semaphore::new(max_tasks));

        loop {
            // Check for shutdown signal
            if *self.shutdown.borrow() {
                info!("Shutdown signal received, stopping worker");
                break;
            }

            // Try to acquire a permit for concurrency control
            let permit = match semaphore.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    // All workers busy, wait a bit
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };

            // Try to consume a task
            match self.task_consumer.try_consume().await {
                Ok(Some(task)) => {
                    let orchestrator = self.orchestrator.clone();
                    let consumer = self.task_consumer.clone();
                    let config = self.config.clone();
                    
                    // Spawn task processing
                    tokio::spawn(async move {
                        let _permit = permit; // Hold permit until done
                        process_task(task, orchestrator, consumer, &config).await;
                    });
                }
                Ok(None) => {
                    // Queue is empty, release permit and wait
                    drop(permit);
                    tokio::time::sleep(Duration::from_millis(self.config.poll_delay_ms)).await;
                }
                Err(QueueError::Connection(e)) => {
                    // Connection error, release permit and retry
                    drop(permit);
                    error!(error = %e, "Redis connection error, retrying...");
                    tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
                Err(e) => {
                    drop(permit);
                    warn!(error = %e, "Queue error");
                    tokio::time::sleep(Duration::from_millis(self.config.poll_delay_ms)).await;
                }
            }
        }

        info!("Worker stopped");
        Ok(())
    }

    /// Run the worker with blocking consume (more efficient for single worker)
    pub async fn run_blocking(&mut self) -> anyhow::Result<()> {
        info!("Starting worker (blocking mode)");

        loop {
            // Check for shutdown signal
            if *self.shutdown.borrow() {
                info!("Shutdown signal received, stopping worker");
                break;
            }

            // Blocking consume
            match self.task_consumer.consume().await {
                Ok(task) => {
                    process_task(
                        task,
                        self.orchestrator.clone(),
                        self.task_consumer.clone(),
                        &self.config,
                    ).await;
                }
                Err(QueueError::Empty) => {
                    // Timeout, just continue
                    continue;
                }
                Err(QueueError::Connection(e)) => {
                    error!(error = %e, "Redis connection error, retrying...");
                    tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
                Err(e) => {
                    warn!(error = %e, "Queue error");
                    tokio::time::sleep(Duration::from_millis(self.config.poll_delay_ms)).await;
                }
            }
        }

        info!("Worker stopped");
        Ok(())
    }

    /// Process a single task (for testing or manual processing)
    pub async fn process_single(&self, task: CrawlerTask) {
        process_task(
            task,
            self.orchestrator.clone(),
            self.task_consumer.clone(),
            &self.config,
        ).await;
    }
}

/// Process a single task
async fn process_task(
    task: CrawlerTask,
    orchestrator: Arc<WorkflowOrchestrator>,
    consumer: Arc<RedisTaskConsumer>,
    _config: &WorkerConfig,
) {
    let task_id = task.task_id();
    let platform = task.platform_name();
    // Get campaign_id from task meta
    let campaign_id = task.campaign_id();
    info!(task_id, campaign_id, platform = %platform, "Processing task");

    let task_config = task.to_domain_task_config(campaign_id);

    match orchestrator.process_task(task_id, task_config).await {
        Ok(result) => {
            info!(
                task_id,
                success = result.success,
                contents = result.contents_processed,
                comments = result.comments_processed,
                analyses = result.analyses_generated,
                duration_ms = result.duration_ms,
                "Task completed"
            );

            // Acknowledge the task
            if let Err(e) = consumer.ack(task_id, result.success, result.error.as_deref()).await {
                error!(task_id, error = %e, "Failed to acknowledge task");
            }
        }
        Err(e) => {
            error!(task_id, error = %e, "Task failed");

            // Acknowledge with error
            if let Err(ack_err) = consumer.ack(task_id, false, Some(&e.to_string())).await {
                error!(task_id, error = %ack_err, "Failed to acknowledge task failure");
            }
        }
    }
}

/// Builder for creating workers with proper dependencies
pub struct WorkerBuilder {
    redis_url: Option<String>,
    queue_name: Option<String>,
    concurrency_config: ConcurrencyConfig,
    orchestrator: Option<Arc<WorkflowOrchestrator>>,
}

impl WorkerBuilder {
    pub fn new() -> Self {
        Self {
            redis_url: None,
            queue_name: None,
            concurrency_config: ConcurrencyConfig::default(),
            orchestrator: None,
        }
    }

    pub fn redis_url(mut self, url: impl Into<String>) -> Self {
        self.redis_url = Some(url.into());
        self
    }

    pub fn queue_name(mut self, name: impl Into<String>) -> Self {
        self.queue_name = Some(name.into());
        self
    }

    /// Set task-level concurrency (legacy, use concurrency_config for full control)
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency_config.max_concurrent_tasks = n;
        self
    }
    
    /// Set full concurrency configuration
    pub fn concurrency_config(mut self, config: ConcurrencyConfig) -> Self {
        self.concurrency_config = config;
        self
    }
    
    /// Load concurrency configuration from environment variables
    pub fn concurrency_from_env(mut self) -> Self {
        self.concurrency_config = ConcurrencyConfig::from_env();
        self
    }

    pub fn orchestrator(mut self, orch: Arc<WorkflowOrchestrator>) -> Self {
        self.orchestrator = Some(orch);
        self
    }

    pub fn build(self) -> Result<(MultiPlatformWorker, tokio::sync::watch::Sender<bool>), String> {
        let redis_url = self.redis_url
            .or_else(|| std::env::var("REDIS_URL").ok())
            .ok_or("redis_url is required")?;
        
        // Default queue name must match scheduler's queue
        let queue_name = self.queue_name
            .unwrap_or_else(|| "crawler:task_queue".to_string());
        
        let consumer = RedisTaskConsumer::new(&redis_url, &queue_name)
            .map_err(|e| format!("Failed to create Redis consumer: {}", e))?;
        
        let orchestrator = self.orchestrator
            .ok_or("orchestrator is required")?;
        
        let config = WorkerConfig {
            concurrency: self.concurrency_config.max_concurrent_tasks,
            concurrency_config: self.concurrency_config,
            ..Default::default()
        };
        
        debug!(
            ?config,
            "Building worker with configuration"
        );

        Ok(MultiPlatformWorker::new(
            Arc::new(consumer),
            orchestrator,
            config,
        ))
    }
}

impl Default for WorkerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_config_defaults() {
        let config = WorkerConfig::default();
        // Default concurrency now comes from ConcurrencyConfig
        assert_eq!(config.concurrency, 5); // max_concurrent_tasks default
        assert_eq!(config.concurrency_config.max_concurrent_tasks, 5);
        assert_eq!(config.concurrency_config.max_concurrent_videos, 5);
        assert_eq!(config.concurrency_config.ai_concurrency, 20);
        assert_eq!(config.max_retries, 3);
        assert!(config.continue_on_error);
    }
    
    #[test]
    fn test_worker_config_from_concurrency() {
        let cc = ConcurrencyConfig::default()
            .with_max_concurrent_tasks(10)
            .with_max_concurrent_videos(8);
        
        let config = WorkerConfig::default().with_concurrency_config(cc);
        
        assert_eq!(config.concurrency, 10);
        assert_eq!(config.concurrency_config.max_concurrent_tasks, 10);
        assert_eq!(config.concurrency_config.max_concurrent_videos, 8);
    }
}
