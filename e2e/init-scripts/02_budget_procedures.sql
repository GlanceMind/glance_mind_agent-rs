-- =============================================================================
-- Budget Management Stored Procedures
-- =============================================================================
-- Required by Scheduler and Agent for task/budget management
-- =============================================================================

-- Helper: Get unit price for platform
CREATE OR REPLACE FUNCTION fn_get_unit_price(p_platform_id INT)
RETURNS NUMERIC AS $$
DECLARE
    v_unit_price NUMERIC;
BEGIN
    SELECT COALESCE(SUM(cost_points), 0)
    INTO v_unit_price
    FROM gm_pricing_rules
    WHERE platform_id = p_platform_id
      AND action_type IN ('SCAN_POST', 'AI_ANALYZE');
    
    IF v_unit_price = 0 THEN
        v_unit_price := 1.0;
    END IF;
    
    RETURN v_unit_price;
END;
$$ LANGUAGE plpgsql;

-- Activate campaign and freeze budget
CREATE OR REPLACE FUNCTION fn_activate_campaign(p_campaign_id INT)
RETURNS TABLE(success BOOLEAN, message TEXT) AS $$
DECLARE
    v_campaign RECORD;
    v_wallet RECORD;
    v_budget_cap NUMERIC;
BEGIN
    SELECT * INTO v_campaign FROM gm_campaigns WHERE id = p_campaign_id FOR UPDATE;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, 'Campaign not found'::TEXT;
        RETURN;
    END IF;
    
    IF v_campaign.is_frozen THEN
        RETURN QUERY SELECT TRUE, 'Campaign already activated'::TEXT;
        RETURN;
    END IF;
    
    v_budget_cap := COALESCE(v_campaign.budget_cap, 0);
    IF v_budget_cap <= 0 THEN
        UPDATE gm_campaigns SET status = 'ACTIVE', is_frozen = TRUE, updated_at = NOW()
        WHERE id = p_campaign_id;
        RETURN QUERY SELECT TRUE, 'Campaign activated (no budget)'::TEXT;
        RETURN;
    END IF;
    
    SELECT * INTO v_wallet FROM gm_user_wallets WHERE user_id = v_campaign.user_id FOR UPDATE;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, 'Wallet not found'::TEXT;
        RETURN;
    END IF;
    
    IF v_wallet.balance_points < v_budget_cap THEN
        RETURN QUERY SELECT FALSE, 
            format('Insufficient balance. Required: %s, Available: %s', v_budget_cap, v_wallet.balance_points)::TEXT;
        RETURN;
    END IF;
    
    UPDATE gm_user_wallets
    SET balance_points = balance_points - v_budget_cap,
        frozen_points = frozen_points + v_budget_cap,
        updated_at = NOW()
    WHERE user_id = v_campaign.user_id;
    
    UPDATE gm_campaigns SET status = 'ACTIVE', is_frozen = TRUE, updated_at = NOW()
    WHERE id = p_campaign_id;
    
    INSERT INTO gm_wallet_transactions (user_id, amount, type, reference_id, description, created_at)
    VALUES (v_campaign.user_id, -v_budget_cap, 'FREEZE', p_campaign_id,
            format('Campaign activated: %s', v_campaign.name), NOW());
    
    RETURN QUERY SELECT TRUE, 'Campaign activated successfully'::TEXT;
END;
$$ LANGUAGE plpgsql;

-- Reserve task budget
CREATE OR REPLACE FUNCTION fn_reserve_task_budget(
    p_campaign_id INT,
    p_task_cost NUMERIC,
    p_search_limit INT
)
RETURNS TABLE(success BOOLEAN, message TEXT, new_pending NUMERIC, reserved_amount NUMERIC) AS $$
DECLARE
    v_campaign RECORD;
BEGIN
    SELECT * INTO v_campaign FROM gm_campaigns WHERE id = p_campaign_id FOR UPDATE;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, 'Campaign not found'::TEXT, 0::NUMERIC, 0::NUMERIC;
        RETURN;
    END IF;
    
    IF v_campaign.status NOT IN ('ACTIVE') THEN
        RETURN QUERY SELECT FALSE, 
            format('Campaign is not active. Status: %s', v_campaign.status)::TEXT,
            v_campaign.pending_consumption, 0::NUMERIC;
        RETURN;
    END IF;
    
    IF v_campaign.pending_consumption + v_campaign.actual_consumption + p_task_cost > v_campaign.budget_cap THEN
        UPDATE gm_campaigns SET status = 'COMPLETED', completed_reason = 'BUDGET_EXHAUSTED', updated_at = NOW()
        WHERE id = p_campaign_id;
        RETURN QUERY SELECT FALSE, 'BUDGET_EXHAUSTED'::TEXT, v_campaign.pending_consumption, 0::NUMERIC;
        RETURN;
    END IF;
    
    UPDATE gm_campaigns
    SET pending_consumption = pending_consumption + p_task_cost,
        total_scanned = total_scanned + p_search_limit,
        updated_at = NOW()
    WHERE id = p_campaign_id;
    
    RETURN QUERY SELECT TRUE, 'Task budget reserved'::TEXT,
        v_campaign.pending_consumption + p_task_cost, p_task_cost;
