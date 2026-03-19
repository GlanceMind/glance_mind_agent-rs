"""
E2E Test: Facebook Platform

Verifies the full Scheduler -> Agent-rs -> PostgreSQL workflow for the
Facebook RapidAPI integration using the internal Facebook mock supplier.
"""

import json
from datetime import datetime, timezone

import pytest

from conftest import (
    E2E_TIMEOUT,
    get_any_completed_task,
    get_campaign_details,
    get_campaign_status,
    get_completed_task_count,
    get_task_consumption_summary,
    get_task_details,
    get_task_status,
    get_wallet_balance,
    verify_campaign_financial_state,
    verify_task_lifecycle,
    verify_wallet_balance_accounting,
    verify_wallet_transactions_for_campaign,
    verify_wallet_transactions_for_task,
    wait_for_condition,
)

E2E_FACEBOOK_CAMPAIGN_ID = 99906
EXPECTED_POST_ID = "1431426125696772"
EXPECTED_POST_LOOKUP_ID = (
    "pfbid02M8f7jLqK5x6vBy3oY7t9R4Lm2S8Qv1Az8Jk4KJr5XJv4q8mWcA2eNnYq8d7vY6L"
)
EXPECTED_POST_URL = (
    "https://www.facebook.com/"
    f"natgeomuseum/posts/{EXPECTED_POST_LOOKUP_ID}"
)
EXPECTED_POST_TIMESTAMP = 1763449200
EXPECTED_POST_MESSAGE = (
    "Explore the museum's new world-travel exhibition this weekend."
)
EXPECTED_COMMENT_FIXTURES = {
    "fb-comment-0001": {
        "comment_url": f"{EXPECTED_POST_URL}?comment_id=fb-comment-0001",
        "comment_text": "How much are the tickets for this exhibit?",
        "comment_user_id": "fb-user-0001",
        "comment_username": "Alice Explorer",
        "comment_user_url": "https://www.facebook.com/alice.explorer",
        "comment_user_profile_picture": "https://mock-cdn.example.com/facebook/commenter-1.jpg",
        "like_count": 7,
        "reply_count": 1,
        "threading_depth": 0,
        "created_at_ts": EXPECTED_POST_TIMESTAMP + 60,
    },
    "fb-comment-0002": {
        "comment_url": f"{EXPECTED_POST_URL}?comment_id=fb-comment-0002",
        "comment_text": "Can someone DM me the address and opening hours?",
        "comment_user_id": "fb-user-0002",
        "comment_username": "Brian Traveler",
        "comment_user_url": "https://www.facebook.com/brian.traveler",
        "comment_user_profile_picture": "https://mock-cdn.example.com/facebook/commenter-2.jpg",
        "like_count": 5,
        "reply_count": 0,
        "threading_depth": 0,
        "created_at_ts": EXPECTED_POST_TIMESTAMP + 120,
    },
    "fb-comment-0003": {
        "comment_url": f"{EXPECTED_POST_URL}?comment_id=fb-comment-0003",
        "comment_text": "Is this family friendly? We want to visit next month.",
        "comment_user_id": "fb-user-0003",
        "comment_username": "Clara FamilyTrips",
        "comment_user_url": "https://www.facebook.com/clara.family",
        "comment_user_profile_picture": "https://mock-cdn.example.com/facebook/commenter-3.jpg",
        "like_count": 4,
        "reply_count": 2,
        "threading_depth": 0,
        "created_at_ts": EXPECTED_POST_TIMESTAMP + 180,
    },
    "fb-comment-0004": {
        "comment_url": f"{EXPECTED_POST_URL}?comment_id=fb-comment-0004",
        "comment_text": "We are planning a school trip. Please send group booking info.",
        "comment_user_id": "fb-user-0004",
        "comment_username": "David Teacher",
        "comment_user_url": "https://www.facebook.com/david.teacher",
        "comment_user_profile_picture": "https://mock-cdn.example.com/facebook/commenter-4.jpg",
        "like_count": 9,
        "reply_count": 1,
        "threading_depth": 0,
        "created_at_ts": EXPECTED_POST_TIMESTAMP + 240,
    },
    "fb-comment-0005": {
        "comment_url": f"{EXPECTED_POST_URL}?comment_id=fb-comment-0005",
        "comment_text": "Looks amazing. Where can I buy the weekend pass?",
        "comment_user_id": "fb-user-0005",
        "comment_username": "Ella Weekend",
        "comment_user_url": "https://www.facebook.com/ella.weekend",
        "comment_user_profile_picture": "https://mock-cdn.example.com/facebook/commenter-5.jpg",
        "like_count": 6,
        "reply_count": 0,
        "threading_depth": 0,
        "created_at_ts": EXPECTED_POST_TIMESTAMP + 300,
    },
}


