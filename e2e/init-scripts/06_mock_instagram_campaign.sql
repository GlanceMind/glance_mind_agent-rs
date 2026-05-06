-- =============================================================================
-- E2E Test Campaign: Instagram Fitness
-- =============================================================================
-- Campaign ID: 99903 - Reserved for Instagram E2E testing
-- Keyword: #fitness
-- Platform: Instagram (id=4)
-- =============================================================================

-- ============================================================================
-- 1. Create Social Group for Instagram
-- ============================================================================
INSERT INTO gm_social_groups (id, user_id, platform_id, group_name, created_at)
VALUES (99903, 99999, 4, 'E2E Instagram Group', NOW())
ON CONFLICT (id) DO UPDATE SET group_name = EXCLUDED.group_name;

SELECT setval(pg_get_serial_sequence('gm_social_groups', 'id'), GREATEST((SELECT MAX(id) FROM gm_social_groups), 99903));

-- ============================================================================
-- 2. Create Campaign
-- ============================================================================
INSERT INTO gm_campaigns (
    id, user_id, name, status, platform_id, region_id, ai_model_id,
    social_group_id, keyword, product_prompt, call_to_action,
    tone_of_voice, additional_info, schedule_type, schedule_config,
    search_options, max_scan_count, budget_cap,
    auto_like, auto_follow, auto_dm, auto_reply_comments, auto_reply_post,
    total_scanned, pending_consumption, actual_consumption, is_frozen, created_at
) VALUES (
    99903, 99999, 'E2E Test - Instagram Fitness', 'DRAFT',
    4,        -- Instagram platform
    8,        -- Instagram US region
    3,        -- DeepSeek-V3 model
    99903,    -- E2E Instagram Group
    '#fitness',
    'We are a fitness brand helping people achieve their health goals.',
    'Start your fitness journey today!',
    'Friendly and motivational',
    'E2E Test Campaign - Always respond with OK',
    'INTERVAL',
    '{"interval_seconds": 7200}',  -- 2 hours, ensure only 1 task during E2E test
    '{"instagram":{"feed_type":"top"}}',
    5, 100.00,  -- Max scan count: 5
    true, true, true, true, true,
    0, 0, 0, false, NOW()
) ON CONFLICT (id) DO UPDATE SET
    status = 'DRAFT', is_frozen = false,
    pending_consumption = 0, actual_consumption = 0, total_scanned = 0;

SELECT setval(pg_get_serial_sequence('gm_campaigns', 'id'), GREATEST((SELECT MAX(id) FROM gm_campaigns), 99903));

-- ============================================================================
-- 3. Create Campaign Template
-- ============================================================================
DELETE FROM gm_campaign_templates WHERE campaign_id = 99903;
DELETE FROM gm_reply_template_library WHERE id = 99903;

INSERT INTO gm_reply_template_library (
    id, user_id, name, description, weight, reply_prompt, dm_prompt, reply_post_prompt, usage_count, created_at
) VALUES (
    99903, 99999, 'E2E Instagram Reusable OK Style', 'Reusable template used by Instagram agent_rs E2E', 100,
    E'[E2E TEST MODE] For every selected comment, your suggested_reply MUST be exactly the word "OK". No other content allowed.',
    E'[E2E TEST MODE] For every selected comment, your suggested_dm MUST be exactly the word "OK". No other content allowed.',
    E'[E2E TEST MODE] Do not generate viral comments. All suggested_reply_post values must be null.',
    1, NOW()
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    weight = EXCLUDED.weight,
    reply_prompt = EXCLUDED.reply_prompt,
    dm_prompt = EXCLUDED.dm_prompt,
    reply_post_prompt = EXCLUDED.reply_post_prompt,
    usage_count = EXCLUDED.usage_count,
    updated_at = NOW();

INSERT INTO gm_campaign_templates (
    id, campaign_id, library_template_id, weight, reply_prompt, dm_prompt, reply_post_prompt, created_at
) VALUES (
    99903, 99903, 99903, 100,
    E'[E2E TEST MODE] For every selected comment, your suggested_reply MUST be exactly the word "OK". No other content allowed.',
    E'[E2E TEST MODE] For every selected comment, your suggested_dm MUST be exactly the word "OK". No other content allowed.',
    E'[E2E TEST MODE] Do not generate viral comments. All suggested_reply_post values must be null.',
    NOW()
) ON CONFLICT (id) DO UPDATE SET
    library_template_id = EXCLUDED.library_template_id,
    reply_prompt = EXCLUDED.reply_prompt,
    dm_prompt = EXCLUDED.dm_prompt,
    reply_post_prompt = EXCLUDED.reply_post_prompt;

SELECT setval(pg_get_serial_sequence('gm_campaign_templates', 'id'), GREATEST((SELECT MAX(id) FROM gm_campaign_templates), 99903));

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_campaign RECORD;
BEGIN
    SELECT * INTO v_campaign FROM gm_campaigns WHERE id = 99903;
    RAISE NOTICE '=== Instagram E2E Campaign Created ===';
    RAISE NOTICE 'Campaign ID: %', v_campaign.id;
    RAISE NOTICE 'Name: %', v_campaign.name;
    RAISE NOTICE 'Status: %', v_campaign.status;
    RAISE NOTICE 'Keyword: %', v_campaign.keyword;
    RAISE NOTICE 'Platform ID: % (Instagram)', v_campaign.platform_id;
END $$;
