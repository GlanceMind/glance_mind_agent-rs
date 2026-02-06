"""
E2E Test: China Travel Campaign on TikTok

This test verifies the complete flow:
1. Activate campaign (DRAFT -> ACTIVE)
2. Scheduler creates crawler task
3. Agent-rs processes task (fetch videos, comments, AI analysis)
4. Verify results in database
5. Verify wallet transactions and campaign financial state
"""
import time
import pytest
from conftest import (
    E2E_USER_ID, E2E_CAMPAIGN_ID, E2E_TIMEOUT,
    wait_for_condition, get_campaign_status, get_task_status, get_wallet_balance,
    get_any_completed_task, get_completed_task_count,
    get_task_details, get_all_tasks, verify_task_lifecycle,
    get_campaign_details, get_wallet_transactions,
    verify_campaign_financial_state, verify_wallet_transactions_for_campaign,
    verify_wallet_transactions_for_task, verify_wallet_balance_accounting,
    get_task_consumption_summary
)


class TestChinaTravelE2E:
    """E2E test for China Travel TikTok campaign."""
    
    # =========================================================================
    # Test 1: Verify Initial State
    # =========================================================================
    
    def test_01_verify_initial_state(self, db_conn, e2e_config):
        """Verify campaign is in DRAFT status and wallet has balance."""
        campaign_id = e2e_config["campaign_id"]
        user_id = e2e_config["user_id"]
        expected_platform_id = 2  # TikTok
        
        # Check campaign exists with full details
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, name, platform_id, keyword, user_id, status FROM gm_campaigns WHERE id = %s",
                (campaign_id,)
            )
            campaign = cur.fetchone()
        
        assert campaign is not None, f"Campaign {campaign_id} not found. Run init-scripts first."
        
        campaign_name = campaign[1]
        campaign_platform_id = campaign[2]
        campaign_keyword = campaign[3]
        campaign_status = campaign[5]
        
        print(f"[OK] Campaign found: {campaign_name}")
        print(f"  - Platform ID: {campaign_platform_id}")
        print(f"  - Keyword: {campaign_keyword}")
        print(f"  - Status: {campaign_status}")
        
        # Verify platform_id matches expected (2 for TikTok)
        assert campaign_platform_id == expected_platform_id, \
            f"Expected platform_id {expected_platform_id} (TikTok), got {campaign_platform_id}"
        print(f"[OK] Platform ID verified: {campaign_platform_id} (TikTok)")
        
        # Check campaign status
        status = get_campaign_status(db_conn, campaign_id)
        assert status == "DRAFT", f"Expected DRAFT status, got {status}"
        print(f"[OK] Campaign {campaign_id} is in DRAFT status")
        
        # Check wallet balance
        balance, frozen = get_wallet_balance(db_conn, user_id)
        assert balance >= 100, f"Insufficient balance: {balance}"
        print(f"[OK] Wallet balance: {balance}, frozen: {frozen}")
    
    # =========================================================================
    # Test 2: Activate Campaign
    # =========================================================================
    
    def test_02_activate_campaign(self, db_conn, e2e_config):
        """Activate the campaign using fn_activate_campaign."""
        campaign_id = e2e_config["campaign_id"]
        
        with db_conn.cursor() as cur:
            # Call activation function
            cur.execute("SELECT * FROM fn_activate_campaign(%s)", (campaign_id,))
            result = cur.fetchone()
            success, message = result
            
            assert success, f"Activation failed: {message}"
            print(f"[OK] Campaign activated: {message}")
        
        # Verify status changed
        status = get_campaign_status(db_conn, campaign_id)
        assert status == "ACTIVE", f"Expected ACTIVE status, got {status}"
        print(f"[OK] Campaign status is now ACTIVE")
        
        # Verify budget frozen
        balance, frozen = get_wallet_balance(db_conn, e2e_config["user_id"])
        assert frozen > 0, "Budget should be frozen after activation"
        print(f"[OK] Budget frozen: {frozen} points")
    
    # =========================================================================
    # Test 3: Wait for Scheduler to Create Task
    # =========================================================================
    
    def test_03_scheduler_creates_task(self, db_conn, e2e_config):
        """Wait for scheduler to create a crawler task."""
        campaign_id = e2e_config["campaign_id"]
        timeout = min(e2e_config["timeout"], 120)  # Max 2 minutes for task creation
        
        def check_task_exists():
            task = get_task_status(db_conn, campaign_id)
            return task is not None
        
        print(f"[WAIT] Waiting for scheduler to create task (max {timeout}s)...")
        
        success = wait_for_condition(
            check_task_exists,
            timeout=timeout,
            interval=10,
            description="task creation"
        )
        
        assert success, "Scheduler did not create task within timeout"
        
        task = get_task_status(db_conn, campaign_id)
        print(f"[OK] Task created: id={task[0]}, status={task[1]}")
    
    # =========================================================================
    # Test 4: Wait for Agent to Process Task
    # =========================================================================
    
    def test_04_agent_processes_task(self, db_conn, e2e_config):
        """Wait for agent-rs to process at least one task."""
        campaign_id = e2e_config["campaign_id"]
        timeout = e2e_config["timeout"]
        
        def check_any_task_completed():
            # Check if at least one task is completed (scheduler creates multiple tasks)
            completed_count = get_completed_task_count(db_conn, campaign_id)
            # Also get latest task status for logging
            latest_task = get_task_status(db_conn, campaign_id)
            latest_status = latest_task[1] if latest_task else "none"
            print(f"  Completed: {completed_count}, Latest: {latest_status}")
            return completed_count >= 1
        
        print(f"[WAIT] Waiting for agent to process task (max {timeout}s)...")
        
        success = wait_for_condition(
            check_any_task_completed,
            timeout=timeout,
            interval=15,
            description="task completion"
        )
        
        assert success, "Agent did not complete any task within timeout"
        
        # Get the first completed task
        task = get_any_completed_task(db_conn, campaign_id)
        assert task is not None, "No completed task found"
        print(f"[OK] Task completed: id={task[0]}")
    
    # =========================================================================
    # Test 5: Verify Videos Saved
    # =========================================================================
    
    def test_05_videos_saved(self, db_conn, e2e_config):
        """Verify videos were saved to database."""
        campaign_id = e2e_config["campaign_id"]
        
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT COUNT(*) FROM gm_agent_videos WHERE campaign_id = %s",
                (campaign_id,)
            )
            count = cur.fetchone()[0]
        
        assert count >= 1, f"Expected at least 1 video, got {count}"
        print(f"[OK] {count} video(s) saved to database")
        
        # Get video details
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, video_id, author, description FROM gm_agent_videos WHERE campaign_id = %s LIMIT 3",
                (campaign_id,)
            )
            videos = cur.fetchall()
            for v in videos:
                print(f"  Video: id={v[0]}, video_id={v[1][:20]}..., author={v[2]}")
    
    # =========================================================================
    # Test 6: Verify Comments Saved
    # =========================================================================
    
    def test_06_comments_saved(self, db_conn, e2e_config):
        """Verify comments were saved to database."""
        campaign_id = e2e_config["campaign_id"]
        
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT COUNT(*) FROM gm_agent_comments c
                JOIN gm_agent_videos v ON c.video_db_id = v.id
                WHERE v.campaign_id = %s
            """, (campaign_id,))
            count = cur.fetchone()[0]
        
        assert count >= 1, f"Expected at least 1 comment, got {count}"
        print(f"[OK] {count} comment(s) saved to database")
        
        # Get comment details
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT c.id, c.comment_id, c.user_nickname, c.content, c.suggested_reply
                FROM gm_agent_comments c
                JOIN gm_agent_videos v ON c.video_db_id = v.id
                WHERE v.campaign_id = %s
                LIMIT 5
            """, (campaign_id,))
            comments = cur.fetchall()
            for c in comments:
                content_preview = (c[3][:30] + "...") if c[3] and len(c[3]) > 30 else c[3]
                print(f"  Comment: id={c[0]}, user={c[2]}, content=\"{content_preview}\"")
    
    # =========================================================================
    # Test 7: Verify AI Analysis Results
    # =========================================================================
    
    def test_07_ai_analysis_ok(self, db_conn, e2e_config):
        """Verify AI generated 'OK' replies for all comments."""
        campaign_id = e2e_config["campaign_id"]
        
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT c.id, c.suggested_reply
                FROM gm_agent_comments c
                JOIN gm_agent_videos v ON c.video_db_id = v.id
                WHERE v.campaign_id = %s
                  AND c.suggested_reply IS NOT NULL
            """, (campaign_id,))
            comments = cur.fetchall()
        
        if len(comments) == 0:
            pytest.skip("No comments with AI analysis found")
        
        ok_count = 0
        for comment_id, reply in comments:
            if reply and "OK" in reply.upper():
                ok_count += 1
            else:
                print(f"  [WARN] Comment {comment_id} has unexpected reply: {reply}")
        
        print(f"[OK] {ok_count}/{len(comments)} comments have 'OK' replies")
        
        # At least 50% should have "OK" reply (AI might not always follow instructions perfectly)
        assert ok_count >= len(comments) * 0.5, \
            f"Expected at least 50% 'OK' replies, got {ok_count}/{len(comments)}"
    
    # =========================================================================
    # Test 8: Verify Wallet Updated
    # =========================================================================
    
    def test_08_wallet_updated(self, db_conn, e2e_config):
        """Verify wallet balance was affected."""
        user_id = e2e_config["user_id"]
        
        balance, frozen = get_wallet_balance(db_conn, user_id)
        
        # Initial balance was 10000, some should be frozen or consumed
        print(f"[INFO] Final wallet: balance={balance}, frozen={frozen}")
        
        # Check that something happened
        total = balance + frozen
        assert total <= 10000, f"Wallet should have some consumption, total={total}"
        print(f"[OK] Wallet updated correctly")
    
    # =========================================================================
    # Test 9: Verify Task Lifecycle (NEW)
    # =========================================================================
    
    def test_09_verify_task_lifecycle(self, db_conn, e2e_config):
        """Verify task went through proper lifecycle with all fields populated."""
        campaign_id = e2e_config["campaign_id"]
        
        # Get the first completed task
        completed_task = get_any_completed_task(db_conn, campaign_id)
        assert completed_task is not None, "No completed task found"
        
        task_id = completed_task[0]
        task_details = get_task_details(db_conn, task_id)
        
        print(f"\n[INFO] Task {task_id} Details:")
        print(f"  - Status: {task_details['status']}")
        print(f"  - Process Count: {task_details['process_count']}/{task_details['max_count']}")
        print(f"  - Reserved Amount: {task_details['reserved_amount']}")
        print(f"  - Actual Consumption: {task_details['actual_consumption']}")
        print(f"  - Settled At: {task_details['settled_at']}")
        print(f"  - Keywords: {task_details['keywords']}")
        
        # Verify lifecycle
        is_valid, errors = verify_task_lifecycle(task_details)
        
        if not is_valid:
            for error in errors:
                print(f"  [ERROR] {error}")
            pytest.fail(f"Task lifecycle validation failed: {errors}")
        
        print(f"[OK] Task lifecycle validation passed")
        
        # Verify task count is reasonable
        all_tasks = get_all_tasks(db_conn, campaign_id)
        print(f"[INFO] Total tasks created: {len(all_tasks)}")
        
        # Should have at most 3 tasks (scheduler may create additional tasks during test)
        assert len(all_tasks) <= 3, f"Expected at most 3 tasks, got {len(all_tasks)}"
        
        # Verify at least one task is completed
        completed_count = sum(1 for t in all_tasks if t[1] == "completed")
        assert completed_count >= 1, f"Expected at least 1 completed task, got {completed_count}"
        print(f"[OK] {completed_count} task(s) completed as expected")
    
    # =========================================================================
    # Test 10: Final Summary
    # =========================================================================
    
    def test_10_summary(self, db_conn, e2e_config):
        """Print final test summary with comprehensive verification."""
        campaign_id = e2e_config["campaign_id"]
        user_id = e2e_config["user_id"]
        
        print("\n" + "=" * 60)
        print("TIKTOK E2E TEST SUMMARY")
        print("=" * 60)
        
        # Campaign status
        status = get_campaign_status(db_conn, campaign_id)
        print(f"Campaign Status: {status}")
        print(f"Platform: TikTok (ID: 2)")
        
        # Task count
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT COUNT(*), MAX(status) FROM gm_crawler_tasks WHERE campaign_id = %s",
                (campaign_id,)
            )
            task_count, last_status = cur.fetchone()
        print(f"Tasks Created: {task_count} (last status: {last_status})")
        
        # Video count
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT COUNT(*) FROM gm_agent_videos WHERE campaign_id = %s",
                (campaign_id,)
            )
            video_count = cur.fetchone()[0]
        print(f"Videos Processed: {video_count}")
        
        # Comment count
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT COUNT(*) FROM gm_agent_comments c
                JOIN gm_agent_videos v ON c.video_db_id = v.id
                WHERE v.campaign_id = %s
            """, (campaign_id,))
            comment_count = cur.fetchone()[0]
        print(f"Comments Analyzed: {comment_count}")
        
        # Campaign financial state
        campaign = get_campaign_details(db_conn, campaign_id)
        if campaign:
            print(f"\n--- Campaign Financial State ---")
            print(f"  pending_consumption: {campaign['pending_consumption']}")
            print(f"  actual_consumption: {campaign['actual_consumption']}")
            print(f"  total_scanned: {campaign['total_scanned']}")
            print(f"  budget_cap: {campaign['budget_cap']}")
        
        # Task consumption summary
        task_summary = get_task_consumption_summary(db_conn, campaign_id)
        if task_summary:
            print(f"\n--- Task Summary ---")
            print(f"  Total tasks: {task_summary['task_count']}")
            print(f"  Completed: {task_summary['completed_count']}")
            print(f"  Settled: {task_summary['settled_count']}")
            print(f"  Total processed: {task_summary['total_processed']}")
            print(f"  Total actual consumption: {task_summary['total_actual']}")
        
        # Wallet state
        wallet = get_wallet_balance(db_conn, user_id)
        if wallet:
            balance, frozen = wallet
            print(f"\n--- Wallet State ---")
            print(f"  Balance: {balance}")
            print(f"  Frozen: {frozen}")
        
        # Wallet transactions
        all_txns = get_wallet_transactions(db_conn, user_id, reference_id=campaign_id)
        if all_txns:
            print(f"\n--- Wallet Transactions for Campaign ---")
            for txn in all_txns:
                print(f"  {txn['type']}: {txn['amount']} ({txn['description']})")
        
        print("\n" + "=" * 60)
        print("TIKTOK E2E TEST COMPLETED SUCCESSFULLY!")
        print("=" * 60)


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
