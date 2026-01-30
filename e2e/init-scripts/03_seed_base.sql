-- =============================================================================
-- GlanceMind E2E Test Seed Data
-- =============================================================================
-- Base data: Platforms, Regions, AI Models, Pricing Rules
-- =============================================================================

-- ============================================================================
-- 1. Platforms
-- IMPORTANT: IDs MUST match glance_mind_protocol/proto/common.proto Platform enum
-- ============================================================================
INSERT INTO gm_platforms (id, name, display_name, base_url, page_size, is_active) VALUES
(1, 'reddit', 'Reddit', 'https://www.reddit.com', 25, true),
(2, 'tiktok', 'TikTok', 'https://www.tiktok.com', 20, true),
(3, 'facebook', 'Facebook', 'https://www.facebook.com', 20, true),
(4, 'instagram', 'Instagram', 'https://www.instagram.com', 12, true),
(5, 'twitter', 'Twitter/X', 'https://twitter.com', 20, true),
(6, 'youtube', 'YouTube', 'https://www.youtube.com', 20, true)
ON CONFLICT (id) DO UPDATE SET
    display_name = EXCLUDED.display_name,
    base_url = EXCLUDED.base_url,
    page_size = EXCLUDED.page_size;

SELECT setval(pg_get_serial_sequence('gm_platforms', 'id'), (SELECT MAX(id) FROM gm_platforms));

-- ============================================================================
-- 2. Regions
-- ============================================================================
INSERT INTO gm_regions (id, platform_id, code, name, display_name, is_active) VALUES
-- TikTok regions
(1, 2, 'US', 'United States', 'United States', true),
(2, 2, 'GB', 'United Kingdom', 'United Kingdom', true),
(3, 2, 'JP', 'Japan', 'Japan', true),
(4, 2, 'TW', 'Taiwan', 'Taiwan', true),
(5, 2, 'GLOBAL', 'Global', 'Global', true),
-- Reddit regions
(6, 1, 'GLOBAL', 'Global', 'Global', true),
-- Facebook regions
(7, 3, 'US', 'United States', 'United States', true),
-- Instagram regions
(8, 4, 'US', 'United States', 'United States', true),
-- Twitter regions
(9, 5, 'US', 'United States', 'United States', true),
(10, 5, 'GLOBAL', 'Global', 'Global', true)
ON CONFLICT (platform_id, code) DO UPDATE SET display_name = EXCLUDED.display_name;

SELECT setval(pg_get_serial_sequence('gm_regions', 'id'), (SELECT MAX(id) FROM gm_regions));

-- ============================================================================
-- 3. AI Models
-- ============================================================================
INSERT INTO gm_ai_models (id, name, provider, model_key, model_type, cost_multiplier, is_active) VALUES
(1, 'GPT-4o', 'openai', 'gpt-4o', 'chat', 1.0, true),
(2, 'GPT-4o-mini', 'openai', 'gpt-4o-mini', 'chat', 0.5, true),
(3, 'DeepSeek-V3', 'deepseek', 'deepseek-ai/DeepSeek-V3', 'chat', 0.3, true),
(4, 'Claude-3.5-Sonnet', 'anthropic', 'claude-3-5-sonnet-20241022', 'chat', 1.2, true)
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    model_key = EXCLUDED.model_key;

SELECT setval(pg_get_serial_sequence('gm_ai_models', 'id'), (SELECT MAX(id) FROM gm_ai_models));

-- ============================================================================
-- 4. Pricing Rules
-- ============================================================================
INSERT INTO gm_pricing_rules (id, platform_id, action_type, cost_points) VALUES
-- TikTok pricing (platform_id = 2)
(1, 2, 'SCAN_POST', 0.50),
(2, 2, 'AI_ANALYZE', 1.00),
(3, 2, 'REPLY_COMMENT', 2.00),
-- Reddit pricing (platform_id = 1)
(4, 1, 'SCAN_POST', 0.50),
(5, 1, 'AI_ANALYZE', 1.00),
(6, 1, 'REPLY_COMMENT', 2.00),
-- Instagram pricing (platform_id = 4)
(7, 4, 'SCAN_POST', 0.50),
(8, 4, 'AI_ANALYZE', 1.00),
(9, 4, 'REPLY_COMMENT', 2.00),
-- Twitter pricing (platform_id = 5)
(10, 5, 'SCAN_POST', 0.50),
(11, 5, 'AI_ANALYZE', 1.00),
(12, 5, 'REPLY_COMMENT', 2.00)
ON CONFLICT (action_type, platform_id) DO UPDATE SET cost_points = EXCLUDED.cost_points;

SELECT setval(pg_get_serial_sequence('gm_pricing_rules', 'id'), (SELECT MAX(id) FROM gm_pricing_rules));

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
BEGIN
    RAISE NOTICE '=== E2E Base Data Loaded ===';
    RAISE NOTICE 'Platforms: %', (SELECT COUNT(*) FROM gm_platforms);
    RAISE NOTICE 'Regions: %', (SELECT COUNT(*) FROM gm_regions);
    RAISE NOTICE 'AI Models: %', (SELECT COUNT(*) FROM gm_ai_models);
    RAISE NOTICE 'Pricing Rules: %', (SELECT COUNT(*) FROM gm_pricing_rules);
END $$;
