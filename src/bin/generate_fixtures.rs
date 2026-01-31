//! CLI tool for generating test fixtures from real TikHub API
//!
//! Usage:
//! ```bash
//! # Generate full test dataset
//! cargo run --bin generate-fixtures -- --full
//!
//! # Generate search fixture
//! cargo run --bin generate-fixtures -- --keyword "travel" --region US --count 10
//!
//! # Generate comments fixture
//! cargo run --bin generate-fixtures -- --video-id "7327061675382260482" --count 100
//!
//! # Generate user videos fixture
//! cargo run --bin generate-fixtures -- --username "tiktok" --count 10
//! ```

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

use glance_mind_agent_rs::fixtures::FixtureGenerator;

#[derive(Parser)]
#[command(name = "generate-fixtures")]
#[command(about = "Generate test fixtures from real TikHub API")]
#[command(version)]
struct Cli {
    /// TikHub API Key (or set TIKHUB_API_KEY env var)
    #[arg(long, env = "TIKHUB_API_KEY")]
    api_key: Option<String>,

    /// TikHub Base URL
    #[arg(long, env = "TIKHUB_BASE_URL", default_value = "https://api.tikhub.io")]
    base_url: String,

    /// Output directory for fixtures
    #[arg(long, short, default_value = "tests/fixtures/tiktok")]
    output: String,

    /// Generate full test dataset
    #[arg(long)]
    full: bool,

    /// Search keyword
    #[arg(long)]
    keyword: Option<String>,

    /// Region code (e.g., US, GB)
    #[arg(long, default_value = "US")]
    region: String,

    /// Video ID (for fetching comments)
    #[arg(long)]
    video_id: Option<String>,

    /// Username (for fetching user videos)
    #[arg(long)]
    username: Option<String>,

    /// Number of items to fetch
    #[arg(long, default_value = "10")]
    count: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Load .env file if present
    let _ = dotenvy::dotenv();

    info!("🔧 TikHub Fixture Generator");
    info!("   Output: {}", cli.output);
    info!("");

    let generator = FixtureGenerator::from_env_with_dir(&cli.output)?;

    if cli.full {
        info!("📦 Generating full test dataset...\n");
        generator.generate_full_dataset().await?;
    } else if let Some(keyword) = cli.keyword {
        info!(
            "🔍 Generating search fixture for keyword '{}'...\n",
            keyword
        );
        let path = generator
            .generate_search_fixture(&keyword, &cli.region, cli.count)
            .await?;
        info!("\n✅ Saved to: {}", path.display());
    } else if let Some(video_id) = cli.video_id {
        info!("💬 Generating comments fixture for video {}...\n", video_id);
        let path = generator
            .generate_comments_fixture(&video_id, cli.count)
            .await?;
        info!("\n✅ Saved to: {}", path.display());
    } else if let Some(username) = cli.username {
        info!("👤 Generating user videos fixture for @{}...\n", username);
        let path = generator
            .generate_user_videos_fixture(&username, cli.count)
            .await?;
        info!("\n✅ Saved to: {}", path.display());
    } else {
        eprintln!("Please specify one of: --full, --keyword, --video-id, or --username");
        eprintln!("\nRun with --help for usage information.");
        std::process::exit(1);
    }

    Ok(())
}
