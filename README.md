# GlanceMind Agent-RS

AI-powered social media comment analysis agent - High-performance Rust implementation.

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-Proprietary-blue.svg)]()

## Overview

`glance_mind_agent_rs` is a Rust rewrite of the Python-based GlanceMind Agent, designed for high-performance, concurrent processing of social media content analysis tasks. It consumes tasks from Redis queue, fetches content via TikHub API and Facebook RapidAPI, performs AI analysis, and stores results to PostgreSQL.

### Key Features

- **High Concurrency**: Tokio-based async runtime with configurable rate limiters
- **Hexagonal Architecture**: Clean separation of domain, ports, and adapters
- **Multi-Platform Support**: Extensible platform strategy pattern (TikTok, Instagram, etc.)
- **Facebook Support**: RapidAPI `facebook-scraper3` integration for posts, pages, places, and post URL workflows
- **AI Integration**: DeepSeek OpenAI-compatible API support for comment analysis
- **Robust Error Handling**: Typed errors with retry mechanisms for transient failures
- **E2E Testing**: Complete Docker-based test environment

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Worker Layer                              │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │   MultiPlatformWorker (Redis consumer, task dispatcher)  │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Orchestrator Layer                           │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  WorkflowOrchestrator (coordinates content→AI→storage)   │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│  ContentGateway │ │   AiAnalyzer    │ │ ContentRepository│
│    (Port)       │ │    (Port)       │ │     (Port)       │
└─────────────────┘ └─────────────────┘ └─────────────────┘
          │                   │                   │
          ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│  TikHubAdapter  │ │  OpenAiAdapter  │ │ PostgresAdapter  │
│   (Adapter)     │ │   (Adapter)     │ │    (Adapter)     │
└─────────────────┘ └─────────────────┘ └─────────────────┘
          │                   │                   │
          ▼                   ▼                   ▼
    TikHub API          AI Service           PostgreSQL
```

## Quick Start

### Prerequisites

- Rust 1.70+
- PostgreSQL 15+
- Redis 7+
- TikHub API Key
- DeepSeek API key

### Installation

1. Clone the repository:
```bash
git clone git@github.com:GlanceMind/glance_mind_agent-rs.git
cd glance_mind_agent-rs
```

2. Copy and configure environment:
```bash
cp .env.example .env
# Edit .env with your credentials
```

3. Build the project:
```bash
cargo build --release
```

### Run the Agent

```bash
# Run with default settings
cargo run --release --bin gm-agent

# Run with debug logging
RUST_LOG=debug cargo run --release --bin gm-agent
```

## Configuration

Twitter deployment notes and coverage status:

- `docs/twitter-support.md`

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Yes | - | PostgreSQL connection string |
| `TIKHUB_API_KEY` | Yes | - | TikHub API key |
| `DEEPSEEK_API_KEY` | Yes | - | DeepSeek API key |
| `FACEBOOK_RAPIDAPI_KEY` | Facebook only | - | RapidAPI key for Facebook campaigns |
| `REDIS_URL` | No | `redis://localhost:6379/0` | Redis connection URL |
| `TIKHUB_BASE_URL` | No | `https://api.tikhub.io` | TikHub API base URL |
| `FACEBOOK_RAPIDAPI_HOST` | No | `facebook-scraper3.p.rapidapi.com` | RapidAPI host header |
| `FACEBOOK_RAPIDAPI_BASE_URL` | No | `https://facebook-scraper3.p.rapidapi.com` | Facebook RapidAPI base URL |
| `DEEPSEEK_BASE_URL` | No | `https://api.deepseek.com/v1` | DeepSeek OpenAI-compatible base URL |
| `DEEPSEEK_MODEL` | No | `deepseek-chat` | DeepSeek model name |
| `RUST_LOG` | No | `info` | Log level |

### Concurrency Settings

Concurrency is configured per-campaign via the database template:

| Setting | Default | Description |
|---------|---------|-------------|
| `max_tasks` | 5 | Maximum parallel task processing |
| `max_videos_per_task` | 5 | Videos processed per task |
| `max_ai_concurrent` | 20 | AI API concurrent requests |

## Project Structure

