#!/bin/bash
# Run E2E test for a specific platform
# Usage: ./scripts/run_platform_test.sh [platform]
# Platforms: tiktok, instagram, reddit, twitter

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
    *)
        echo "Unknown platform: $PLATFORM"
        echo "Available platforms: tiktok, instagram, reddit, twitter"
        exit 1
        ;;
esac

echo "=========================================="
echo "Running E2E Test for: $PLATFORM"
echo "Test file: $TEST_FILE"
echo "=========================================="

# Export the test file for docker-compose
export TEST_FILE

# Run the test using docker-compose
docker compose run --rm -e TEST_FILE=$TEST_FILE test-runner
