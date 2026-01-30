-- =============================================================================
-- E2E Test Campaign: 中国旅游 (China Travel) on TikTok
-- =============================================================================
-- Campaign ID: 99901 - Reserved for E2E testing
-- Keyword: 中国旅游
-- Platform: TikTok (id=2)
-- Max Scan: 1 video
-- AI Prompt: Always reply "OK"
-- =============================================================================

-- ============================================================================
-- 1. Create Campaign
-- Status: DRAFT (will be activated by test script)
-- ============================================================================
INSERT INTO gm_campaigns (
    id,
    user_id,
    name,
    status,
    platform_id,
    region_id,
    ai_model_id,
    social_group_id,
    keyword,
    product_prompt,
    call_to_action,
    tone_of_voice,
    additional_info,
    schedule_type,
    schedule_config,
    search_options,
    max_scan_count,
    budget_cap,
    auto_like,
    auto_follow,
    auto_dm,
    auto_reply_comments,
    auto_reply_post,
    total_scanned,
    pending_consumption,
    actual_consumption,
    is_frozen,
    created_at
) VALUES (
    99901,
    99999,
    'E2E Test - 中国旅游',
    'DRAFT',  -- Will be activated by test script
    2,        -- TikTok platform
    5,        -- GLOBAL region
    3,        -- DeepSeek-V3 model
    99901,    -- E2E TikTok Group
    '中国旅游',
    'We are a travel agency specializing in China tours. Help travelers discover the best destinations in China.',
    'Book your China trip now!',
    'Friendly and helpful',
    'E2E Test Campaign - Always respond with OK',
    'INTERVAL',
    '{"interval_seconds": 3600}',
    '{}',
    1,        -- Max scan count (only scan 1 video for E2E test)
    100.00,   -- Budget cap (enough for testing)
    true,
    true,
    true,
    true,
    true,
    0,
    0,
    0,
    false,
    NOW()
) ON CONFLICT (id) DO UPDATE SET
    status = 'DRAFT',
    is_frozen = false,
    pending_consumption = 0,
    actual_consumption = 0,
    total_scanned = 0;

SELECT setval(pg_get_serial_sequence('gm_campaigns', 'id'), GREATEST((SELECT MAX(id) FROM gm_campaigns), 99901));

-- ============================================================================
-- 2. Create Campaign Template with "Always OK" Prompt
-- ============================================================================
DELETE FROM gm_campaign_templates WHERE campaign_id = 99901;

INSERT INTO gm_campaign_templates (
    id,
    campaign_id,
    weight,
    reply_prompt,
    dm_prompt,
    reply_post_prompt,
    created_at
) VALUES (
    99901,
    99901,
    100,
    -- Reply strategy prompt (just the strategy description, not the output format)
    -- This gets embedded into the system template as: "Reply Strategy: {reply_strategy_prompt}"
    E'[E2E TEST MODE] For every selected comment, your suggested_reply MUST be exactly the word "OK". No other content allowed.',
    
    -- DM strategy prompt (just the strategy description)
    -- This gets embedded into the system template as: "DM Strategy: {dm_strategy_prompt}"
    E'[E2E TEST MODE] For every selected comment, your suggested_dm MUST be exactly the word "OK". No other content allowed.',
    
    -- Viral comment strategy prompt (just the strategy description)
    -- This gets embedded into the system template as: "Viral Comment Strategy: {reply_post_prompt}"
    E'[E2E TEST MODE] Do not generate viral comments. All suggested_reply_post values must be null.',
    
    NOW()
) ON CONFLICT (id) DO UPDATE SET
    reply_prompt = EXCLUDED.reply_prompt,
    dm_prompt = EXCLUDED.dm_prompt,
    reply_post_prompt = EXCLUDED.reply_post_prompt;

SELECT setval(pg_get_serial_sequence('gm_campaign_templates', 'id'), GREATEST((SELECT MAX(id) FROM gm_campaign_templates), 99901));

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_campaign RECORD;
    v_template RECORD;
    v_wallet RECORD;
BEGIN
    SELECT * INTO v_campaign FROM gm_campaigns WHERE id = 99901;
    SELECT * INTO v_template FROM gm_campaign_templates WHERE campaign_id = 99901;
    SELECT * INTO v_wallet FROM gm_user_wallets WHERE user_id = 99999;
    
    RAISE NOTICE '';
    RAISE NOTICE '=== E2E Campaign Created ===';
    RAISE NOTICE 'Campaign ID: %', v_campaign.id;
    RAISE NOTICE 'Name: %', v_campaign.name;
    RAISE NOTICE 'Status: %', v_campaign.status;
    RAISE NOTICE 'Keyword: %', v_campaign.keyword;
    RAISE NOTICE 'Platform ID: % (TikTok)', v_campaign.platform_id;
    RAISE NOTICE 'Max Scan: % video(s)', v_campaign.max_scan_count;
    RAISE NOTICE 'Budget Cap: % points', v_campaign.budget_cap;
    RAISE NOTICE '';
    RAISE NOTICE 'User Wallet Balance: % points', v_wallet.balance_points;
    RAISE NOTICE '';
    RAISE NOTICE 'Template ID: %', v_template.id;
    RAISE NOTICE 'Reply Prompt Preview: %.50s...', v_template.reply_prompt;
    RAISE NOTICE '';
    RAISE NOTICE '=== Ready for E2E Test ===';
    RAISE NOTICE 'Next Step: Activate campaign with fn_activate_campaign(99901)';
END $$;
