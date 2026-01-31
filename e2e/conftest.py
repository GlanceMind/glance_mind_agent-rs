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


def get_task_details(conn, task_id):
    """Get full task details by task_id."""
    with conn.cursor() as cur:
        cur.execute("""
            SELECT id, campaign_id, keywords, max_count, process_count,
                   status, search_offset, search_limit, reserved_amount,
                   actual_consumption, settled_at, created_at, updated_at
            FROM gm_crawler_tasks WHERE id = %s
        """, (task_id,))
        row = cur.fetchone()
        if row:
            return {
                "id": row[0],
                "campaign_id": row[1],
                "keywords": row[2],
                "max_count": row[3],
                "process_count": row[4],
                "status": row[5],
                "search_offset": row[6],
                "search_limit": row[7],
                "reserved_amount": float(row[8]) if row[8] else 0,
                "actual_consumption": float(row[9]) if row[9] else 0,
                "settled_at": row[10],
                "created_at": row[11],
                "updated_at": row[12],
            }
        return None


def get_all_tasks(conn, campaign_id):
    """Get all tasks for a campaign."""
    with conn.cursor() as cur:
        cur.execute("""
            SELECT id, status, process_count, max_count, reserved_amount,
                   actual_consumption, settled_at, created_at
            FROM gm_crawler_tasks WHERE campaign_id = %s ORDER BY id
        """, (campaign_id,))
        return cur.fetchall()


def verify_task_lifecycle(task_details):
    """
    Verify a task went through proper lifecycle.
    Returns (is_valid, errors) tuple.
    """
    errors = []
    
    if not task_details:
        return False, ["Task not found"]
    
    # Status should be completed
    if task_details["status"] != "completed":
        errors.append(f"Expected status 'completed', got '{task_details['status']}'")
    
    # process_count should be > 0 if task completed
    if task_details["process_count"] <= 0:
        errors.append(f"Expected process_count > 0, got {task_details['process_count']}")
    
    # process_count should not exceed max_count
    if task_details["process_count"] > task_details["max_count"]:
        errors.append(f"process_count ({task_details['process_count']}) exceeds max_count ({task_details['max_count']})")
    
    # reserved_amount should be >= 0
    if task_details["reserved_amount"] < 0:
        errors.append(f"Reserved amount should be >= 0, got {task_details['reserved_amount']}")
    
    # actual_consumption should be >= 0
    if task_details["actual_consumption"] < 0:
        errors.append(f"Actual consumption should be >= 0, got {task_details['actual_consumption']}")
    
    # settled_at should be set for completed tasks
    if task_details["settled_at"] is None:
        errors.append("settled_at should be set for completed task")
    
    # updated_at should be set
    if task_details["updated_at"] is None:
        errors.append("updated_at should be set")
    
    # updated_at should be >= created_at
    if task_details["updated_at"] and task_details["created_at"]:
        if task_details["updated_at"] < task_details["created_at"]:
            errors.append("updated_at should be >= created_at")
    
    return len(errors) == 0, errors