END;
$$ LANGUAGE plpgsql;

-- Update task progress
CREATE OR REPLACE FUNCTION fn_update_task_progress(p_task_id INT, p_increment INT DEFAULT 1)
RETURNS TABLE(success BOOLEAN, should_stop BOOLEAN, new_process_count INT, new_actual_consumption NUMERIC) AS $$
DECLARE
    v_task RECORD;
    v_unit_price NUMERIC;
    v_consumption_increment NUMERIC;
BEGIN
    SELECT t.*, c.platform_id, c.status as campaign_status
    INTO v_task
    FROM gm_crawler_tasks t
    JOIN gm_campaigns c ON t.campaign_id = c.id
    WHERE t.id = p_task_id
    FOR UPDATE OF t;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, TRUE, 0, 0::NUMERIC;
        RETURN;
    END IF;
    
    -- Accept 'pending', 'processing', or 'running' status
    -- 'running' is used by Rust agent, 'processing' by Python agent
    IF v_task.status NOT IN ('pending', 'processing', 'running') THEN
        RETURN QUERY SELECT FALSE, TRUE, v_task.process_count, v_task.actual_consumption;
        RETURN;
    END IF;
    
    v_unit_price := fn_get_unit_price(v_task.platform_id);
    v_consumption_increment := p_increment * v_unit_price;
    
    UPDATE gm_crawler_tasks
    SET process_count = process_count + p_increment,
        actual_consumption = actual_consumption + v_consumption_increment,
        status = 'processing',
        updated_at = NOW()
    WHERE id = p_task_id;
    
    UPDATE gm_campaigns
    SET actual_consumption = actual_consumption + v_consumption_increment, updated_at = NOW()
    WHERE id = v_task.campaign_id;
    
    RETURN QUERY SELECT TRUE, v_task.campaign_status = 'STOPPING',
        v_task.process_count + p_increment, v_task.actual_consumption + v_consumption_increment;
END;
$$ LANGUAGE plpgsql;

-- Settle task consumption
CREATE OR REPLACE FUNCTION fn_settle_task_consumption(p_task_id INT)
RETURNS TABLE(success BOOLEAN, actual_cost NUMERIC) AS $$
DECLARE
    v_task RECORD;
BEGIN
    SELECT * INTO v_task FROM gm_crawler_tasks WHERE id = p_task_id FOR UPDATE;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, 0::NUMERIC;
        RETURN;
    END IF;
    
    IF v_task.settled_at IS NOT NULL THEN
        RETURN QUERY SELECT TRUE, v_task.actual_consumption;
        RETURN;
    END IF;
    
    UPDATE gm_campaigns
    SET pending_consumption = pending_consumption - v_task.reserved_amount, updated_at = NOW()
    WHERE id = v_task.campaign_id;
    
    UPDATE gm_crawler_tasks SET settled_at = NOW(), updated_at = NOW() WHERE id = p_task_id;
    
    INSERT INTO gm_wallet_transactions (user_id, amount, type, reference_id, description, created_at)
    SELECT c.user_id, -v_task.actual_consumption, 'SETTLE', p_task_id,
           format('Task %s settled', p_task_id), NOW()
    FROM gm_campaigns c WHERE c.id = v_task.campaign_id;
    
    RETURN QUERY SELECT TRUE, v_task.actual_consumption;
END;
$$ LANGUAGE plpgsql;

-- Complete task
CREATE OR REPLACE FUNCTION fn_complete_task(
    p_task_id INT,
    p_final_status TEXT DEFAULT 'completed',
    p_terminal_reason TEXT DEFAULT NULL
)
RETURNS TABLE(success BOOLEAN, campaign_status TEXT) AS $$
DECLARE
    v_task RECORD;
    v_active_tasks INT;
