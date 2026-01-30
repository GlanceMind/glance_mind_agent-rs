-- =============================================================================
-- E2E Test User and Wallet
-- =============================================================================
-- User ID: 99999 - Reserved for E2E testing
-- =============================================================================

-- Password hash for 'E2ETestPassword123!' using bcrypt
-- Generated with: python3 -c "import bcrypt; print(bcrypt.hashpw(b'E2ETestPassword123!', bcrypt.gensalt(12)).decode())"
INSERT INTO gm_users (
    id, 
    email, 
    username, 
    password_hash, 
    full_name, 
    role, 
    status, 
    is_active, 
    invite_code,
    created_at
) VALUES (
    99999,
    'e2e@test.local',
    'e2e_tester',
    '$2b$12$Ikc.R4FMMGahbGhfHlLl4.PciMiV37qXfHpPNCjGGQg/yOEgk7k/e',
    'E2E Test User',
    'user',
    'ACTIVE',
    true,
    'E2E99999',
    NOW()
) ON CONFLICT (id) DO UPDATE SET
    email = EXCLUDED.email,
    username = EXCLUDED.username,
    status = 'ACTIVE';

SELECT setval(pg_get_serial_sequence('gm_users', 'id'), GREATEST((SELECT MAX(id) FROM gm_users), 99999));

-- ============================================================================
-- User Wallet with 10000 points
-- ============================================================================
INSERT INTO gm_user_wallets (
    user_id,
    balance_points,
    frozen_points,
    deposit_cny,
    deposit_usd,
    created_at
) VALUES (
    99999,
    10000.00,
    0.00,
    1000.00,
    150.00,
    NOW()
) ON CONFLICT (user_id) DO UPDATE SET
    balance_points = 10000.00,
    frozen_points = 0.00;

-- ============================================================================
-- Social Group for TikTok (required for campaign)
-- ============================================================================
INSERT INTO gm_social_groups (
    id,
    user_id,
    platform_id,
    group_name,
    created_at
) VALUES (
    99901,
    99999,
    2,  -- TikTok platform
    'E2E TikTok Group',
    NOW()
) ON CONFLICT (id) DO NOTHING;

SELECT setval(pg_get_serial_sequence('gm_social_groups', 'id'), GREATEST((SELECT MAX(id) FROM gm_social_groups), 99901));

-- ============================================================================
-- Social Account (optional, for complete testing)
-- ============================================================================
INSERT INTO gm_social_accounts (
    id,
    user_id,
    platform_id,
    username,
    group_id,
    status,
    health_score,
    cookie,
    daily_max_replies,
    device_id,
    profile_name,
    created_at
) VALUES (
    99901,
    99999,
    2,  -- TikTok platform
    'e2e_tiktok_account',
    99901,
    'ACTIVE',
    100,
    '{}',
    50,
    'e2e_device_001',
    'E2E TikTok Profile',
    NOW()
) ON CONFLICT (id) DO NOTHING;

SELECT setval(pg_get_serial_sequence('gm_social_accounts', 'id'), GREATEST((SELECT MAX(id) FROM gm_social_accounts), 99901));

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_wallet RECORD;
BEGIN
    SELECT * INTO v_wallet FROM gm_user_wallets WHERE user_id = 99999;
    
    RAISE NOTICE '=== E2E User Created ===';
    RAISE NOTICE 'User ID: 99999';
    RAISE NOTICE 'Email: e2e@test.local';
    RAISE NOTICE 'Wallet Balance: % points', v_wallet.balance_points;
    RAISE NOTICE 'Wallet Frozen: % points', v_wallet.frozen_points;
    RAISE NOTICE 'Social Group ID: 99901';
END $$;
