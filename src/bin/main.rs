//! GlanceMind Agent - Main Entry Point
//!
//! This is the main worker process that:
//! 1. Consumes tasks from Redis queue
//! 2. Fetches content and comments from social media platforms
//! 3. Runs AI analysis on comments
//! 4. Saves results to database

use std::sync::Arc;

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use glance_mind_agent_rs::{
    TikHubAdapter, OpenAiAdapter, PostgresAdapter, RedisTaskConsumer,
    WorkflowOrchestrator, TikTokStrategy,
    MultiPlatformWorker, WorkerConfig,
    AiAnalyzer, // For health_check method
    PlatformRegistry, init_global_registry,
};

#[derive(Parser)]
#[command(name = "gm-agent")]
#[command(about = "GlanceMind Agent - AI-powered social media comment analysis")]
#[command(version)]
struct Cli {
    /// Database URL
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Redis URL for task queue
    #[arg(long, env = "REDIS_URL", default_value = "redis://127.0.0.1:6379")]
    redis_url: String,

    /// Queue name for tasks (must match scheduler's queue)
    #[arg(long, env = "AGENT_QUEUE_NAME", default_value = "crawler:task_queue")]
    queue_name: String,

    /// Number of concurrent workers
    #[arg(long, short, default_value = "1")]
    workers: usize,

    /// Run in blocking mode (more efficient for single worker)
    #[arg(long)]
    blocking: bool,

    /// Health check only - verify connections and exit
    #[arg(long)]
    health_check: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,glance_mind_agent_rs=debug"))
        )
        .init();

    let cli = Cli::parse();

    // Load .env file if present
    let _ = dotenvy::dotenv();

    info!("🚀 GlanceMind Agent Starting...");
    info!("   Redis: {}", cli.redis_url);
    info!("   Queue: {}", cli.queue_name);
    info!("   Workers: {}", cli.workers);
    info!("");

    // Get database URL
    let database_url = cli.database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .ok_or_else(|| anyhow::anyhow!("DATABASE_URL is required"))?;

    // Health check mode
    if cli.health_check {
        return run_health_check(&cli.redis_url, &database_url).await;
    }

    // Initialize platform registry (loaded from database or defaults)
    info!("Initializing platform registry...");
    let platform_registry = load_platform_registry(&database_url).await?;
    init_global_registry(platform_registry);
    info!("Platform registry initialized");

    // Create adapters
    info!("Creating adapters...");

    // TikHub adapter
    let tikhub_adapter = match TikHubAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn glance_mind_agent_rs::ContentGateway>,
        Err(e) => {
            error!("Failed to create TikHub adapter: {}", e);
            return Err(anyhow::anyhow!("TikHub adapter initialization failed: {}", e));
        }
    };

    // Also use TikHub for comments
    let comment_adapter = match TikHubAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn glance_mind_agent_rs::CommentGateway>,
        Err(e) => {
            error!("Failed to create TikHub comment adapter: {}", e);
            return Err(anyhow::anyhow!("TikHub adapter initialization failed: {}", e));
        }
    };

    // OpenAI adapter
    let ai_adapter = match OpenAiAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn glance_mind_agent_rs::AiAnalyzer>,
        Err(e) => {
            error!("Failed to create OpenAI adapter: {}", e);
            return Err(anyhow::anyhow!("OpenAI adapter initialization failed: {}", e));
        }
    };

    // PostgreSQL adapter
    let postgres_adapter = match PostgresAdapter::from_url(&database_url) {
        Ok(adapter) => Arc::new(adapter),
        Err(e) => {
            error!("Failed to create PostgreSQL adapter: {}", e);
            return Err(anyhow::anyhow!("PostgreSQL adapter initialization failed: {}", e));
        }
    };

    // Build orchestrator
    info!("Building orchestrator...");
    let orchestrator = WorkflowOrchestrator::builder()
        .content_gateway(tikhub_adapter)
        .comment_gateway(comment_adapter)
        .ai_analyzer(ai_adapter)
        .content_repository(postgres_adapter.clone())
        .prompt_repository(postgres_adapter.clone())
        .progress_tracker(postgres_adapter)
        .add_strategy(Arc::new(TikTokStrategy::new()))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build orchestrator: {}", e))?;

    let orchestrator = Arc::new(orchestrator);

    // Create Redis task consumer
    info!("Connecting to Redis...");
    let mut task_consumer = RedisTaskConsumer::new(&cli.redis_url, &cli.queue_name)
        .map_err(|e| anyhow::anyhow!("Failed to create Redis consumer: {}", e))?;
    
    // Initialize the connection manager (required before use)
    task_consumer.init().await
        .map_err(|e| anyhow::anyhow!("Failed to initialize Redis connection: {}", e))?;
    
    let task_consumer = Arc::new(task_consumer);

    // Create worker configuration
    let worker_config = WorkerConfig {
        concurrency: cli.workers,
        ..Default::default()
    };

    // Create worker
    let (mut worker, shutdown_tx) = MultiPlatformWorker::new(
        task_consumer,
        orchestrator,
        worker_config,
    );

    // Setup shutdown handler
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received Ctrl+C, shutting down...");
        let _ = shutdown_tx_clone.send(true);
    });

    // Run the worker
    info!("Worker ready, starting task processing...");
    if cli.blocking && cli.workers == 1 {
        worker.run_blocking().await?;
    } else {
        worker.run().await?;
    }

    info!("Agent stopped");
    Ok(())
}

