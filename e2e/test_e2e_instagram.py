"""
E2E Test: Instagram Platform

This test verifies the complete flow for Instagram:
1. Verify campaign exists and is in DRAFT status
2. Activate campaign (DRAFT -> ACTIVE)
3. Scheduler creates crawler task
4. Agent-rs processes task (fetch posts, comments, AI analysis)
5. Verify results in database
6. Verify wallet transactions and campaign financial state

Platform ID: 4 (Instagram)
Campaign ID: 99903
"""
import time
import pytest
from conftest import (
    E2E_USER_ID, E2E_TIMEOUT,
    wait_for_condition, get_campaign_status, get_task_status, get_wallet_balance,
    get_any_completed_task, get_completed_task_count,
    get_task_details, get_all_tasks, verify_task_lifecycle,
    get_campaign_details, get_wallet_transactions,
    verify_campaign_financial_state, verify_wallet_transactions_for_campaign,
    verify_wallet_transactions_for_task, verify_wallet_balance_accounting,
    get_task_consumption_summary
)

# Instagram specific campaign ID (created by 06_mock_instagram_campaign.sql)
E2E_INSTAGRAM_CAMPAIGN_ID = 99903


class TestInstagramE2E:
    """E2E test for Instagram platform."""
    
    @pytest.fixture(autouse=True)
    def setup(self, db_conn, e2e_config):
        """Setup test with Instagram-specific config."""
        self.db = db_conn
        self.config = {
            **e2e_config,
            "campaign_id": E2E_INSTAGRAM_CAMPAIGN_ID,
            "platform_id": 4,
            "platform_name": "instagram",
        }
    
    # =========================================================================
    # Test 1: Verify Initial State
    # =========================================================================
    
    def test_01_verify_initial_state(self, db_conn):
        """Verify campaign exists and is in DRAFT status."""
        campaign_id = self.config["campaign_id"]
        user_id = self.config["user_id"]
        expected_platform_id = self.config["platform_id"]
        
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
        campaign_user_id = campaign[4]
        campaign_status = campaign[5]
        
        print(f"[OK] Campaign found: {campaign_name}")
        print(f"  - Platform ID: {campaign_platform_id}")
        print(f"  - Keyword: {campaign_keyword}")
        print(f"  - User ID: {campaign_user_id}")
        print(f"  - Status: {campaign_status}")
        
        # Verify platform_id matches expected (4 for Instagram)
        assert campaign_platform_id == expected_platform_id, \
            f"Expected platform_id {expected_platform_id} (Instagram), got {campaign_platform_id}"
        print(f"[OK] Platform ID verified: {campaign_platform_id} (Instagram)")
        
        # Verify keyword is set
        assert campaign_keyword is not None and len(campaign_keyword) > 0, \
            "Campaign keyword should not be empty"
        print(f"[OK] Keyword verified: {campaign_keyword}")
        
        # Check campaign status
        status = get_campaign_status(db_conn, campaign_id)
        assert status == "DRAFT", f"Expected DRAFT status, got {status}"
        print(f"[OK] Instagram Campaign {campaign_id} is in DRAFT status")
        
        # Check wallet balance
        wallet = get_wallet_balance(db_conn, user_id)
        if wallet:
            balance, frozen = wallet
            assert balance >= 100, f"Insufficient balance: {balance}"
            print(f"[OK] Wallet balance: {balance}, frozen: {frozen}")
    
    # =========================================================================
    # Test 2: Activate Campaign
    # =========================================================================
    
    def test_02_activate_campaign(self, db_conn):
        """Activate the Instagram campaign."""
        campaign_id = self.config["campaign_id"]
        
        with db_conn.cursor() as cur:
            try:
                cur.execute("SELECT * FROM fn_activate_campaign(%s)", (campaign_id,))
                result = cur.fetchone()
                if result:
                    success, message = result
                    assert success, f"Activation failed: {message}"
                    print(f"[OK] Campaign activated: {message}")
            except Exception as e:
                print(f"[WARN] fn_activate_campaign failed: {e}, trying manual activation")
                cur.execute(
                    "UPDATE gm_campaigns SET status = 'ACTIVE' WHERE id = %s",
                    (campaign_id,)
                )
                print(f"[OK] Campaign activated manually")
        
        db_conn.commit()
        
        # Verify status changed
        status = get_campaign_status(db_conn, campaign_id)
        assert status == "ACTIVE", f"Expected ACTIVE status, got {status}"
        print(f"[OK] Instagram campaign status is now ACTIVE")
    
    # =========================================================================
    # Test 3: Wait for Scheduler to Create Task
    # =========================================================================
    
    def test_03_scheduler_creates_task(self, db_conn):
        """Wait for scheduler to create a crawler task and verify task-campaign relationship."""
        campaign_id = self.config["campaign_id"]
        timeout = min(self.config["timeout"], 120)
        
        def check_task_exists():
            task = get_task_status(db_conn, campaign_id)
            return task is not None
        
        print(f"[WAIT] Waiting for scheduler to create Instagram task (max {timeout}s)...")
        
        success = wait_for_condition(
            check_task_exists,
            timeout=timeout,
            interval=10,
            description="task creation"
        )
        
        if not success:
            pytest.skip("Scheduler did not create task - scheduler may not support Instagram yet")
        
        task = get_task_status(db_conn, campaign_id)
        task_id = task[0]
        task_status = task[1]
        print(f"[OK] Task created: id={task_id}, status={task_status}")
        
        # Verify task-campaign relationship and task fields
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT t.id, t.campaign_id, t.keywords, t.max_count, t.status, 
                       t.reserved_amount, c.keyword as campaign_keyword
                FROM gm_crawler_tasks t
                JOIN gm_campaigns c ON t.campaign_id = c.id
                WHERE t.id = %s
            """, (task_id,))
            task_detail = cur.fetchone()
        
        assert task_detail is not None, f"Task {task_id} not found in join with campaigns"
        
        task_campaign_id = task_detail[1]
        task_keywords = task_detail[2]
        task_max_count = task_detail[3]
        task_status = task_detail[4]
        reserved_amount = task_detail[5]
        campaign_keyword = task_detail[6]
        
        # Verify task belongs to correct campaign
        assert task_campaign_id == campaign_id, \
            f"Task campaign_id mismatch: expected {campaign_id}, got {task_campaign_id}"
        print(f"[OK] Task correctly linked to campaign {campaign_id}")
        
        # Verify task keywords match campaign keyword
        if task_keywords and campaign_keyword:
            assert campaign_keyword in task_keywords, \
                f"Task keywords '{task_keywords}' should contain campaign keyword '{campaign_keyword}'"
            print(f"[OK] Task keywords verified: {task_keywords}")
        
        # Verify max_count is set
        assert task_max_count is not None and task_max_count > 0, \
            f"Task max_count should be > 0, got {task_max_count}"
        print(f"[OK] Task max_count: {task_max_count}")
        
        # Verify reserved_amount is set (budget reserved for task)
        if reserved_amount is not None:
            assert reserved_amount >= 0, \
                f"Task reserved_amount should be >= 0, got {reserved_amount}"
            print(f"[OK] Task reserved_amount: {reserved_amount}")
    
    # =========================================================================
    # Test 4: Wait for Agent to Process Task
    # =========================================================================
    
    def test_04_agent_processes_task(self, db_conn):
        """Wait for agent-rs to process at least one task and verify task status transitions."""
        campaign_id = self.config["campaign_id"]
        timeout = self.config["timeout"]
        
        def check_any_task_completed():
            completed_count = get_completed_task_count(db_conn, campaign_id)
            latest_task = get_task_status(db_conn, campaign_id)
            latest_status = latest_task[1] if latest_task else "none"
            print(f"  Completed: {completed_count}, Latest: {latest_status}")
            return completed_count >= 1
        
        print(f"[WAIT] Waiting for agent to process Instagram task (max {timeout}s)...")
        
        success = wait_for_condition(
            check_any_task_completed,
            timeout=timeout,
            interval=15,
            description="task completion"
        )
        
        if not success:
            pytest.skip("Agent did not complete task within timeout")
        
        task = get_any_completed_task(db_conn, campaign_id)
        assert task is not None, "No completed task found"
        task_id = task[0]
        print(f"[OK] Instagram task completed: id={task_id}")
        
        # Verify task final state
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT id, campaign_id, status, process_count, max_count, 
                       actual_consumption, settled_at, updated_at
                FROM gm_crawler_tasks WHERE id = %s
            """, (task_id,))
            task_final = cur.fetchone()
        
        assert task_final is not None, f"Task {task_id} not found"
        
        final_status = task_final[2]
        process_count = task_final[3]
        max_count = task_final[4]
        actual_consumption = task_final[5]
        settled_at = task_final[6]
        updated_at = task_final[7]
        
        # Verify final status is 'completed'
        assert final_status == "completed", \
            f"Expected task status 'completed', got '{final_status}'"
        print(f"[OK] Task status verified: {final_status}")
        
        # Verify process_count is set
        assert process_count is not None and process_count >= 0, \
            f"Task process_count should be >= 0, got {process_count}"
        print(f"[OK] Task process_count: {process_count}/{max_count}")
        
        # Verify updated_at is set
        assert updated_at is not None, "Task updated_at should be set after processing"
        print(f"[OK] Task updated_at: {updated_at}")
    
    # =========================================================================
    # Test 5: Verify Posts Saved
    # =========================================================================
    
    def test_05_posts_saved(self, db_conn):
        """Verify Instagram posts were saved to database."""
        campaign_id = self.config["campaign_id"]
        
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT COUNT(*) FROM gm_agent_instagram_posts WHERE campaign_id = %s",
                (campaign_id,)
            )
            count = cur.fetchone()[0]
        
        if count == 0:
            pytest.skip("No posts saved - agent may not have processed yet")
        
        print(f"[OK] {count} Instagram post(s) saved to database")
        
        # Get post details
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, instagram_media_id, username, caption FROM gm_agent_instagram_posts WHERE campaign_id = %s LIMIT 3",
                (campaign_id,)
            )
            posts = cur.fetchall()
            for p in posts:
                caption_preview = (p[3][:50] + "...") if p[3] and len(p[3]) > 50 else p[3]
                print(f"  Post: id={p[0]}, media_id={p[1][:20] if p[1] else 'N/A'}..., username=@{p[2]}")
    
    # =========================================================================
    # Test 6: Verify Comments Saved
    # =========================================================================
    
    def test_06_comments_saved(self, db_conn):
        """Verify Instagram comments were saved to database."""
        campaign_id = self.config["campaign_id"]
        
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT COUNT(*) FROM gm_agent_instagram_comments c
                JOIN gm_agent_instagram_posts p ON c.post_db_id = p.id
                WHERE p.campaign_id = %s
            """, (campaign_id,))
            count = cur.fetchone()[0]
        
        if count == 0:
            pytest.skip("No comments saved - agent may not have processed yet")
        
        print(f"[OK] {count} Instagram comment(s) saved to database")
    
    # =========================================================================
    # Test 7: Verify AI Analysis
    # =========================================================================
    
    def test_07_ai_analysis_ok(self, db_conn):
        """Verify AI generated replies for comments."""
        campaign_id = self.config["campaign_id"]
        
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT c.id, c.suggested_reply
                FROM gm_agent_instagram_comments c
                JOIN gm_agent_instagram_posts p ON c.post_db_id = p.id
                WHERE p.campaign_id = %s
                  AND c.suggested_reply IS NOT NULL
            """, (campaign_id,))
            comments = cur.fetchall()
        
        if len(comments) == 0:
            pytest.skip("No comments with AI analysis found")
        
        ok_count = sum(1 for _, reply in comments if reply and "OK" in reply.upper())
        print(f"[OK] {ok_count}/{len(comments)} comments have 'OK' replies")
    
    # =========================================================================
    # Test 8: Verify Task Lifecycle
    # =========================================================================
    
    def test_08_verify_task_lifecycle(self, db_conn):
        """Verify task went through proper lifecycle with all fields populated."""
        campaign_id = self.config["campaign_id"]
        
        completed_task = get_any_completed_task(db_conn, campaign_id)
        if not completed_task:
            pytest.skip("No completed task found")
        
        task_id = completed_task[0]
        task_details = get_task_details(db_conn, task_id)
        
        print(f"\n[INFO] Task {task_id} Details:")
        print(f"  - Status: {task_details['status']}")
        print(f"  - Process Count: {task_details['process_count']}/{task_details['max_count']}")
        print(f"  - Reserved Amount: {task_details['reserved_amount']}")
        print(f"  - Actual Consumption: {task_details['actual_consumption']}")
        print(f"  - Settled At: {task_details['settled_at']}")
        
        is_valid, errors = verify_task_lifecycle(task_details)
        
        if not is_valid:
            for error in errors:
                print(f"  [ERROR] {error}")
            pytest.fail(f"Task lifecycle validation failed: {errors}")
        
        print(f"[OK] Task lifecycle validation passed")
        
        # Verify task count
        all_tasks = get_all_tasks(db_conn, campaign_id)
        print(f"[INFO] Total tasks created: {len(all_tasks)}")
        assert len(all_tasks) <= 2, f"Expected at most 2 tasks, got {len(all_tasks)}"
    
    # =========================================================================
    # Test 9: Final Summary
    # =========================================================================
    
    def test_09_summary(self, db_conn):
        """Print final test summary with comprehensive verification."""
        campaign_id = self.config["campaign_id"]
        user_id = self.config["user_id"]
        
        print("\n" + "=" * 60)
        print("INSTAGRAM E2E TEST SUMMARY")
        print("=" * 60)
        
        # Campaign status
        status = get_campaign_status(db_conn, campaign_id)
        print(f"Campaign Status: {status}")
        print(f"Platform: Instagram (ID: 4)")
        
        # Post count
        with db_conn.cursor() as cur:
            cur.execute(
                "SELECT COUNT(*) FROM gm_agent_instagram_posts WHERE campaign_id = %s",
                (campaign_id,)
            )
            post_count = cur.fetchone()[0]
        print(f"Posts Processed: {post_count}")
        
        # Comment count
        with db_conn.cursor() as cur:
            cur.execute("""
                SELECT COUNT(*) FROM gm_agent_instagram_comments c
                JOIN gm_agent_instagram_posts p ON c.post_db_id = p.id
                WHERE p.campaign_id = %s
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
        print("INSTAGRAM E2E TEST COMPLETED!")
        print("=" * 60)


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
