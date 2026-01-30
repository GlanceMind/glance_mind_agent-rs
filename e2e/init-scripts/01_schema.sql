-- =============================================================================
-- GlanceMind E2E Test Database Schema
-- =============================================================================
-- This schema matches the production database used by Python agent and scheduler.
-- Source: glance_mind_rust/crates/db/migrations/baseline + host database
-- =============================================================================

-- Helper functions
CREATE OR REPLACE FUNCTION diesel_manage_updated_at(_tbl regclass) RETURNS void
    LANGUAGE plpgsql AS $$
BEGIN
    EXECUTE format('CREATE TRIGGER set_updated_at BEFORE UPDATE ON %s
                    FOR EACH ROW EXECUTE PROCEDURE diesel_set_updated_at()', _tbl);
END;
$$;

CREATE OR REPLACE FUNCTION diesel_set_updated_at() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    IF (NEW IS DISTINCT FROM OLD AND NEW.updated_at IS NOT DISTINCT FROM OLD.updated_at) THEN
        NEW.updated_at := current_timestamp;
    END IF;
    RETURN NEW;
END;
$$;

-- =============================================================================
-- Core Tables
-- =============================================================================

-- Users table
CREATE TABLE IF NOT EXISTS gm_users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE,
    username VARCHAR(50) UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    full_name VARCHAR DEFAULT '' NOT NULL,
    role VARCHAR DEFAULT 'user' NOT NULL,
    status VARCHAR(50) DEFAULT 'ACTIVE' NOT NULL,
    is_active BOOLEAN DEFAULT true NOT NULL,
    invitation_code VARCHAR(50) UNIQUE,
    referred_by VARCHAR(50),
    invite_code VARCHAR(36) UNIQUE,
    invited_by VARCHAR(36),
    company_name VARCHAR(255),
    api_key VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at TIMESTAMPTZ
);

-- User wallets
CREATE TABLE IF NOT EXISTS gm_user_wallets (
    user_id INTEGER PRIMARY KEY REFERENCES gm_users(id) ON DELETE CASCADE,
    balance_points NUMERIC(10,2) DEFAULT 0.00 NOT NULL CHECK (balance_points >= 0),
    frozen_points NUMERIC(10,2) DEFAULT 0.00 NOT NULL CHECK (frozen_points >= 0),
    deposit_cny NUMERIC DEFAULT 0 NOT NULL,
    deposit_usd NUMERIC DEFAULT 0 NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    CONSTRAINT chk_balance_frozen CHECK (balance_points >= frozen_points)
);

-- Platforms
CREATE TABLE IF NOT EXISTS gm_platforms (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    display_name VARCHAR NOT NULL,
    base_url VARCHAR DEFAULT '' NOT NULL,
    page_size INTEGER DEFAULT 20 NOT NULL,
    is_active BOOLEAN DEFAULT true NOT NULL,
    content_table_name VARCHAR(100),
    comment_table_name VARCHAR(100),
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ
);

-- Regions
CREATE TABLE IF NOT EXISTS gm_regions (
    id SERIAL PRIMARY KEY,
    platform_id INTEGER NOT NULL REFERENCES gm_platforms(id) ON DELETE CASCADE,
    code VARCHAR NOT NULL,
    name VARCHAR DEFAULT '' NOT NULL,
    display_name VARCHAR NOT NULL,
    is_active BOOLEAN DEFAULT true NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    UNIQUE (platform_id, code)
);

-- AI Models
CREATE TABLE IF NOT EXISTS gm_ai_models (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    provider VARCHAR NOT NULL,
    model_key VARCHAR NOT NULL,
    model_type VARCHAR(50) DEFAULT 'chat' NOT NULL,
    cost_multiplier NUMERIC(10,2) DEFAULT 1.0 NOT NULL,
    is_active BOOLEAN DEFAULT true NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    CONSTRAINT valid_model_type CHECK (model_type IN ('chat', 'video'))
);

-- Pricing rules
CREATE TABLE IF NOT EXISTS gm_pricing_rules (
    id SERIAL PRIMARY KEY,
    action_type VARCHAR NOT NULL,
    platform_id INTEGER REFERENCES gm_platforms(id) ON DELETE CASCADE,
    cost_points NUMERIC(10,2) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    UNIQUE (action_type, platform_id)
);

-- Social groups
CREATE TABLE IF NOT EXISTS gm_social_groups (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES gm_users(id) ON DELETE CASCADE,
    platform_id INTEGER NOT NULL REFERENCES gm_platforms(id),
    group_name VARCHAR NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ
);

