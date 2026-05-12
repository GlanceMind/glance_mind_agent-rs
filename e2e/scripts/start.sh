#!/bin/bash
# =============================================================================
# E2E Test Runner Script
# =============================================================================
# Usage: ./scripts/start.sh
# 
# All tests run inside Docker Compose network - no host port exposure needed.
# =============================================================================

set -e

# Change to e2e directory
cd "$(dirname "$0")/.."

echo "=============================================="
echo "GlanceMind E2E Test Runner"
echo "=============================================="

# Check .env file
if [ ! -f .env ]; then
    echo "[ERROR] .env file not found!"
    echo ""
    echo "Please create .env from template:"
    echo "  cp .env.example .env"
    echo ""
    echo "Then fill in your API keys:"
    echo "  - TIKHUB_API_KEY"
    echo "  - DEEPSEEK_API_KEY"
    exit 1
fi

# Validate required API keys
source .env
if [ -z "$TIKHUB_API_KEY" ] || [ "$TIKHUB_API_KEY" = "your_tikhub_api_key_here" ]; then
    echo "[ERROR] TIKHUB_API_KEY is not set in .env"
    exit 1
fi

if [ -z "$DEEPSEEK_API_KEY" ] || [ "$DEEPSEEK_API_KEY" = "your_deepseek_api_key_here" ]; then
    echo "[ERROR] DEEPSEEK_API_KEY is not set in .env"
    exit 1
fi

echo "[OK] Environment variables validated"

# Cleanup old containers
echo ""
echo "[STEP 1] Cleaning up old containers..."
docker-compose down -v 2>/dev/null || true

# Build images
echo ""
echo "[STEP 2] Building Docker images..."
docker-compose build

# Start infrastructure services first (postgres, redis)
echo ""
echo "[STEP 3] Starting infrastructure services..."
docker-compose up -d postgres redis

# Wait for infrastructure to be healthy
echo ""
echo "[STEP 4] Waiting for infrastructure to be ready..."
echo "Waiting for PostgreSQL..."
until docker-compose exec -T postgres pg_isready -U ${POSTGRES_USER:-aihub_user} -d ${POSTGRES_DB:-aihub_e2e_db} > /dev/null 2>&1; do
    sleep 2
    echo "  Still waiting for PostgreSQL..."
done
echo "[OK] PostgreSQL is ready"

echo "Waiting for Redis..."
until docker-compose exec -T redis redis-cli ping > /dev/null 2>&1; do
    sleep 2
    echo "  Still waiting for Redis..."
done
echo "[OK] Redis is ready"

# Verify E2E campaign exists
echo "Verifying E2E campaign..."
docker-compose exec -T postgres psql -U ${POSTGRES_USER:-aihub_user} -d ${POSTGRES_DB:-aihub_e2e_db} -c \
    "SELECT id, name, status FROM gm_campaigns WHERE id = 99901" | grep -q "99901" && \
    echo "[OK] E2E campaign found" || \
    (echo "[ERROR] E2E campaign not found!" && exit 1)

# Start application services
echo ""
echo "[STEP 5] Starting application services (scheduler, agent-rs)..."
docker-compose up -d scheduler agent-rs

# Give services time to start
echo "Waiting for services to initialize..."
sleep 5

# Check scheduler is running
if docker-compose ps scheduler | grep -q "Up"; then
    echo "[OK] Scheduler is running"
else
    echo "[WARN] Scheduler may not be running properly"
    docker-compose logs scheduler | tail -20
fi

# Check agent-rs is running
if docker-compose ps agent-rs | grep -q "Up"; then
    echo "[OK] Agent-rs is running"
else
    echo "[WARN] Agent-rs may not be running properly"
    docker-compose logs agent-rs | tail -20
fi

# Run tests inside Docker Compose network
echo ""
echo "[STEP 6] Running E2E tests inside Docker..."
echo "=============================================="

# Run test-runner container
docker-compose run --rm test-runner

# Show results
TEST_EXIT_CODE=$?

echo ""
echo "=============================================="
if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo "E2E Test Complete - SUCCESS!"
else
    echo "E2E Test Complete - FAILED (exit code: $TEST_EXIT_CODE)"
fi
echo "=============================================="
echo ""
echo "To view logs:"
echo "  docker-compose logs -f scheduler"
echo "  docker-compose logs -f agent-rs"
echo ""
echo "To cleanup:"
echo "  ./scripts/cleanup.sh"

exit $TEST_EXIT_CODE
