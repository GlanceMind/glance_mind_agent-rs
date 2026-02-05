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
    init_global_registry, AiAnalyzer, CommentGateway, ContentGateway, InstagramAdapter,
    InstagramStrategy, MultiPlatformWorker, OpenAiAdapter, PlatformLookup, PlatformRegistry,
    PostgresAdapter, RedditAdapter, RedditStrategy, RedisTaskConsumer, TikHubAdapter,
    TikTokStrategy, TwitterAdapter, TwitterStrategy, WorkerConfig, WorkflowOrchestrator,
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
                .unwrap_or_else(|_| EnvFilter::new("info,glance_mind_agent_rs=debug")),
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
    let database_url = cli
        .database_url
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

    // Create adapters for all platforms
    info!("Creating adapters...");

    // TikTok adapter
    let tiktok_content = match TikHubAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn ContentGateway>,
        Err(e) => {
            error!("Failed to create TikTok adapter: {}", e);
            return Err(anyhow::anyhow!(
                "TikTok adapter initialization failed: {}",
                e
            ));
        }
    };
    let tiktok_comment = match TikHubAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn CommentGateway>,
        Err(e) => {
            error!("Failed to create TikTok comment adapter: {}", e);
            return Err(anyhow::anyhow!(
                "TikTok adapter initialization failed: {}",
                e
            ));
        }
    };

    // Instagram adapter
    let instagram_content = match InstagramAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn ContentGateway>,
        Err(e) => {
            error!("Failed to create Instagram adapter: {}", e);
            return Err(anyhow::anyhow!(
                "Instagram adapter initialization failed: {}",
                e
            ));
        }
    };
    let instagram_comment = match InstagramAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn CommentGateway>,
        Err(e) => {
            error!("Failed to create Instagram comment adapter: {}", e);
            return Err(anyhow::anyhow!(
                "Instagram adapter initialization failed: {}",
                e
            ));
        }
    };

    // Reddit adapter
    let reddit_content = match RedditAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn ContentGateway>,
        Err(e) => {
            error!("Failed to create Reddit adapter: {}", e);
            return Err(anyhow::anyhow!(
                "Reddit adapter initialization failed: {}",
                e
            ));
        }
    };
    let reddit_comment = match RedditAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn CommentGateway>,
        Err(e) => {
            error!("Failed to create Reddit comment adapter: {}", e);
            return Err(anyhow::anyhow!(
                "Reddit adapter initialization failed: {}",
                e
            ));
        }
    };

    // Twitter adapter
    let twitter_content = match TwitterAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn ContentGateway>,
        Err(e) => {
            error!("Failed to create Twitter adapter: {}", e);
            return Err(anyhow::anyhow!(
                "Twitter adapter initialization failed: {}",
                e
            ));
        }
    };
    let twitter_comment = match TwitterAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn CommentGateway>,
        Err(e) => {
            error!("Failed to create Twitter comment adapter: {}", e);
            return Err(anyhow::anyhow!(
                "Twitter adapter initialization failed: {}",
                e
            ));
        }
    };

    // OpenAI adapter
    let ai_adapter = match OpenAiAdapter::from_env() {
        Ok(adapter) => Arc::new(adapter) as Arc<dyn glance_mind_agent_rs::AiAnalyzer>,
        Err(e) => {
            error!("Failed to create OpenAI adapter: {}", e);
            return Err(anyhow::anyhow!(
                "OpenAI adapter initialization failed: {}",
                e
            ));
        }
    };

    // PostgreSQL adapter
    let postgres_adapter = match PostgresAdapter::from_url(&database_url) {
        Ok(adapter) => Arc::new(adapter),
        Err(e) => {
            error!("Failed to create PostgreSQL adapter: {}", e);
            return Err(anyhow::anyhow!(
                "PostgreSQL adapter initialization failed: {}",
                e
            ));
        }
    };

    // Build orchestrator with platform-specific gateways
    info!("Building orchestrator...");
    let orchestrator = WorkflowOrchestrator::builder()
        // Content gateways per platform
        .add_content_gateway("tiktok", tiktok_content)
        .add_content_gateway("instagram", instagram_content)
        .add_content_gateway("reddit", reddit_content)
        .add_content_gateway("twitter", twitter_content)
        // Comment gateways per platform
        .add_comment_gateway("tiktok", tiktok_comment)
        .add_comment_gateway("instagram", instagram_comment)
        .add_comment_gateway("reddit", reddit_comment)
        .add_comment_gateway("twitter", twitter_comment)
        // Common services
        .ai_analyzer(ai_adapter)
        .content_repository(postgres_adapter.clone())
        .prompt_repository(postgres_adapter.clone())
        .progress_tracker(postgres_adapter)
        // Platform strategies
        .add_strategy(Arc::new(TikTokStrategy::new()))
        .add_strategy(Arc::new(InstagramStrategy::new()))
        .add_strategy(Arc::new(RedditStrategy::new()))
        .add_strategy(Arc::new(TwitterStrategy::new()))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build orchestrator: {}", e))?;

    let orchestrator = Arc::new(orchestrator);

    // Create Redis task consumer
    info!("Connecting to Redis...");
    let mut task_consumer = RedisTaskConsumer::new(&cli.redis_url, &cli.queue_name)
        .map_err(|e| anyhow::anyhow!("Failed to create Redis consumer: {}", e))?;

    // Initialize the connection manager (required before use)
    task_consumer
        .init()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize Redis connection: {}", e))?;

    let task_consumer = Arc::new(task_consumer);

    // Create worker configuration
    let worker_config = WorkerConfig {
        concurrency: cli.workers,
        ..Default::default()
    };

    // Create worker
    let (mut worker, shutdown_tx) =
        MultiPlatformWorker::new(task_consumer, orchestrator, worker_config);

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
        Ok(adapter) => match adapter.health_check().await {
            Ok(true) => info!("  ✅ OpenAI: OK"),
            Ok(false) => {
                error!("  ❌ OpenAI: API not responding");
                return Err(anyhow::anyhow!("OpenAI health check failed"));
            }
            Err(e) => {
                error!("  ❌ OpenAI: {}", e);
                return Err(anyhow::anyhow!("OpenAI health check failed: {}", e));
            }
        },
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
/// Returns an error if database loading fails - platform config must come from database.
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

    // Connect to database - fail if connection fails
    let pool = establish_pool(database_url, Some(2))
        .map_err(|e| anyhow::anyhow!("Failed to connect to database for platform registry: {}", e))?;

    let mut conn = pool
        .get()
        .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;

    // Query platforms - fail if query fails
    let records: Vec<(i32, String, String, bool)> = gm_platforms::table
        .select((
            gm_platforms::id,
            gm_platforms::name,
            gm_platforms::display_name,
            gm_platforms::is_active,
        ))
        .load(&mut conn)
        .map_err(|e| anyhow::anyhow!("Failed to query platforms from database: {}", e))?;

    // Fail if no platforms found
    if records.is_empty() {
        return Err(anyhow::anyhow!(
            "No platforms found in gm_platforms table. Please ensure the database is properly initialized."
        ));
    }

    let count = records.len();
    let registry = PlatformRegistry::from_records(records);

    // Log loaded platforms for debugging
    info!("Loaded {} platforms from database:", count);
    for platform in registry.all_platforms() {
        info!(
            "  - {} (id={}, active={})",
            platform.name, platform.id, platform.is_active
        );
    }

    Ok(registry)
}