-- Social accounts
CREATE TABLE IF NOT EXISTS gm_social_accounts (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES gm_users(id) ON DELETE CASCADE,
    platform_id INTEGER NOT NULL REFERENCES gm_platforms(id),
    username VARCHAR NOT NULL,
    group_id INTEGER REFERENCES gm_social_groups(id) ON DELETE CASCADE,
    proxy_url VARCHAR,
    status VARCHAR DEFAULT 'ACTIVE' NOT NULL,
    health_score INTEGER DEFAULT 100 CHECK (health_score >= 0 AND health_score <= 100),
    cookie TEXT DEFAULT '' NOT NULL,
    daily_max_replies INTEGER DEFAULT 50 NOT NULL,
    device_id VARCHAR(255),
    profile_name VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ
);

-- Campaigns (matches production schema)
CREATE TABLE IF NOT EXISTS gm_campaigns (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES gm_users(id),
    name VARCHAR NOT NULL,
    status VARCHAR DEFAULT 'DRAFT' NOT NULL,
    platform_id INTEGER NOT NULL REFERENCES gm_platforms(id),
    region_id INTEGER NOT NULL REFERENCES gm_regions(id),
    ai_model_id INTEGER NOT NULL REFERENCES gm_ai_models(id),
    target_audience TEXT,
    product_prompt TEXT DEFAULT '' NOT NULL,
    schedule_config JSONB,
    enable_ai_refactor BOOLEAN DEFAULT false,
    persona_id INTEGER,
    max_scan_count INTEGER DEFAULT 1000,
    budget_cap NUMERIC(10,2),
    end_date TIMESTAMPTZ,
    schedule_type VARCHAR NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    keyword TEXT,
    social_group_id INTEGER REFERENCES gm_social_groups(id),
    call_to_action TEXT,
    tone_of_voice TEXT,
    additional_info TEXT,
    total_scanned INTEGER DEFAULT 0 NOT NULL,
    auto_like BOOLEAN DEFAULT true NOT NULL,
    auto_follow BOOLEAN DEFAULT true NOT NULL,
    auto_dm BOOLEAN DEFAULT true NOT NULL,
    pending_consumption NUMERIC DEFAULT 0 NOT NULL,
    actual_consumption NUMERIC DEFAULT 0 NOT NULL,
    is_frozen BOOLEAN DEFAULT false NOT NULL,
    search_options JSONB DEFAULT '{}',
    auto_reply_comments BOOLEAN DEFAULT true NOT NULL,
    auto_reply_post BOOLEAN DEFAULT true NOT NULL,
    completed_reason TEXT
);

-- Campaign templates (for AI prompts/strategies)
CREATE TABLE IF NOT EXISTS gm_campaign_templates (
    id SERIAL PRIMARY KEY,
    campaign_id INTEGER NOT NULL REFERENCES gm_campaigns(id) ON DELETE CASCADE,
    weight INTEGER NOT NULL DEFAULT 1,
    reply_prompt TEXT,
    dm_prompt TEXT,
    reply_post_prompt TEXT,
    name VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ
);

-- Crawler tasks (matches production schema - status is VARCHAR)
CREATE TABLE IF NOT EXISTS gm_crawler_tasks (
    id SERIAL PRIMARY KEY,
    campaign_id INTEGER NOT NULL REFERENCES gm_campaigns(id) ON DELETE CASCADE,
    keywords TEXT[],
    max_count INTEGER NOT NULL,
    process_count INTEGER DEFAULT 0 NOT NULL,
    status VARCHAR(50) DEFAULT 'init' NOT NULL,
    search_offset INTEGER DEFAULT 0 NOT NULL,
    search_limit INTEGER DEFAULT 10 NOT NULL,
    reserved_amount NUMERIC(18,4) DEFAULT 0,
    actual_consumption NUMERIC(18,4) DEFAULT 0,
    settled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ
);

-- =============================================================================
-- Agent Tables (TikTok - used by Python agent)
-- =============================================================================

-- Agent videos (TikTok)
CREATE TABLE IF NOT EXISTS gm_agent_videos (
    id SERIAL PRIMARY KEY,
    video_id VARCHAR(255),
    author VARCHAR(255),
    author_unique_id VARCHAR(255),
    description TEXT,
    url TEXT,
    task_id INTEGER NOT NULL REFERENCES gm_crawler_tasks(id),
    campaign_id INTEGER REFERENCES gm_campaigns(id) ON DELETE CASCADE,
    like_count INTEGER DEFAULT 0,
    comment_count INTEGER DEFAULT 0,
    share_count INTEGER DEFAULT 0,
    play_count INTEGER DEFAULT 0,
    publish_time BIGINT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    UNIQUE (task_id, video_id)
);

