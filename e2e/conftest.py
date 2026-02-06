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


# ============================================================================
# Enhanced Verification Functions (Campaign + Wallet + Transactions)
# ============================================================================

def get_campaign_details(conn, campaign_id):
    """Get full campaign details including financial fields."""
    with conn.cursor() as cur:
        cur.execute("""
            SELECT id, user_id, name, status, platform_id, keyword,
                   pending_consumption, actual_consumption, total_scanned,
                   budget_cap, is_frozen, created_at, updated_at
            FROM gm_campaigns WHERE id = %s
        """, (campaign_id,))
        row = cur.fetchone()
        if row:
            return {
                "id": row[0],
                "user_id": row[1],
                "name": row[2],
                "status": row[3],
                "platform_id": row[4],
                "keyword": row[5],
                "pending_consumption": float(row[6]) if row[6] else 0,
                "actual_consumption": float(row[7]) if row[7] else 0,
                "total_scanned": row[8] or 0,
                "budget_cap": float(row[9]) if row[9] else 0,
                "is_frozen": row[10],
                "created_at": row[11],
                "updated_at": row[12],
            }
        return None


def get_wallet_transactions(conn, user_id, reference_id=None, txn_type=None):
    """Get wallet transactions for user, optionally filtered by reference_id or type."""
    with conn.cursor() as cur:
        sql = """
            SELECT id, user_id, amount, type, reference_id, description, created_at
            FROM gm_wallet_transactions
            WHERE user_id = %s
        """
        params = [user_id]
        
        if reference_id is not None:
            sql += " AND reference_id = %s"
            params.append(reference_id)
        
        if txn_type is not None:
            sql += " AND type = %s"
            params.append(txn_type)
        
        sql += " ORDER BY created_at DESC"
        
        cur.execute(sql, params)
        rows = cur.fetchall()
        return [
            {
                "id": row[0],
                "user_id": row[1],
                "amount": float(row[2]) if row[2] else 0,
                "type": row[3],
                "reference_id": row[4],
                "description": row[5],
                "created_at": row[6],
            }
            for row in rows
        ]


def verify_campaign_financial_state(conn, campaign_id, expected_status=None):
    """
    Verify campaign financial state is consistent.
    Returns (is_valid, errors, details) tuple.
    """
    errors = []
    campaign = get_campaign_details(conn, campaign_id)
    
    if not campaign:
        return False, ["Campaign not found"], None
    
    # Check status if expected
    if expected_status and campaign["status"] != expected_status:
        errors.append(f"Expected status '{expected_status}', got '{campaign['status']}'")
    
    # pending_consumption should be >= 0
    if campaign["pending_consumption"] < 0:
        errors.append(f"pending_consumption should be >= 0, got {campaign['pending_consumption']}")
    
    # actual_consumption should be >= 0
    if campaign["actual_consumption"] < 0:
        errors.append(f"actual_consumption should be >= 0, got {campaign['actual_consumption']}")
    
    # total_scanned should be >= 0
    if campaign["total_scanned"] < 0:
        errors.append(f"total_scanned should be >= 0, got {campaign['total_scanned']}")
    
    # pending + actual should not exceed budget_cap
    total_consumption = campaign["pending_consumption"] + campaign["actual_consumption"]
    if campaign["budget_cap"] > 0 and total_consumption > campaign["budget_cap"] * 1.1:  # 10% tolerance
        errors.append(
            f"Total consumption ({total_consumption}) exceeds budget_cap ({campaign['budget_cap']})"
        )
    
    return len(errors) == 0, errors, campaign


def verify_wallet_transactions_for_campaign(conn, user_id, campaign_id):
    """
    Verify wallet transactions exist for campaign activation.
    Returns (is_valid, errors, transactions) tuple.
    """
    errors = []
    
    # Get FREEZE transaction (campaign activation)
    freeze_txns = get_wallet_transactions(conn, user_id, reference_id=campaign_id, txn_type='FREEZE')
    
    if len(freeze_txns) == 0:
        errors.append(f"No FREEZE transaction found for campaign {campaign_id}")
    else:
        freeze_txn = freeze_txns[0]
        if freeze_txn["amount"] >= 0:
            errors.append(f"FREEZE amount should be negative, got {freeze_txn['amount']}")
    
    return len(errors) == 0, errors, freeze_txns


def verify_wallet_transactions_for_task(conn, user_id, task_id):
    """
    Verify wallet transactions exist for task settlement.
    Returns (is_valid, errors, transactions) tuple.
    """
    errors = []
    
    # Get SETTLE transaction (task completion)
    settle_txns = get_wallet_transactions(conn, user_id, reference_id=task_id, txn_type='SETTLE')
    
    if len(settle_txns) == 0:
        errors.append(f"No SETTLE transaction found for task {task_id}")
    else:
        settle_txn = settle_txns[0]
        if settle_txn["amount"] > 0:
            errors.append(f"SETTLE amount should be non-positive, got {settle_txn['amount']}")
    
    return len(errors) == 0, errors, settle_txns


def verify_wallet_balance_accounting(conn, user_id, initial_balance=10000.0):
    """
    Verify wallet balance accounting is correct.
    balance + frozen should <= initial_balance (consumption reduces total)
    Returns (is_valid, errors, wallet_state) tuple.
    """
    errors = []
    wallet = get_wallet_balance(conn, user_id)
    
    if not wallet:
        return False, ["Wallet not found"], None
    
    balance, frozen = wallet
    balance = float(balance) if balance else 0
    frozen = float(frozen) if frozen else 0
    total = balance + frozen
    
    wallet_state = {
        "balance": balance,
        "frozen": frozen,
        "total": total,
        "initial_balance": initial_balance,
        "consumed": initial_balance - total,
    }
    
    # Total should not exceed initial balance
    if total > initial_balance * 1.01:  # 1% tolerance for rounding
        errors.append(
            f"Wallet total ({total}) exceeds initial balance ({initial_balance})"
        )
    
    # Balance should be >= 0
    if balance < 0:
        errors.append(f"Balance should be >= 0, got {balance}")
    
    # Frozen should be >= 0
    if frozen < 0:
        errors.append(f"Frozen should be >= 0, got {frozen}")
    
    return len(errors) == 0, errors, wallet_state


def get_task_consumption_summary(conn, campaign_id):
    """Get summary of all task consumptions for a campaign."""
    with conn.cursor() as cur:
        cur.execute("""
            SELECT 
                COUNT(*) as task_count,
                SUM(COALESCE(reserved_amount, 0)) as total_reserved,
                SUM(COALESCE(actual_consumption, 0)) as total_actual,
                SUM(COALESCE(process_count, 0)) as total_processed,
                COUNT(*) FILTER (WHERE status = 'completed') as completed_count,
                COUNT(*) FILTER (WHERE settled_at IS NOT NULL) as settled_count
            FROM gm_crawler_tasks
            WHERE campaign_id = %s
        """, (campaign_id,))
        row = cur.fetchone()
        if row:
            return {
                "task_count": row[0] or 0,
                "total_reserved": float(row[1]) if row[1] else 0,
                "total_actual": float(row[2]) if row[2] else 0,
                "total_processed": row[3] or 0,
                "completed_count": row[4] or 0,
                "settled_count": row[5] or 0,
            }
        return None
