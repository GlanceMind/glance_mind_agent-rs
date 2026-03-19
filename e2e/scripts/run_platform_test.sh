#!/bin/bash
# Run E2E test for a specific platform
# Usage: ./scripts/run_platform_test.sh [platform]
# Platforms: tiktok, instagram, reddit, twitter, facebook

set -e

PLATFORM=${1:-tiktok}

case $PLATFORM in
    tiktok)
        TEST_FILE="test_e2e_china_travel.py"
        ;;
    instagram)
        TEST_FILE="test_e2e_instagram.py"
        ;;
    reddit)
        TEST_FILE="test_e2e_reddit.py"
        ;;
    twitter)
        TEST_FILE="test_e2e_twitter.py"
        ;;
    facebook)
        TEST_FILE="test_e2e_facebook.py"
        ;;
    *)
        echo "Unknown platform: $PLATFORM"
        echo "Available platforms: tiktok, instagram, reddit, twitter, facebook"
        exit 1
        ;;
esac

if [ "$PLATFORM" = "twitter" ]; then
    export TIKHUB_BASE_URL="http://tikhub-mock:8500"
    export TIKHUB_API_KEY="${TIKHUB_API_KEY:-mock-tikhub-key}"
fi

echo "=========================================="
echo "Running E2E Test for: $PLATFORM"
echo "Test file: $TEST_FILE"
echo "=========================================="

echo ""
echo "[STEP 1] Recreating E2E services..."
docker compose down -v --remove-orphans >/dev/null 2>&1 || true
docker compose up -d --build postgres redis tikhub-mock facebook-scraper-mock laozhang-mock scheduler agent-rs test-runner

echo ""
echo "[STEP 2] Waiting for test-runner dependencies..."
until docker compose exec -T test-runner /bin/sh -lc "python -m pytest --version" >/dev/null 2>&1; do
    sleep 2
done

echo ""
echo "[STEP 3] Running pytest in test-runner..."
docker compose exec -T test-runner python -m pytest "$TEST_FILE" -v --tb=short