-- Agent comments (TikTok - stores AI analysis results)
CREATE TABLE IF NOT EXISTS gm_agent_comments (
    id SERIAL PRIMARY KEY,
    video_db_id INTEGER NOT NULL REFERENCES gm_agent_videos(id) ON DELETE CASCADE,
    comment_id VARCHAR(255) NOT NULL,
    user_nickname VARCHAR(255),
    user_unique_id VARCHAR(255),
    content TEXT,
    reason TEXT,
    suggested_reply TEXT,
    suggested_dm TEXT,
    suggested_reply_post TEXT,
    create_time TIMESTAMP,
    campaign_id INTEGER NOT NULL REFERENCES gm_campaigns(id) ON DELETE CASCADE,
    status SMALLINT DEFAULT 0 NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    -- Unique constraint: same comment in same campaign is upserted
    UNIQUE (campaign_id, comment_id)
);

-- Wallet transactions
CREATE TABLE IF NOT EXISTS gm_wallet_transactions (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES gm_users(id) ON DELETE CASCADE,
    amount NUMERIC(10,2) NOT NULL,
    type VARCHAR NOT NULL,
    payment_method VARCHAR,
    external_txn_id VARCHAR,
    reference_id INTEGER,
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ
);

-- =============================================================================
-- Video Generation Tasks (for Scheduler)
-- =============================================================================
CREATE TABLE IF NOT EXISTS gm_video_generation_tasks (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES gm_users(id),
    task_id VARCHAR(255) NOT NULL,
    generation_id VARCHAR(255),
    prompt TEXT,
    media_id VARCHAR(255),
    status VARCHAR(50) DEFAULT 'pending' NOT NULL,
    progress_pct NUMERIC(3,2),
    video_width INTEGER,
    video_height INTEGER,
    video_url TEXT,
    thumbnail_url TEXT,
    provider_post_id VARCHAR(255),
    provider_response JSONB,
    cost_points NUMERIC(10,2) DEFAULT 200.00 NOT NULL,
    wallet_transaction_id INTEGER REFERENCES gm_wallet_transactions(id),
    error_message TEXT,
    retry_count INTEGER DEFAULT 0 NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    model_id INTEGER REFERENCES gm_ai_models(id),
    title VARCHAR(255),
    orientation VARCHAR(20) DEFAULT 'portrait',
    video_seconds VARCHAR(10),
    video_size VARCHAR(20),
    CONSTRAINT valid_video_status CHECK (status IN ('pending', 'queued', 'processing', 'succeeded', 'failed', 'cancelled'))
);

-- =============================================================================
-- AI Publish Module Tables (for Scheduler)
-- =============================================================================

-- AI Publish Plans
CREATE TABLE IF NOT EXISTS gm_aipub_plans (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES gm_users(id),
    group_id INTEGER REFERENCES gm_social_groups(id),
    social_account_id INTEGER REFERENCES gm_social_accounts(id),
    platform_id INTEGER NOT NULL REFERENCES gm_platforms(id),
    content_type VARCHAR(20) NOT NULL,
    ai_task_types TEXT[],
    ai_service_config JSONB,
    ai_input JSONB,
    content JSONB,
    status VARCHAR(20) DEFAULT 'pending' NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMPTZ,
    chat_ai_model_id INTEGER REFERENCES gm_ai_models(id),
    video_ai_model_id INTEGER REFERENCES gm_ai_models(id),
    name VARCHAR(255),
    plan_type VARCHAR(50),
    CONSTRAINT aipub_plans_valid_content_type CHECK (content_type IN ('post', 'video', 'reel', 'story')),
    CONSTRAINT aipub_plans_valid_status CHECK (status IN ('pending', 'ai_processing', 'ready', 'completed', 'failed'))
);

-- AI Publish AI Tasks
CREATE TABLE IF NOT EXISTS gm_aipub_ai_tasks (
    id SERIAL PRIMARY KEY,
    plan_id INTEGER NOT NULL REFERENCES gm_aipub_plans(id) ON DELETE CASCADE,
    task_type VARCHAR(20) NOT NULL,
    external_service VARCHAR(50) NOT NULL,
    external_job_id VARCHAR(200),
    input JSONB NOT NULL,
    result JSONB,
    status VARCHAR(20) DEFAULT 'pending' NOT NULL,
    progress INTEGER DEFAULT 0,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    sequence INTEGER DEFAULT 0,
    CONSTRAINT aipub_ai_tasks_valid_task_type CHECK (task_type IN ('video_gen', 'content_gen', 'image_gen')),
    CONSTRAINT aipub_ai_tasks_valid_status CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    CONSTRAINT aipub_ai_tasks_valid_progress CHECK (progress >= 0 AND progress <= 100)
);