async fn run_health_check(redis_url: &str, database_url: &str) -> anyhow::Result<()> {
    info!("Running health checks...");

    // Check Redis
    info!("  Checking Redis...");
    let mut consumer = RedisTaskConsumer::new(redis_url, "health_check")?;
    match consumer.init().await {
        Ok(_) => {
            if consumer.health_check().await {
                info!("  ✅ Redis: OK");
            } else {
                error!("  ❌ Redis: Failed (health check)");
                return Err(anyhow::anyhow!("Redis health check failed"));
            }
        }
        Err(e) => {
            error!("  ❌ Redis: Failed to connect - {}", e);
            return Err(anyhow::anyhow!("Redis connection failed: {}", e));
        }
    }

    // Check Database
    info!("  Checking Database...");
    match PostgresAdapter::from_url(database_url) {
        Ok(_) => info!("  ✅ Database: OK"),
        Err(e) => {
            error!("  ❌ Database: {}", e);
            return Err(anyhow::anyhow!("Database health check failed: {}", e));
        }
    }

    // Check TikHub
    info!("  Checking TikHub API...");
    match TikHubAdapter::from_env() {
        Ok(_) => info!("  ✅ TikHub: OK (credentials loaded)"),
        Err(e) => {
            error!("  ❌ TikHub: {}", e);
            return Err(anyhow::anyhow!("TikHub health check failed: {}", e));
        }
    }

    // Check OpenAI
    info!("  Checking OpenAI API...");
    match OpenAiAdapter::from_env() {
        Ok(adapter) => {
            match adapter.health_check().await {
                Ok(true) => info!("  ✅ OpenAI: OK"),
                Ok(false) => {
                    error!("  ❌ OpenAI: API not responding");
                    return Err(anyhow::anyhow!("OpenAI health check failed"));
                }
                Err(e) => {
                    error!("  ❌ OpenAI: {}", e);
                    return Err(anyhow::anyhow!("OpenAI health check failed: {}", e));
                }
            }
        }
        Err(e) => {
            error!("  ❌ OpenAI: {}", e);
            return Err(anyhow::anyhow!("OpenAI health check failed: {}", e));
        }
    }

    info!("");
    info!("All health checks passed! ✅");
    Ok(())
}

/// Load platform registry from database
///
/// This queries the gm_platforms table and initializes the platform mappings.
/// Falls back to defaults if database query fails.
async fn load_platform_registry(database_url: &str) -> anyhow::Result<PlatformRegistry> {
    use diesel::prelude::*;
    use glance_mind_agent_rs::db::establish_pool;

    // Define table for querying
    diesel::table! {
        gm_platforms (id) {
            id -> Int4,
            name -> Varchar,
            display_name -> Varchar,
            is_active -> Bool,
        }
    }

    // Try to load from database
    let pool = match establish_pool(database_url, Some(2)) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to connect to database for platform registry: {}", e);
            tracing::warn!("Using default platform mappings");
            return Ok(PlatformRegistry::with_defaults());
        }
    };

    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to get database connection: {}", e);
            tracing::warn!("Using default platform mappings");
            return Ok(PlatformRegistry::with_defaults());
        }
    };

    // Query platforms
    let records: Result<Vec<(i32, String, String, bool)>, _> = gm_platforms::table
        .select((
            gm_platforms::id,
            gm_platforms::name,
            gm_platforms::display_name,
            gm_platforms::is_active,
        ))
        .load(&mut conn);

    match records {
        Ok(platforms) => {
            if platforms.is_empty() {
                tracing::warn!("No platforms found in database, using defaults");
                return Ok(PlatformRegistry::with_defaults());
            }
            
            let count = platforms.len();
            let registry = PlatformRegistry::from_records(platforms);
            info!("Loaded {} platforms from database", count);
            Ok(registry)
        }
        Err(e) => {
            tracing::warn!("Failed to query platforms from database: {}", e);
            tracing::warn!("Using default platform mappings");
            Ok(PlatformRegistry::with_defaults())
        }
    }
}
