#!/usr/bin/env python3
"""
Wait for all E2E services to be ready.
"""
import os
import sys
import time
import socket
import psycopg2
import redis
from dotenv import load_dotenv

# Load environment
load_dotenv()

# Configuration
POSTGRES_HOST = os.getenv("POSTGRES_HOST", "localhost")
POSTGRES_PORT = int(os.getenv("POSTGRES_PORT", "5433"))
POSTGRES_DB = os.getenv("POSTGRES_DB", "aihub_e2e_db")
POSTGRES_USER = os.getenv("POSTGRES_USER", "aihub_user")
POSTGRES_PASSWORD = os.getenv("POSTGRES_PASSWORD", "aihub_password")

REDIS_HOST = os.getenv("REDIS_HOST", "localhost")
REDIS_PORT = int(os.getenv("REDIS_PORT", "6380"))

MAX_WAIT = 120  # Maximum wait time in seconds


def wait_for_port(host, port, name, timeout=60):
    """Wait for a port to be accessible."""
    start = time.time()
    while time.time() - start < timeout:
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
                sock.settimeout(5)
                result = sock.connect_ex((host, port))
                if result == 0:
                    print(f"[OK] {name} is accessible on {host}:{port}")
                    return True
        except Exception as e:
            pass
        print(f"[WAIT] Waiting for {name} ({host}:{port})...")
        time.sleep(2)
    return False


def wait_for_postgres():
    """Wait for PostgreSQL to be ready."""
    print("\nChecking PostgreSQL...")
    
    if not wait_for_port(POSTGRES_HOST, POSTGRES_PORT, "PostgreSQL"):
        return False
    
    # Try to connect
    for attempt in range(30):
        try:
            conn = psycopg2.connect(
                host=POSTGRES_HOST,
                port=POSTGRES_PORT,
                database=POSTGRES_DB,
                user=POSTGRES_USER,
                password=POSTGRES_PASSWORD,
            )
            
            # Check if tables exist
            with conn.cursor() as cur:
                cur.execute("SELECT COUNT(*) FROM gm_platforms")
                count = cur.fetchone()[0]
                if count > 0:
                    print(f"[OK] PostgreSQL ready with {count} platforms")
                    conn.close()
                    return True
            
            conn.close()
        except Exception as e:
            print(f"[WAIT] PostgreSQL not ready: {e}")
        
        time.sleep(2)
    
    return False


def wait_for_redis():
    """Wait for Redis to be ready."""
    print("\nChecking Redis...")
    
    if not wait_for_port(REDIS_HOST, REDIS_PORT, "Redis"):
        return False
    
    for attempt in range(30):
        try:
            client = redis.Redis(host=REDIS_HOST, port=REDIS_PORT)
            client.ping()
            print("[OK] Redis ready")
            client.close()
            return True
        except Exception as e:
            print(f"[WAIT] Redis not ready: {e}")
        
        time.sleep(2)
    
    return False


def wait_for_campaign():
    """Wait for E2E campaign to be created."""
    print("\nChecking E2E campaign...")
    
    for attempt in range(30):
        try:
            conn = psycopg2.connect(
                host=POSTGRES_HOST,
                port=POSTGRES_PORT,
                database=POSTGRES_DB,
                user=POSTGRES_USER,
                password=POSTGRES_PASSWORD,
            )
            
            with conn.cursor() as cur:
                cur.execute("SELECT id, name, status FROM gm_campaigns WHERE id = 99901")
                row = cur.fetchone()
                if row:
                    print(f"[OK] E2E campaign found: id={row[0]}, name={row[1]}, status={row[2]}")
                    conn.close()
                    return True
            
            conn.close()
        except Exception as e:
            print(f"[WAIT] Campaign not ready: {e}")
        
        time.sleep(2)
    
    return False


def main():
    """Main entry point."""
    print("=" * 50)
    print("Waiting for E2E services to be ready...")
    print("=" * 50)
    
    start = time.time()
    
    # Check PostgreSQL
    if not wait_for_postgres():
        print("\n[ERROR] PostgreSQL failed to start!")
        sys.exit(1)
    
    # Check Redis
    if not wait_for_redis():
        print("\n[ERROR] Redis failed to start!")
        sys.exit(1)
    
    # Check campaign
    if not wait_for_campaign():
        print("\n[ERROR] E2E campaign not found!")
        sys.exit(1)
    
    elapsed = time.time() - start
    print("\n" + "=" * 50)
    print(f"All services ready! (took {elapsed:.1f}s)")
    print("=" * 50)


if __name__ == "__main__":
    main()
