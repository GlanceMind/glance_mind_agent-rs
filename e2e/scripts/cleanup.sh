#!/bin/bash
# =============================================================================
# E2E Test Cleanup Script
# =============================================================================
# Usage: ./scripts/cleanup.sh
# =============================================================================

set -e

# Change to e2e directory
cd "$(dirname "$0")/.."

echo "=============================================="
echo "GlanceMind E2E Cleanup"
echo "=============================================="

# Stop and remove containers
echo "[STEP 1] Stopping containers..."
docker-compose down -v 2>/dev/null || true

# Remove volumes
echo "[STEP 2] Removing volumes..."
docker volume rm e2e_postgres_data 2>/dev/null || true
docker volume rm e2e_redis_data 2>/dev/null || true

# Remove dangling images
echo "[STEP 3] Cleaning up images..."
docker image prune -f 2>/dev/null || true

echo ""
echo "=============================================="
echo "Cleanup complete!"
echo "=============================================="
