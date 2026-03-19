# Twitter Support Guide

## Status

Twitter support is functionally complete for the main production paths:

- Keyword search via TikHub search
- Hashtag search
- Direct handle lookup via `twitter_handle:`
- Direct tweet fetch via `twitter_tweet_id:`
- Comment/reply fetch and deduplication
- Persistence into `gm_agent_twitter_tweets` and `gm_agent_twitter_comments`
- Scheduler -> agent -> PostgreSQL E2E validation with the internal TikHub mock

The implementation already includes platform strategy parsing, Redis `search_options` propagation, TikHub detail fetch support, workflow orchestration, Postgres read/write symmetry, and CI entrypoints for Twitter E2E.

## Supported Keyword Formats

Twitter campaigns currently support these keyword forms:

| Keyword form | Meaning |
| --- | --- |
| `twitter_tweet_id:<tweet_id>` | Fetch one specific tweet by tweet id |
| `twitter_handle:<screen_name>` | Fetch a user's timeline by handle |
| `twitter_rest_id:<rest_id>` | Fetch a user's timeline by Twitter/X rest id |
| `#rustlang` | Hashtag search |
| `OpenAI` | Plain text search |
| `1234567890123456789` | Treated as tweet id if numeric and long enough |

Twitter-specific campaign `search_options` are supported in this shape:

```json
{
  "twitter": {
    "search_type": "Top"
  }
}
```

Supported `search_type` values currently normalize to:

- `Latest`
- `Top`
- `Media`
- `People`
- `Lists`

## What Is Covered By Tests

### Unit tests

Covered in repo:

- `src/strategies/twitter.rs`
  - keyword parsing for handle, rest id, tweet id, hashtag, plain search
  - `search_options` construction
- `src/adapters/twitter.rs`
  - content mapping
  - comment mapping
  - search type propagation
  - direct tweet detail fetch
  - comment deduplication/root-tweet filtering
- `src/adapters/redis.rs`
  - Twitter `search_options` parsing and propagation

### Live API tests

`tests/twitter_real_api_test.rs` covers:

- search contract and adapter mapping
- handle/timeline contract and adapter mapping
- direct tweet detail contract and adapter mapping
- single-page comment fetch
- paginated `fetch_all_comments` dedupe behavior

### Real DB workflow tests

`tests/twitter_real_db_test.rs` covers:

- search workflow -> real PostgreSQL
- handle workflow -> real PostgreSQL
- tweet-id workflow -> real PostgreSQL

These tests assert the persisted columns in:

- `gm_agent_twitter_tweets`
- `gm_agent_twitter_comments`

### E2E tests

`e2e/test_e2e_twitter.py` covers the mock-based full path:

- campaign activation
- scheduler task creation
- agent completion
- saved tweet field assertions
- saved comment field assertions
- wallet/financial side effects

CI also includes `make e2e-twitter` in `.github/workflows/test.yml`.

## Remaining Gaps And Residual Risks

Twitter support is strong, but it is not yet exhaustively covered.

Remaining gaps:

1. `twitter_rest_id:` is implemented, but it does not yet have dedicated live API, real DB workflow, or E2E coverage.
2. Twitter-specific Postgres upsert/update branches are covered mainly through full workflow tests, but not with focused unit tests for malformed `raw_data`, insert-vs-update behavior, or update helpers.
3. Twitter adapter error-path tests for 401/402/429/5xx are still thin. The happy path is well covered; supplier failure behavior is less directly exercised.
4. `--health_check` validates Redis, PostgreSQL, generic TikHub credentials, and OpenAI, but it does not perform a Twitter-specific live contract check.

Practical conclusion:

- For `twitter_tweet_id`, `twitter_handle`, plain search, hashtag, comments, persistence, and mock E2E, support is ready.
- For `twitter_rest_id` and some failure/update edge cases, coverage is not yet fully complete.

## Production Configuration

### Required environment variables

| Variable | Required | Notes |
| --- | --- | --- |
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `TIKHUB_API_KEY` | Yes | Required for Twitter/TikTok/Instagram/Reddit |
| `OPENAI_API_KEY` | Yes | Required for AI analysis |
| `REDIS_URL` | Usually yes | Defaults exist in some entrypoints, but should be set explicitly in production |

### Optional but commonly needed

| Variable | Default | Notes |
| --- | --- | --- |
| `TIKHUB_BASE_URL` | `https://api.tikhub.io` | Use the real TikHub base URL in production |
| `OPENAI_BASE_URL` | `https://timicc.com/v1` | Override if you use another OpenAI-compatible endpoint |
| `AGENT_QUEUE_NAME` | `crawler:task_queue` | Must match scheduler |
| `RUST_LOG` | `info,glance_mind_agent_rs=debug` | Adjust for production logging |
| `AGENT_MAX_CONCURRENT_TASKS` | `5` | Global task concurrency |
| `AGENT_MAX_CONCURRENT_VIDEOS` | `5` | Per-task content concurrency |
| `AGENT_AI_CONCURRENCY` | `20` | AI concurrency |
| `AGENT_TIKHUB_CONCURRENCY` | `3` | Shared TikHub concurrency across Twitter/TikTok/Instagram/Reddit |
| `AGENT_AI_MIN_INTERVAL_MS` | `50` | AI throttle |

