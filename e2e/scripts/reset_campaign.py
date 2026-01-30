#!/usr/bin/env python3
"""
Reset E2E campaign to DRAFT status for re-running tests.
"""
import os
import psycopg2
from dotenv import load_dotenv

load_dotenv()

# Configuration
DB_CONFIG = {
    "host": os.getenv("POSTGRES_HOST", "localhost"),
    "port": int(os.getenv("POSTGRES_PORT", "5433")),
    "database": os.getenv("POSTGRES_DB", "aihub_e2e_db"),
    "user": os.getenv("POSTGRES_USER", "aihub_user"),
    "password": os.getenv("POSTGRES_PASSWORD", "aihub_password"),
}

E2E_CAMPAIGN_ID = 99901
E2E_USER_ID = 99999


def reset_campaign():
    """Reset campaign to DRAFT status."""
    conn = psycopg2.connect(**DB_CONFIG)
    conn.autocommit = True
    
    try:
        with conn.cursor() as cur:
            # Delete existing tasks and results
            print("[STEP 1] Deleting existing tasks and results...")
            
            # Delete comments
            cur.execute("""
                DELETE FROM gm_agent_comments
                WHERE video_db_id IN (
                    SELECT id FROM gm_agent_videos WHERE campaign_id = %s
                )
            """, (E2E_CAMPAIGN_ID,))
            
            # Delete videos
            cur.execute("DELETE FROM gm_agent_videos WHERE campaign_id = %s", (E2E_CAMPAIGN_ID,))
            
            # Delete tasks
            cur.execute("DELETE FROM gm_crawler_tasks WHERE campaign_id = %s", (E2E_CAMPAIGN_ID,))
            
            # Reset campaign
            print("[STEP 2] Resetting campaign...")
            cur.execute("""
                UPDATE gm_campaigns
                SET status = 'DRAFT',
                    is_frozen = false,
                    pending_consumption = 0,
                    actual_consumption = 0,
                    total_scanned = 0,
                    completed_reason = NULL,
                    updated_at = NOW()
                WHERE id = %s
            """, (E2E_CAMPAIGN_ID,))
            
            # Reset wallet
            print("[STEP 3] Resetting wallet...")
            cur.execute("""
                UPDATE gm_user_wallets
                SET balance_points = 10000.00,
                    frozen_points = 0.00,
                    updated_at = NOW()
                WHERE user_id = %s
            """, (E2E_USER_ID,))
            
            # Delete transaction logs
            cur.execute("""
                DELETE FROM gm_wallet_transactions
                WHERE user_id = %s AND reference_id = %s
            """, (E2E_USER_ID, E2E_CAMPAIGN_ID))
            
            print("\n[OK] Campaign reset successfully!")
            print(f"  Campaign ID: {E2E_CAMPAIGN_ID}")
            print(f"  Status: DRAFT")
            print(f"  Wallet Balance: 10000.00")
            
    finally:
        conn.close()


if __name__ == "__main__":
    reset_campaign()