BEGIN
    SELECT t.*, c.status as current_campaign_status
    INTO v_task
    FROM gm_crawler_tasks t
    JOIN gm_campaigns c ON t.campaign_id = c.id
    WHERE t.id = p_task_id
    FOR UPDATE OF t;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, 'NOT_FOUND'::TEXT;
        RETURN;
    END IF;
    
    UPDATE gm_crawler_tasks
    SET status = p_final_status,
        terminal_reason = NULLIF(BTRIM(p_terminal_reason), ''),
        updated_at = NOW()
    WHERE id = p_task_id;
    
    PERFORM fn_settle_task_consumption(p_task_id);
    
    IF v_task.current_campaign_status = 'STOPPING' THEN
        SELECT COUNT(*) INTO v_active_tasks
        FROM gm_crawler_tasks
        WHERE campaign_id = v_task.campaign_id
          AND status NOT IN ('completed', 'failed', 'cancelled');
        
        IF v_active_tasks = 0 THEN
            PERFORM fn_finalize_campaign(v_task.campaign_id);
            RETURN QUERY SELECT TRUE, 'STOPPED'::TEXT;
            RETURN;
        END IF;
    END IF;
    
    RETURN QUERY SELECT TRUE, v_task.current_campaign_status::TEXT;
END;
$$ LANGUAGE plpgsql;

-- Finalize campaign
CREATE OR REPLACE FUNCTION fn_finalize_campaign(p_campaign_id INT)
RETURNS TABLE(success BOOLEAN, refunded_amount NUMERIC) AS $$
DECLARE
    v_campaign RECORD;
    v_refund NUMERIC;
BEGIN
    SELECT * INTO v_campaign FROM gm_campaigns WHERE id = p_campaign_id FOR UPDATE;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, 0::NUMERIC;
        RETURN;
    END IF;
    
    IF v_campaign.status IN ('STOPPED', 'COMPLETED') AND v_campaign.completed_reason IS NOT NULL THEN
        RETURN QUERY SELECT TRUE, 0::NUMERIC;
        RETURN;
    END IF;
    
    v_refund := v_campaign.budget_cap - v_campaign.actual_consumption;
    
    UPDATE gm_user_wallets
    SET frozen_points = frozen_points - v_campaign.budget_cap,
        balance_points = balance_points + v_refund,
        updated_at = NOW()
    WHERE user_id = v_campaign.user_id;
    
    UPDATE gm_campaigns
    SET status = CASE WHEN status = 'STOPPING' THEN 'STOPPED' ELSE 'COMPLETED' END,
        completed_reason = COALESCE(completed_reason, 'FINALIZED'),
        pending_consumption = 0,
        updated_at = NOW()
    WHERE id = p_campaign_id;
    
    INSERT INTO gm_wallet_transactions (user_id, amount, type, reference_id, description, created_at)
    VALUES (v_campaign.user_id, v_campaign.budget_cap, 'UNFREEZE', p_campaign_id,
            format('Campaign %s finalized', p_campaign_id), NOW());
    
    IF v_refund > 0 THEN
        INSERT INTO gm_wallet_transactions (user_id, amount, type, reference_id, description, created_at)
        VALUES (v_campaign.user_id, v_refund, 'REFUND', p_campaign_id,
                format('Campaign %s refund', p_campaign_id), NOW());
    END IF;
    
    RETURN QUERY SELECT TRUE, v_refund;
END;
$$ LANGUAGE plpgsql;

-- Cleanup zombie tasks
CREATE OR REPLACE FUNCTION fn_cleanup_zombie_tasks(
    p_timeout_hours INT DEFAULT 24,
    p_pending_timeout_minutes INT DEFAULT 30
)
RETURNS TABLE(cleaned_count INT, total_refunded NUMERIC) AS $$
DECLARE
    v_zombie_task RECORD;
    v_cleaned INT := 0;
BEGIN
    FOR v_zombie_task IN
        SELECT id FROM gm_crawler_tasks
        WHERE status = 'processing'
          AND updated_at < NOW() - (p_timeout_hours || ' hours')::INTERVAL
        FOR UPDATE SKIP LOCKED
    LOOP
        PERFORM fn_complete_task(
            v_zombie_task.id,
            'failed',
            'ZOMBIE_CLEANUP: Processing task timed out and was cleaned up'
        );
        v_cleaned := v_cleaned + 1;
    END LOOP;

    FOR v_zombie_task IN
        SELECT id FROM gm_crawler_tasks
        WHERE status = 'pending'
          AND created_at < NOW() - (p_pending_timeout_minutes || ' minutes')::INTERVAL
        FOR UPDATE SKIP LOCKED
    LOOP
        PERFORM fn_complete_task(
            v_zombie_task.id,
            'failed',
            'ZOMBIE_CLEANUP: Pending task timed out before processing and was cleaned up'
        );
        v_cleaned := v_cleaned + 1;
    END LOOP;
    
    RETURN QUERY SELECT v_cleaned, 0::NUMERIC;
END;
$$ LANGUAGE plpgsql;