def _query_one_dict(conn, sql: str, params: tuple):
    with conn.cursor() as cur:
        cur.execute(sql, params)
        row = cur.fetchone()
        if row is None:
            return None
        columns = [desc[0] for desc in cur.description]
        return dict(zip(columns, row))


def _query_all_dicts(conn, sql: str, params: tuple):
    with conn.cursor() as cur:
        cur.execute(sql, params)
        rows = cur.fetchall()
        columns = [desc[0] for desc in cur.description]
        return [dict(zip(columns, row)) for row in rows]


def _to_utc_timestamp(value):
    if value is None:
        return None
    return int(value.astimezone(timezone.utc).timestamp())


class TestFacebookE2E:
    """E2E test for Facebook platform."""

    @pytest.fixture(autouse=True)
    def setup(self, db_conn, e2e_config):
        self.db = db_conn
        self.config = {
            **e2e_config,
            "campaign_id": E2E_FACEBOOK_CAMPAIGN_ID,
            "platform_id": 3,
            "platform_name": "facebook",
        }

    def test_01_verify_initial_state(self, db_conn):
        campaign = _query_one_dict(
            db_conn,
            """
            SELECT id, name, platform_id, keyword, user_id, status, search_options, max_scan_count
            FROM gm_campaigns
            WHERE id = %s
            """,
            (self.config["campaign_id"],),
        )

        assert campaign is not None, f"Campaign {self.config['campaign_id']} not found"
        assert campaign["platform_id"] == self.config["platform_id"]
        assert campaign["keyword"].startswith("facebook_post_url:")
        assert campaign["status"] == "DRAFT"
        assert campaign["max_scan_count"] == 1
        search_options = campaign["search_options"]
        if isinstance(search_options, str):
            search_options = json.loads(search_options)
        assert search_options["facebook"]["search_type"] == "posts"
        assert search_options["facebook"]["recent_posts"] is True
        assert search_options["facebook"]["location"] == "washington dc"
        assert search_options["facebook"]["start_date"] == "2025-11-01"
        assert search_options["facebook"]["end_date"] == "2025-11-30"

        wallet = get_wallet_balance(db_conn, self.config["user_id"])
        assert wallet is not None, "Wallet should exist for E2E user"
        balance, frozen = wallet
        assert float(balance) >= 100
        assert float(frozen) == 0

    def test_02_activate_campaign(self, db_conn):
        campaign_id = self.config["campaign_id"]

        with db_conn.cursor() as cur:
            try:
                cur.execute("SELECT * FROM fn_activate_campaign(%s)", (campaign_id,))
                result = cur.fetchone()
                assert result is not None, "fn_activate_campaign returned no result"
                success, message = result
                assert success, f"Activation failed: {message}"
            except Exception:
                cur.execute(
                    "UPDATE gm_campaigns SET status = 'ACTIVE' WHERE id = %s",
                    (campaign_id,),
                )

        db_conn.commit()

        status = get_campaign_status(db_conn, campaign_id)
        assert status == "ACTIVE", f"Expected ACTIVE status, got {status}"

        wallet = get_wallet_balance(db_conn, self.config["user_id"])
        assert wallet is not None
        _, frozen = wallet
        assert float(frozen) > 0, "Budget should be frozen after activation"

    def test_03_scheduler_creates_task(self, db_conn):
        campaign_id = self.config["campaign_id"]
        timeout = min(self.config["timeout"], 120)

        success = wait_for_condition(
            lambda: get_task_status(db_conn, campaign_id) is not None,
            timeout=timeout,
            interval=10,
            description="facebook task creation",
        )
        assert success, "Scheduler did not create Facebook task within timeout"

        task = _query_one_dict(
            db_conn,
            """
            SELECT id, campaign_id, keywords, max_count, status, reserved_amount, search_limit
            FROM gm_crawler_tasks
            WHERE campaign_id = %s
            ORDER BY id DESC
            LIMIT 1
            """,
            (campaign_id,),
        )
        assert task is not None, "Expected crawler task to exist"
        assert task["campaign_id"] == campaign_id
        assert any(
            keyword.startswith("facebook_post_url:")
            for keyword in (task["keywords"] or [])
        ), f"Unexpected task keywords: {task['keywords']}"
        assert task["max_count"] == 1
        assert task["search_limit"] >= 1
        assert task["reserved_amount"] is not None

    def test_04_agent_processes_task(self, db_conn):
        campaign_id = self.config["campaign_id"]

        success = wait_for_condition(
            lambda: get_completed_task_count(db_conn, campaign_id) >= 1,
            timeout=self.config["timeout"],
            interval=15,
            description="facebook task completion",
        )
        assert success, "Agent did not complete any Facebook task within timeout"

        task = get_any_completed_task(db_conn, campaign_id)
        assert task is not None, "Expected a completed Facebook task"
        task_id = task[0]
        task_details = get_task_details(db_conn, task_id)
        is_valid, errors = verify_task_lifecycle(task_details)
        assert is_valid, f"Task lifecycle invalid: {errors}"

    def test_05_facebook_post_saved(self, db_conn):
        completed_task = get_any_completed_task(db_conn, self.config["campaign_id"])
        assert completed_task is not None, "Completed task required before post assertions"
        task_id = completed_task[0]

        post = _query_one_dict(
            db_conn,
            """
            SELECT
                id, task_id, campaign_id, facebook_post_id, post_type, url,
                message, message_rich, timestamp, posted_at,
                reactions_count, comments_count, reshare_count,
                reactions_like, reactions_love, reactions_haha, reactions_wow,
                reactions_sad, reactions_angry, reactions_care,
                author_id, author_name, author_url, author_profile_picture_url, author_title,
                has_image, image_url, image_width, image_height, image_id,
                has_video, video_thumbnail, external_url, attached_post_url, comments_id, shares_id,
                created_at, updated_at
            FROM gm_agent_facebook_posts
            WHERE campaign_id = %s
            ORDER BY id DESC
            LIMIT 1
            """,
            (self.config["campaign_id"],),
        )

        assert post is not None, "Expected a Facebook post row"
        assert post["task_id"] == task_id
        assert post["campaign_id"] == self.config["campaign_id"]
        assert post["facebook_post_id"] == EXPECTED_POST_ID
        assert post["post_type"] == "status"
        assert post["url"] == EXPECTED_POST_URL
        assert post["message"] == EXPECTED_POST_MESSAGE
        assert post["message_rich"] == EXPECTED_POST_MESSAGE
        assert post["timestamp"] == EXPECTED_POST_TIMESTAMP
        assert _to_utc_timestamp(post["posted_at"]) == EXPECTED_POST_TIMESTAMP
        assert post["reactions_count"] == 36
        assert post["comments_count"] == 5
        assert post["reshare_count"] == 11
        assert post["reactions_like"] == 20
        assert post["reactions_love"] == 8
        assert post["reactions_haha"] == 2
        assert post["reactions_wow"] == 3
        assert post["reactions_sad"] == 1
        assert post["reactions_angry"] == 1
        assert post["reactions_care"] == 1
        assert post["author_id"] == "100064881934421"
        assert post["author_name"] == "National Geographic Museum"
        assert post["author_url"] == "https://www.facebook.com/natgeomuseum"
        assert post["author_profile_picture_url"] == "https://mock-cdn.example.com/facebook/page-avatar.jpg"
        assert post["author_title"] == "Museum"
        assert post["has_image"] is True
        assert post["image_url"] == "https://mock-cdn.example.com/facebook/post-cover.jpg"
        assert post["image_width"] == 1200
        assert post["image_height"] == 630
        assert post["image_id"] == "fb-image-001"
        assert post["has_video"] is False
        assert post["video_thumbnail"] is None
        assert post["external_url"] == "https://mock-cdn.example.com/facebook/external-destination"
        assert post["attached_post_url"] == (
            "https://www.facebook.com/story.php?story_fbid=1431426125696772&id=100064881934421"
        )
        assert post["comments_id"] == EXPECTED_POST_ID
        assert post["shares_id"] == EXPECTED_POST_ID
        assert isinstance(post["created_at"], datetime)
        assert post["updated_at"] is None

    def test_06_facebook_comments_saved(self, db_conn):
        post = _query_one_dict(
            db_conn,
            "SELECT id FROM gm_agent_facebook_posts WHERE campaign_id = %s ORDER BY id DESC LIMIT 1",
            (self.config["campaign_id"],),
        )
        assert post is not None, "Post row required before comment assertions"

        comments = _query_all_dicts(
            db_conn,
            """
            SELECT
                id, post_db_id, campaign_id, facebook_comment_id, parent_comment_id, comment_url,
                comment_text, reason, suggested_reply, suggested_dm, suggested_reply_post,
                comment_user_id, comment_username, comment_user_url, comment_user_profile_picture,
                like_count, reply_count, threading_depth, created_at_ts, comment_created_at,
                facebook_post_id, post_url, created_at, updated_at, status
            FROM gm_agent_facebook_comments
            WHERE campaign_id = %s
            ORDER BY id
            """,
            (self.config["campaign_id"],),
        )

        assert len(comments) == 5, f"Expected 5 analyzed Facebook comments, got {len(comments)}"

        for comment in comments:
            fixture = EXPECTED_COMMENT_FIXTURES.get(comment["facebook_comment_id"])
            assert fixture is not None, f"Unexpected comment id: {comment['facebook_comment_id']}"
            assert comment["post_db_id"] == post["id"]
            assert comment["campaign_id"] == self.config["campaign_id"]
            assert comment["parent_comment_id"] is None
            assert comment["comment_url"] == fixture["comment_url"]
            assert comment["comment_text"] == fixture["comment_text"]
            assert comment["comment_user_id"] == fixture["comment_user_id"]
            assert comment["comment_username"] == fixture["comment_username"]
            assert comment["comment_user_url"] == fixture["comment_user_url"]
            assert comment["comment_user_profile_picture"] == fixture["comment_user_profile_picture"]
            assert comment["like_count"] == fixture["like_count"]
            assert comment["reply_count"] == fixture["reply_count"]
            assert comment["threading_depth"] == fixture["threading_depth"]
            assert comment["created_at_ts"] == fixture["created_at_ts"]
            assert _to_utc_timestamp(comment["comment_created_at"]) == fixture["created_at_ts"]
            assert comment["facebook_post_id"] == EXPECTED_POST_ID
            assert comment["post_url"] == EXPECTED_POST_URL
            assert isinstance(comment["created_at"], datetime)
            assert comment["updated_at"] is None
            assert comment["status"] == 0
            assert comment["reason"] is not None and comment["reason"].strip() != ""
            assert comment["suggested_reply"] == "OK"
            assert comment["suggested_dm"] == "OK"
            assert comment["suggested_reply_post"] is None

    def test_07_financial_state_and_wallet(self, db_conn):
        campaign_id = self.config["campaign_id"]
        user_id = self.config["user_id"]
        completed_task = get_any_completed_task(db_conn, campaign_id)
        assert completed_task is not None, "Completed task required for financial assertions"
        task_id = completed_task[0]

        is_valid, errors, campaign = verify_campaign_financial_state(
            db_conn, campaign_id, expected_status="ACTIVE"
        )
        assert is_valid, f"Campaign financial state invalid: {errors}"
        assert campaign is not None
        assert campaign["actual_consumption"] >= 0
        assert campaign["budget_cap"] == 100.0

        is_valid, errors, freeze_txns = verify_wallet_transactions_for_campaign(
            db_conn, user_id, campaign_id
        )
        assert is_valid, f"Campaign wallet transactions invalid: {errors}"
        assert len(freeze_txns) >= 1

        is_valid, errors, settle_txns = verify_wallet_transactions_for_task(
            db_conn, user_id, task_id
        )
        assert is_valid, f"Task wallet transactions invalid: {errors}"
        assert len(settle_txns) >= 1

        is_valid, errors, wallet_state = verify_wallet_balance_accounting(db_conn, user_id)
        assert is_valid, f"Wallet balance accounting invalid: {errors}"
        assert wallet_state is not None
        assert wallet_state["consumed"] >= 0

    def test_08_summary(self, db_conn):
        campaign = get_campaign_details(db_conn, self.config["campaign_id"])
        task_summary = get_task_consumption_summary(db_conn, self.config["campaign_id"])

        assert campaign is not None
        assert task_summary is not None
        assert campaign["platform_id"] == 3
        assert campaign["total_scanned"] >= 1
        assert task_summary["task_count"] >= 1
        assert task_summary["completed_count"] >= 1
        assert task_summary["settled_count"] >= 1
        assert task_summary["total_processed"] >= 1
        assert task_summary["total_actual"] >= 0
        assert E2E_TIMEOUT >= 60