### Facebook variables

Twitter does not require Facebook variables. These are only needed if the same deployment also handles Facebook campaigns:

- `FACEBOOK_RAPIDAPI_KEY`
- `FACEBOOK_RAPIDAPI_HOST`
- `FACEBOOK_RAPIDAPI_BASE_URL`

## Where To Configure These In The Repo

### Local or server `.env`

Use:

- `.env.example`

Recommended production values:

```env
DATABASE_URL=postgresql://user:password@db-host:5432/glance_mind
REDIS_URL=redis://redis-host:6379/0
AGENT_QUEUE_NAME=crawler:task_queue

TIKHUB_API_KEY=your_real_tikhub_key
TIKHUB_BASE_URL=https://api.tikhub.io

OPENAI_API_KEY=your_openai_compatible_key
OPENAI_BASE_URL=https://timicc.com/v1

RUST_LOG=info,glance_mind_agent_rs=debug
AGENT_MAX_CONCURRENT_TASKS=5
AGENT_MAX_CONCURRENT_VIDEOS=5
AGENT_AI_CONCURRENCY=20
AGENT_TIKHUB_CONCURRENCY=3
AGENT_AI_MIN_INTERVAL_MS=50
```

### Development compose

Use:

- `docker-compose.dev.yml`

This file already wires:

- `TIKHUB_API_KEY`
- `TIKHUB_BASE_URL`
- `DATABASE_URL`
- `REDIS_URL`
- concurrency env vars

### Production compose

Use:

- `docker-compose.agent-rs.yml`

This compose file loads `.env` via:

```yaml
env_file:
  - .env
```

### GitHub deploy workflow

Use:

- `.github/workflows/deploy.yml`

This workflow builds a `.env` file from:

- GitHub Secrets
- GitHub Variables

At minimum for Twitter production:

- Secret: `DATABASE_URL`
- Secret: `TIKHUB_API_KEY`
- Secret: `OPENAI_API_KEY`
- Variable or default: `TIKHUB_BASE_URL`

## Important Mock vs Production Difference

E2E uses the internal TikHub mock:

- `e2e/docker-compose.yml`
- `e2e/.env.example`
- `e2e/scripts/run_platform_test.sh`

Do not copy the E2E Twitter values into production.

Production should use:

```env
TIKHUB_BASE_URL=https://api.tikhub.io
TIKHUB_API_KEY=<real key>
```

Not:

```env
TIKHUB_BASE_URL=http://tikhub-mock:8500
TIKHUB_API_KEY=mock-tikhub-key
```

## Worker Count Note

Concurrency env vars such as `AGENT_MAX_CONCURRENT_TASKS` and `AGENT_TIKHUB_CONCURRENCY` are read by the application.

However, the process worker count itself is currently controlled by the CLI flag:

```bash
gm-agent --workers 2
```

The binary does not currently bind `AGENT_WORKERS` directly as a CLI env input, so if you want more than the default single worker in production, pass `--workers` explicitly in the container command or startup command.

## Database Requirements

Before enabling Twitter campaigns, ensure the database schema includes:

- `gm_agent_twitter_tweets`
- `gm_agent_twitter_comments`

The repo already contains the required schema definitions in:

- `src/db/schema.rs`
- `src/db/models.rs`
- `e2e/init-scripts/01_schema.sql`

## Recommended Production Rollout Checklist

1. Apply the database schema that includes the Twitter tables.
2. Set a real `TIKHUB_API_KEY`.
3. Set `TIKHUB_BASE_URL=https://api.tikhub.io`.
4. Set `OPENAI_API_KEY` and `OPENAI_BASE_URL`.
5. Ensure scheduler and agent use the same `AGENT_QUEUE_NAME`.
6. Start with conservative concurrency, especially `AGENT_TIKHUB_CONCURRENCY=3`.
7. Prefer first rollout with `twitter_tweet_id:` or `twitter_handle:` campaigns before broader search traffic.
8. Monitor for TikHub 401, 402, and 429 errors.

## Suggested Verification Before Go-Live

### Basic dependency check

```bash
cargo run --release --bin gm-agent -- --health_check
```

### Twitter live API verification

```bash
cargo test --test twitter_real_api_test -- --nocapture --test-threads=1
```

### Twitter real DB workflow verification

```bash
cargo test --test twitter_real_db_test -- --nocapture --test-threads=1
```

### Mock E2E verification

```bash
make e2e-twitter
```

## Recommendation

Current Twitter support is good enough for rollout on the core paths, but if you want to call it fully comprehensive, the next additions should be:

1. Live API coverage for `twitter_rest_id:`
2. Real DB workflow coverage for `twitter_rest_id:`
3. Focused Twitter Postgres upsert/update tests
4. Focused Twitter adapter error-path tests
