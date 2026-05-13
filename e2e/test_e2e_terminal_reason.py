"""Fixture-backed terminal reason persistence checks."""

from conftest import E2E_USER_ID


def upsert_terminal_reason_campaign(cur, campaign_id):
    cur.execute(
        """
        INSERT INTO gm_campaigns (
            id, user_id, name, status, platform_id, region_id, ai_model_id,
            product_prompt, schedule_type, budget_cap, pending_consumption,
            actual_consumption, is_frozen, created_at, updated_at
        )
        VALUES (
            %s, %s, 'terminal reason e2e', 'ACTIVE', 2, 1, 1,
            'terminal reason fixture', 'ONCE', 100, 10, 0, true, NOW(), NOW()
        )
        ON CONFLICT (id) DO UPDATE
        SET status = EXCLUDED.status,
            pending_consumption = EXCLUDED.pending_consumption,
            actual_consumption = EXCLUDED.actual_consumption,
            is_frozen = EXCLUDED.is_frozen,
            updated_at = NOW()
        """,
        (campaign_id, E2E_USER_ID),
    )


def test_completed_task_has_terminal_reason_after_fn_complete_task(db_conn):
    campaign_id = 91001
    task_id = 92001
    reason = "COMPLETED: Task completed successfully"

    with db_conn.cursor() as cur:
        upsert_terminal_reason_campaign(cur, campaign_id)
        cur.execute(
            """
            INSERT INTO gm_crawler_tasks (
                id, campaign_id, keywords, max_count, process_count, status,
                search_offset, search_limit, reserved_amount, actual_consumption,
                terminal_reason
            )
            VALUES (%s, %s, ARRAY['terminal-reason'], 10, 1, 'processing', 0, 10, 10, 1, NULL)
            ON CONFLICT (id) DO UPDATE
            SET status = EXCLUDED.status,
                process_count = EXCLUDED.process_count,
                reserved_amount = EXCLUDED.reserved_amount,
                actual_consumption = EXCLUDED.actual_consumption,
                settled_at = NULL,
                terminal_reason = NULL,
                updated_at = NOW()
            """,
            (task_id, campaign_id),
        )
        cur.execute(
            """
            SELECT success, campaign_status
            FROM fn_complete_task(%s, 'completed', %s)
            """,
            (task_id, reason),
        )
        success, _campaign_status = cur.fetchone()
        assert success is True
        cur.execute(
            """
            SELECT status, terminal_reason, settled_at
            FROM gm_crawler_tasks
            WHERE id = %s
            """,
            (task_id,),
        )
        status, terminal_reason, settled_at = cur.fetchone()

    assert status == "completed"
    assert terminal_reason == reason
    assert settled_at is not None


def test_legacy_complete_task_signature_keeps_terminal_reason_null(db_conn):
    campaign_id = 91003
    task_id = 92003

    with db_conn.cursor() as cur:
        upsert_terminal_reason_campaign(cur, campaign_id)
        cur.execute(
            """
            INSERT INTO gm_crawler_tasks (
                id, campaign_id, keywords, max_count, process_count, status,
                search_offset, search_limit, reserved_amount, actual_consumption,
                terminal_reason
            )
            VALUES (%s, %s, ARRAY['terminal-reason-legacy'], 10, 1, 'processing', 0, 10, 10, 1, 'STALE')
            ON CONFLICT (id) DO UPDATE
            SET status = EXCLUDED.status,
                process_count = EXCLUDED.process_count,
                reserved_amount = EXCLUDED.reserved_amount,
                actual_consumption = EXCLUDED.actual_consumption,
                settled_at = NULL,
                terminal_reason = EXCLUDED.terminal_reason,
                updated_at = NOW()
            """,
            (task_id, campaign_id),
        )
        cur.execute("SELECT success FROM fn_complete_task(%s, 'completed')", (task_id,))
        (success,) = cur.fetchone()
        assert success is True
        cur.execute("SELECT status, terminal_reason FROM gm_crawler_tasks WHERE id = %s", (task_id,))
        status, terminal_reason = cur.fetchone()

    assert status == "completed"
    assert terminal_reason is None