-- AI Publish Tasks
CREATE TABLE IF NOT EXISTS gm_aipub_tasks (
    id SERIAL PRIMARY KEY,
    plan_id INTEGER NOT NULL REFERENCES gm_aipub_plans(id) ON DELETE CASCADE,
    social_account_id INTEGER NOT NULL REFERENCES gm_social_accounts(id),
    content JSONB NOT NULL,
    status VARCHAR(20) DEFAULT 'ready' NOT NULL,
    result_url VARCHAR(500),
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    ai_task_id INTEGER REFERENCES gm_aipub_ai_tasks(id),
    CONSTRAINT aipub_tasks_valid_status CHECK (status IN ('pending', 'video_pending', 'video_processing', 'ready', 'processing', 'completed', 'failed'))
);

-- =============================================================================
-- Indexes
-- =============================================================================
CREATE INDEX IF NOT EXISTS idx_campaigns_status ON gm_campaigns(status);
CREATE INDEX IF NOT EXISTS idx_campaigns_user_status ON gm_campaigns(user_id, status);
CREATE INDEX IF NOT EXISTS idx_campaigns_platform_id ON gm_campaigns(platform_id);
CREATE INDEX IF NOT EXISTS idx_crawler_tasks_campaign_id ON gm_crawler_tasks(campaign_id);
CREATE INDEX IF NOT EXISTS idx_crawler_tasks_status ON gm_crawler_tasks(status);
CREATE INDEX IF NOT EXISTS idx_crawler_tasks_status_updated ON gm_crawler_tasks(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_crawler_tasks_campaign_settled ON gm_crawler_tasks(campaign_id, settled_at);
CREATE INDEX IF NOT EXISTS idx_agent_videos_task_id ON gm_agent_videos(task_id);
CREATE INDEX IF NOT EXISTS idx_agent_comments_status ON gm_agent_comments(status);
CREATE INDEX IF NOT EXISTS idx_agent_comments_video_db_id ON gm_agent_comments(video_db_id);

-- Video generation indexes
CREATE INDEX IF NOT EXISTS idx_video_generation_tasks_status ON gm_video_generation_tasks(status);
CREATE INDEX IF NOT EXISTS idx_video_generation_tasks_user ON gm_video_generation_tasks(user_id);

-- AI Publish indexes
CREATE INDEX IF NOT EXISTS idx_aipub_plans_user ON gm_aipub_plans(user_id);
CREATE INDEX IF NOT EXISTS idx_aipub_plans_status ON gm_aipub_plans(status);
CREATE INDEX IF NOT EXISTS idx_aipub_ai_tasks_plan ON gm_aipub_ai_tasks(plan_id);
CREATE INDEX IF NOT EXISTS idx_aipub_ai_tasks_status ON gm_aipub_ai_tasks(status);
CREATE INDEX IF NOT EXISTS idx_aipub_tasks_plan ON gm_aipub_tasks(plan_id);
CREATE INDEX IF NOT EXISTS idx_aipub_tasks_status ON gm_aipub_tasks(status);

-- =============================================================================
-- Triggers
-- =============================================================================
SELECT diesel_manage_updated_at('gm_users');
SELECT diesel_manage_updated_at('gm_user_wallets');
SELECT diesel_manage_updated_at('gm_platforms');
SELECT diesel_manage_updated_at('gm_regions');
SELECT diesel_manage_updated_at('gm_ai_models');
SELECT diesel_manage_updated_at('gm_pricing_rules');
SELECT diesel_manage_updated_at('gm_social_groups');
SELECT diesel_manage_updated_at('gm_social_accounts');
SELECT diesel_manage_updated_at('gm_campaigns');
SELECT diesel_manage_updated_at('gm_campaign_templates');
SELECT diesel_manage_updated_at('gm_crawler_tasks');
SELECT diesel_manage_updated_at('gm_agent_videos');
SELECT diesel_manage_updated_at('gm_agent_comments');
SELECT diesel_manage_updated_at('gm_wallet_transactions');
SELECT diesel_manage_updated_at('gm_video_generation_tasks');
SELECT diesel_manage_updated_at('gm_aipub_plans');
SELECT diesel_manage_updated_at('gm_aipub_ai_tasks');
SELECT diesel_manage_updated_at('gm_aipub_tasks');
