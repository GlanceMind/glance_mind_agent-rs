"""
E2E Test: Twitter Platform

Verifies the full Scheduler -> Agent-rs -> PostgreSQL workflow for the
Twitter integration using the internal TikHub mock supplier.
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

E2E_TWITTER_CAMPAIGN_ID = 99905
EXPECTED_TWEET_ID = "1808168603721650364"
EXPECTED_TWEET_URL = f"https://twitter.com/glancemindbot/status/{EXPECTED_TWEET_ID}"
EXPECTED_TWEET_TEXT = (
    "Mock detail tweet for GlanceMind Twitter E2E coverage. "
    "Reply with OK if you need the product page."
)
EXPECTED_TWEET_CREATED_AT = "Thu Mar 05 12:30:45 +0000 2026"
EXPECTED_TWEET_MEDIA_URL = "https://mock-cdn.example.com/twitter/tweet-detail-photo.jpg"

EXPECTED_COMMENT_FIXTURES = {
    "tw-reply-0001": {
        "comment_screen_name": "alice_builder",
        "comment_user_name": "Alice Builder",
        "comment_user_id": "tw-user-1001",
        "comment_user_followers": 321,
        "comment_text": "Can you share pricing details?",
        "favorite_count": 5,
        "retweet_count": 1,
        "reply_count": 0,
        "in_reply_to_status_id": EXPECTED_TWEET_ID,
        "media_urls": None,
        "has_media": False,
        "created_at_str": "Thu Mar 05 12:31:45 +0000 2026",
    },
    "tw-reply-0002": {
        "comment_screen_name": "ben_ops",
        "comment_user_name": "Ben Ops",
        "comment_user_id": "tw-user-1002",
        "comment_user_followers": 654,
        "comment_text": "Is there a demo available for teams?",
        "favorite_count": 8,
        "retweet_count": 2,
        "reply_count": 1,
        "in_reply_to_status_id": EXPECTED_TWEET_ID,
        "media_urls": ["https://mock-cdn.example.com/twitter/reply-2-photo.jpg"],
        "has_media": True,
        "created_at_str": "Thu Mar 05 12:32:45 +0000 2026",
    },
    "tw-reply-0003": {
        "comment_screen_name": "clara_growth",
        "comment_user_name": "Clara Growth",
        "comment_user_id": "tw-user-1003",
        "comment_user_followers": 777,
        "comment_text": "Where can I read customer reviews?",
        "favorite_count": 3,
        "retweet_count": 0,
        "reply_count": 0,
        "in_reply_to_status_id": EXPECTED_TWEET_ID,
        "media_urls": None,
        "has_media": False,
        "created_at_str": "Thu Mar 05 12:33:45 +0000 2026",
    },
    "tw-reply-0004": {
        "comment_screen_name": "dana_cs",
        "comment_user_name": "Dana Success",
        "comment_user_id": "tw-user-1004",
        "comment_user_followers": 912,
        "comment_text": "Please DM the onboarding guide.",
        "favorite_count": 11,
        "retweet_count": 3,
        "reply_count": 2,
        "in_reply_to_status_id": EXPECTED_TWEET_ID,
        "media_urls": None,
        "has_media": False,
        "created_at_str": "Thu Mar 05 12:34:45 +0000 2026",
    },
    "tw-reply-0005": {
        "comment_screen_name": "eli_product",
        "comment_user_name": "Eli Product",
        "comment_user_id": "tw-user-1005",
        "comment_user_followers": 1200,
        "comment_text": "Does it support scheduling replies?",
        "favorite_count": 6,
        "retweet_count": 1,
        "reply_count": 0,
        "in_reply_to_status_id": EXPECTED_TWEET_ID,
        "media_urls": None,
        "has_media": False,
        "created_at_str": "Thu Mar 05 12:35:45 +0000 2026",
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


def _twitter_timestamp(value: str) -> int:
    parsed = datetime.strptime(value, "%a %b %d %H:%M:%S %z %Y")
    return int(parsed.astimezone(timezone.utc).timestamp())


def _to_utc_timestamp(value):
    if value is None:
        return None
    return int(value.astimezone(timezone.utc).timestamp())


class TestTwitterE2E:
    """E2E test for Twitter platform."""

    @pytest.fixture(autouse=True)
    def setup(self, db_conn, e2e_config):
        self.db = db_conn
        self.config = {
            **e2e_config,
            "campaign_id": E2E_TWITTER_CAMPAIGN_ID,
            "platform_id": 5,
            "platform_name": "twitter",
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
        assert campaign["keyword"] == f"twitter_tweet_id:{EXPECTED_TWEET_ID}"
        assert campaign["status"] == "DRAFT"
        assert campaign["max_scan_count"] == 1
        search_options = campaign["search_options"]
        if isinstance(search_options, str):
            search_options = json.loads(search_options)
        assert search_options["twitter"]["search_type"] == "Top"

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
            description="twitter task creation",
        )
        assert success, "Scheduler did not create Twitter task within timeout"

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
        assert task["keywords"] == [f"twitter_tweet_id:{EXPECTED_TWEET_ID}"]
        assert task["max_count"] == 1
        assert task["search_limit"] >= 1
        assert task["reserved_amount"] is not None

    def test_04_agent_processes_task(self, db_conn):
        campaign_id = self.config["campaign_id"]

        success = wait_for_condition(
            lambda: get_completed_task_count(db_conn, campaign_id) >= 1,
            timeout=self.config["timeout"],
            interval=15,
            description="twitter task completion",
        )
        assert success, "Agent did not complete any Twitter task within timeout"

        task = get_any_completed_task(db_conn, campaign_id)
        assert task is not None, "Expected a completed Twitter task"
        task_id = task[0]
        task_details = get_task_details(db_conn, task_id)
        is_valid, errors = verify_task_lifecycle(task_details)
        assert is_valid, f"Task lifecycle invalid: {errors}"

    def test_05_twitter_tweet_saved(self, db_conn):
        completed_task = get_any_completed_task(db_conn, self.config["campaign_id"])
        assert completed_task is not None, "Completed task required before tweet assertions"
        task_id = completed_task[0]

        tweet = _query_one_dict(
            db_conn,
            """
            SELECT
                id, task_id, campaign_id, twitter_tweet_id, conversation_id, full_text,
                lang, screen_name, user_name, user_id, user_description,
                user_followers_count, user_avatar, user_verified, media_urls, has_media,
                favorite_count, retweet_count, reply_count, quote_count, bookmark_count,
                view_count, is_reply, in_reply_to_status_id, in_reply_to_user_id,
                created_at_str, created_at_ts, tweet_created_at, created_at, updated_at
            FROM gm_agent_twitter_tweets
            WHERE campaign_id = %s
            ORDER BY id DESC
            LIMIT 1
            """,
            (self.config["campaign_id"],),
        )

        assert tweet is not None, "Expected a Twitter tweet row"
        assert tweet["task_id"] == task_id
        assert tweet["campaign_id"] == self.config["campaign_id"]
        assert tweet["twitter_tweet_id"] == EXPECTED_TWEET_ID
        assert tweet["conversation_id"] == EXPECTED_TWEET_ID
        assert tweet["full_text"] == EXPECTED_TWEET_TEXT
        assert tweet["lang"] == "en"
        assert tweet["screen_name"] == "glancemindbot"
        assert tweet["user_name"] == "GlanceMind Bot"
        assert tweet["user_id"] == "tw-user-9001"
        assert tweet["user_description"] == "Mock automation account for deterministic Twitter E2E coverage."
        assert tweet["user_followers_count"] == 9876
        assert tweet["user_avatar"] == "https://mock-cdn.example.com/twitter/glancemindbot-avatar.jpg"
        assert tweet["user_verified"] is True
        assert tweet["media_urls"] == [EXPECTED_TWEET_MEDIA_URL]
        assert tweet["has_media"] is True
        assert tweet["favorite_count"] == 42
        assert tweet["retweet_count"] == 7
        assert tweet["reply_count"] == 5
        assert tweet["quote_count"] == 3
        assert tweet["bookmark_count"] == 14
        assert tweet["view_count"] == 1234
        assert tweet["is_reply"] is False
        assert tweet["in_reply_to_status_id"] is None
        assert tweet["in_reply_to_user_id"] is None
        assert tweet["created_at_str"] == EXPECTED_TWEET_CREATED_AT
        assert tweet["created_at_ts"] == _twitter_timestamp(EXPECTED_TWEET_CREATED_AT)
        assert _to_utc_timestamp(tweet["tweet_created_at"]) == _twitter_timestamp(EXPECTED_TWEET_CREATED_AT)
        assert isinstance(tweet["created_at"], datetime)
        assert tweet["updated_at"] is None

    def test_06_twitter_comments_saved(self, db_conn):
        tweet = _query_one_dict(
            db_conn,
            """
            SELECT id
            FROM gm_agent_twitter_tweets
            WHERE campaign_id = %s
            ORDER BY id DESC
            LIMIT 1
            """,
            (self.config["campaign_id"],),
        )
        assert tweet is not None, "Tweet row required before comment assertions"

        comments = _query_all_dicts(
            db_conn,
            """
            SELECT
                id, tweet_db_id, campaign_id, twitter_comment_id, conversation_id,
                comment_screen_name, comment_user_name, comment_user_id, comment_user_followers,
                comment_text, reason, suggested_reply, favorite_count, retweet_count, reply_count,
                in_reply_to_status_id, is_reply, media_urls, has_media, created_at_str,
                created_at_ts, comment_created_at, created_at, updated_at, suggested_dm,
                suggested_reply_post, status
            FROM gm_agent_twitter_comments
            WHERE campaign_id = %s
            ORDER BY id
            """,
            (self.config["campaign_id"],),
        )

        assert len(comments) == 5, f"Expected 5 analyzed Twitter comments, got {len(comments)}"

        for comment in comments:
            fixture = EXPECTED_COMMENT_FIXTURES.get(comment["twitter_comment_id"])
            assert fixture is not None, f"Unexpected comment id: {comment['twitter_comment_id']}"
            assert comment["tweet_db_id"] == tweet["id"]
            assert comment["campaign_id"] == self.config["campaign_id"]
            assert comment["conversation_id"] == EXPECTED_TWEET_ID
            assert comment["comment_screen_name"] == fixture["comment_screen_name"]
            assert comment["comment_user_name"] == fixture["comment_user_name"]
            assert comment["comment_user_id"] == fixture["comment_user_id"]
            assert comment["comment_user_followers"] == fixture["comment_user_followers"]
            assert comment["comment_text"] == fixture["comment_text"]
            assert comment["favorite_count"] == fixture["favorite_count"]
            assert comment["retweet_count"] == fixture["retweet_count"]
            assert comment["reply_count"] == fixture["reply_count"]
            assert comment["in_reply_to_status_id"] == fixture["in_reply_to_status_id"]
            assert comment["is_reply"] is True
            assert comment["media_urls"] == fixture["media_urls"]
            assert comment["has_media"] == fixture["has_media"]
            assert comment["created_at_str"] == fixture["created_at_str"]
            assert comment["created_at_ts"] == _twitter_timestamp(fixture["created_at_str"])
            assert _to_utc_timestamp(comment["comment_created_at"]) == _twitter_timestamp(
                fixture["created_at_str"]
            )
            assert isinstance(comment["created_at"], datetime)
            assert comment["updated_at"] is None
            assert str(comment["status"]) == "0"
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
        assert campaign["platform_id"] == 5
        assert campaign["total_scanned"] >= 1
        assert task_summary["task_count"] >= 1
        assert task_summary["completed_count"] >= 1
        assert task_summary["settled_count"] >= 1
        assert task_summary["total_processed"] >= 1
        assert task_summary["total_actual"] >= 0
        assert E2E_TIMEOUT >= 60