def test_blank_terminal_reason_is_stored_as_null(db_conn):
    campaign_id = 91004
    task_id = 92004

    with db_conn.cursor() as cur:
        upsert_terminal_reason_campaign(cur, campaign_id)
        cur.execute(
            """
            INSERT INTO gm_crawler_tasks (
                id, campaign_id, keywords, max_count, process_count, status,
                search_offset, search_limit, reserved_amount, actual_consumption,
                terminal_reason
            )
            VALUES (%s, %s, ARRAY['terminal-reason-blank'], 10, 1, 'processing', 0, 10, 10, 1, 'STALE')
            ON CONFLICT (id) DO UPDATE
            SET status = EXCLUDED.status,
                process_count = EXCLUDED.process_count,
                reserved_amount = EXCLUDED.reserved_amount,
                actual_consumption = EXCLUDED.actual_consumption,
                settled_at = NULL,
                terminal_reason = EXCLUDED.terminal_reason,
                updated_at = NOW()
            """,
            (task_id, campaign_id),
        )
        cur.execute("SELECT success FROM fn_complete_task(%s, 'completed', '   ')", (task_id,))
        (success,) = cur.fetchone()
        assert success is True
        cur.execute("SELECT status, terminal_reason FROM gm_crawler_tasks WHERE id = %s", (task_id,))
        status, terminal_reason = cur.fetchone()

    assert status == "completed"
    assert terminal_reason is None


def test_zombie_cleanup_sets_terminal_reason(db_conn):
    campaign_id = 91002
    processing_task_id = 92002
    pending_task_id = 92005

    with db_conn.cursor() as cur:
        upsert_terminal_reason_campaign(cur, campaign_id)
        cur.execute(
            """
            UPDATE gm_campaigns
            SET pending_consumption = 20,
                actual_consumption = 0,
                updated_at = NOW()
            WHERE id = %s
            """,
            (campaign_id,),
        )
        cur.execute(
            """
            INSERT INTO gm_crawler_tasks (
                id, campaign_id, keywords, max_count, process_count, status,
                search_offset, search_limit, reserved_amount, actual_consumption,
                terminal_reason, created_at, updated_at
            )
            VALUES (
                %s, %s, ARRAY['terminal-reason-zombie-processing'], 10, 1, 'processing',
                0, 10, 10, 1, NULL, NOW() - INTERVAL '48 hours', NOW() - INTERVAL '48 hours'
            )
            ON CONFLICT (id) DO UPDATE
            SET status = EXCLUDED.status,
                process_count = EXCLUDED.process_count,
                reserved_amount = EXCLUDED.reserved_amount,
                actual_consumption = EXCLUDED.actual_consumption,
                settled_at = NULL,
                terminal_reason = NULL,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at
            """,
            (processing_task_id, campaign_id),
        )
        cur.execute(
            """
            INSERT INTO gm_crawler_tasks (
                id, campaign_id, keywords, max_count, process_count, status,
                search_offset, search_limit, reserved_amount, actual_consumption,
                terminal_reason, created_at, updated_at
            )
            VALUES (
                %s, %s, ARRAY['terminal-reason-zombie-pending'], 10, 0, 'pending',
                0, 10, 10, 0, NULL, NOW() - INTERVAL '48 hours', NOW() - INTERVAL '48 hours'
            )
            ON CONFLICT (id) DO UPDATE
            SET status = EXCLUDED.status,
                process_count = EXCLUDED.process_count,
                reserved_amount = EXCLUDED.reserved_amount,
                actual_consumption = EXCLUDED.actual_consumption,
                settled_at = NULL,
                terminal_reason = NULL,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at
            """,
            (pending_task_id, campaign_id),
        )
        cur.execute("SELECT cleaned_count, total_refunded FROM fn_cleanup_zombie_tasks(24, 30)")
        cleaned_count, _total_refunded = cur.fetchone()
        assert cleaned_count >= 2
        cur.execute(
            """
            SELECT id, status, terminal_reason, settled_at
            FROM gm_crawler_tasks
            WHERE id IN (%s, %s)
            ORDER BY id
            """,
            (processing_task_id, pending_task_id),
        )
        rows = {row[0]: row[1:] for row in cur.fetchall()}

    processing_status, processing_reason, processing_settled_at = rows[processing_task_id]
    pending_status, pending_reason, pending_settled_at = rows[pending_task_id]

    assert processing_status == "failed"
    assert processing_reason == "ZOMBIE_CLEANUP: Processing task timed out and was cleaned up"
    assert processing_settled_at is not None

    assert pending_status == "failed"
    assert pending_reason == "ZOMBIE_CLEANUP: Pending task timed out before processing and was cleaned up"
    assert pending_settled_at is not None
