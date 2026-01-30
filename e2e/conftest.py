"""
E2E Test Configuration and Fixtures
"""
import os
import time
import pytest
import psycopg2
import redis
from dotenv import load_dotenv
from tenacity import retry, stop_after_attempt, wait_exponential

# Load environment variables
load_dotenv()

# ============================================================================
# Configuration
# ============================================================================

DB_CONFIG = {
    "host": os.getenv("POSTGRES_HOST", "localhost"),
    "port": int(os.getenv("POSTGRES_PORT", "5433")),
    "database": os.getenv("POSTGRES_DB", "aihub_e2e_db"),
    "user": os.getenv("POSTGRES_USER", "aihub_user"),
    "password": os.getenv("POSTGRES_PASSWORD", "aihub_password"),
}

REDIS_CONFIG = {
    "host": os.getenv("REDIS_HOST", "localhost"),
    "port": int(os.getenv("REDIS_PORT", "6380")),
    "db": 0,
}

E2E_USER_ID = int(os.getenv("E2E_USER_ID", "99999"))
E2E_CAMPAIGN_ID = int(os.getenv("E2E_CAMPAIGN_ID", "99901"))
E2E_TIMEOUT = int(os.getenv("E2E_TIMEOUT", "300"))


# ============================================================================
# Fixtures
# ============================================================================

@pytest.fixture(scope="module")
def db_conn():
    """Database connection fixture."""
    conn = None
    for attempt in range(30):
        try:
            conn = psycopg2.connect(**DB_CONFIG)
            conn.autocommit = True
            print(f"[OK] Database connected after {attempt + 1} attempts")
            break
        except psycopg2.OperationalError as e:
            print(f"[WAIT] Database not ready (attempt {attempt + 1}/30): {e}")
            time.sleep(2)
    
    if conn is None:
        pytest.fail("Could not connect to database after 30 attempts")
    
    yield conn
    conn.close()


@pytest.fixture(scope="module")
def redis_client():
    """Redis client fixture."""
    client = None
    for attempt in range(30):
        try:
            client = redis.Redis(**REDIS_CONFIG)
            client.ping()
            print(f"[OK] Redis connected after {attempt + 1} attempts")
            break
        except redis.ConnectionError as e:
            print(f"[WAIT] Redis not ready (attempt {attempt + 1}/30): {e}")
            time.sleep(2)
    
    if client is None:
        pytest.fail("Could not connect to Redis after 30 attempts")
    
    yield client
    client.close()


@pytest.fixture(scope="module")
def e2e_config():
    """E2E test configuration."""
    return {
        "user_id": E2E_USER_ID,
        "campaign_id": E2E_CAMPAIGN_ID,
        "timeout": E2E_TIMEOUT,
    }


# ============================================================================
# Helper Functions
# ============================================================================

def wait_for_condition(check_fn, timeout=60, interval=5, description="condition"):
    """Wait for a condition to be true."""
    start = time.time()
    while time.time() - start < timeout:
        try:
            if check_fn():
                return True
        except Exception as e:
            print(f"[WAIT] Checking {description}: {e}")
        time.sleep(interval)
    return False


def get_campaign_status(conn, campaign_id):
    """Get campaign status from database."""
    with conn.cursor() as cur:
        cur.execute("SELECT status FROM gm_campaigns WHERE id = %s", (campaign_id,))
        row = cur.fetchone()
        return row[0] if row else None


def get_task_status(conn, campaign_id):
    """Get latest task status for a campaign."""
    with conn.cursor() as cur:
        cur.execute(
            "SELECT id, status FROM gm_crawler_tasks WHERE campaign_id = %s ORDER BY id DESC LIMIT 1",
            (campaign_id,)
        )
        return cur.fetchone()


def get_any_completed_task(conn, campaign_id):
    """Check if any task has been completed for a campaign."""
    with conn.cursor() as cur:
        cur.execute(
            "SELECT id, status FROM gm_crawler_tasks WHERE campaign_id = %s AND status = 'completed' ORDER BY id LIMIT 1",
            (campaign_id,)
        )
        return cur.fetchone()


def get_completed_task_count(conn, campaign_id):
    """Get count of completed tasks for a campaign."""
    with conn.cursor() as cur:
        cur.execute(
            "SELECT COUNT(*) FROM gm_crawler_tasks WHERE campaign_id = %s AND status = 'completed'",
            (campaign_id,)
        )
        return cur.fetchone()[0]


def get_wallet_balance(conn, user_id):
    """Get user wallet balance."""
    with conn.cursor() as cur:
        cur.execute(
            "SELECT balance_points, frozen_points FROM gm_user_wallets WHERE user_id = %s",
            (user_id,)
        )
        return cur.fetchone()