```
glance_mind_agent_rs/
├── src/
│   ├── lib.rs                 # Library entry, re-exports
│   ├── bin/
│   │   ├── main.rs            # Agent worker entry point
│   │   └── generate_fixtures.rs
│   ├── config/                # Platform registry, settings
│   ├── domain/                # Core entities (Content, Comment, etc.)
│   ├── ports/                 # Trait definitions (interfaces)
│   │   ├── content_gateway.rs # Fetch content from platforms
│   │   ├── ai_analyzer.rs     # AI analysis interface
│   │   └── content_repository.rs
│   ├── adapters/              # Concrete implementations
│   │   ├── tikhub.rs          # TikHub API adapter
│   │   ├── openai.rs          # OpenAI-compatible adapter
│   │   ├── postgres.rs        # Database adapter
│   │   └── redis.rs           # Task queue consumer
│   ├── strategies/            # Platform-specific behavior
│   ├── orchestrator.rs        # Workflow coordination
│   ├── worker.rs              # Multi-platform worker
│   ├── concurrency.rs         # Rate limiters
│   ├── tikhub/                # TikHub client library
│   ├── db/                    # Diesel ORM setup
│   └── protocol_gen/          # Shared protocol definitions
├── e2e/                       # End-to-end tests (Docker)
│   ├── docker-compose.yml
│   ├── test_e2e_china_travel.py
│   └── scripts/
├── tests/                     # Integration tests
└── Cargo.toml
```

## Development

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests (requires database)
cargo test --test integration_test

# E2E tests (Docker)
cd e2e && ./scripts/start.sh
```

### Generate Test Fixtures

```bash
# Generate from real TikHub API
cargo run --bin generate-fixtures -- --keyword "travel" --region US --count 10
cargo run --bin generate-fixtures -- --video-id "7327061675382260482" --count 100
```

### Code Quality

```bash
# Format code
cargo fmt

# Lint
cargo clippy

# Check compilation
cargo check
```

## Deployment

### Docker

```bash
# Build and run with Docker Compose
docker-compose -f docker-compose.agent-rs.yml up -d --build
```

### GitHub Actions

The project includes a CD workflow (`.github/workflows/deploy.yml`) that:
1. Builds the Docker image
2. Deploys to the configured server
3. Uses GitHub Secrets for sensitive configuration

The deployment workflow reads configuration from GitHub `secrets.*` and `vars.*`, so you can configure these at the organization, repository, or environment level.

Required GitHub Secrets:
- `DATABASE_URL`
- `TIKHUB_API_KEY`
- `DEEPSEEK_API_KEY`

Optional GitHub Secret:
- `FACEBOOK_RAPIDAPI_KEY` (required only if you want Facebook campaigns enabled in production or live Facebook tests)

Optional GitHub Variables:
- `REDIS_URL` (default: `redis://host.docker.internal:6379/0`)
- `TIKHUB_BASE_URL` (default: `https://api.tikhub.io`)
- `FACEBOOK_RAPIDAPI_HOST` (default: `facebook-scraper3.p.rapidapi.com`)
- `FACEBOOK_RAPIDAPI_BASE_URL` (default: `https://facebook-scraper3.p.rapidapi.com`)
- `DEEPSEEK_BASE_URL` (default: `https://api.deepseek.com/v1`)
- `DEEPSEEK_MODEL` (default: `deepseek-chat`)
- `RUST_LOG` (default: `info,glance_mind_agent_rs=debug`)

## Protocol Sync

This project shares protocol definitions with other GlanceMind services:

```bash
# In glance_mind_protocol directory
make sync
```

This syncs `lib_inline.rs` to `src/protocol_gen/mod.rs`.

## API Endpoints Used

### TikHub API

| Endpoint | Description |
|----------|-------------|
| `/api/v1/tiktok/app/v3/fetch_video_search_result` | Search videos by keyword |
| `/api/v1/tiktok/web/fetch_post_comment` | Fetch video comments |
| `/api/v1/tiktok/app/v3/fetch_user_post_videos` | Fetch user's videos |

### Facebook RapidAPI

| Endpoint | Description |
|----------|-------------|
| `/search/posts` | Search Facebook posts by keyword |
| `/search/pages` | Resolve page search queries to Facebook page IDs |
| `/search/places` | Resolve place queries to place/page IDs |
| `/page/posts` | Fetch posts for a Facebook page or place |
| `/post` | Fetch a single Facebook post by `post_id`/`pfbid` |
| `/post/comments` | Fetch comments for a Facebook post |

### AI Service

DeepSeek is used through the OpenAI Chat Completions API format:
- `POST /chat/completions`

## Database Tables

| Table | Description |
|-------|-------------|
| `gm_campaigns` | Campaign configuration |
| `gm_crawler_tasks` | Task queue status |
| `gm_agent_videos` | Processed video metadata |
| `gm_agent_comments` | Comment analysis results |
| `gm_agent_facebook_posts` | Processed Facebook post metadata |
| `gm_agent_facebook_comments` | Facebook comment analysis results |
| `gm_wallet_bill` | Consumption tracking |

## Related Projects

- [glance_mind_api](https://github.com/GlanceMind/glance_mind_api) - Core backend API (Rust/Axum)
- [glance_mind_worker](https://github.com/GlanceMind/glance_mind_worker) - Scheduler & Executor (Python)
- [glance_mind_protocol](https://github.com/GlanceMind/glance_mind_protocol) - Shared protocol definitions

## License

Proprietary - GlanceMind Team
