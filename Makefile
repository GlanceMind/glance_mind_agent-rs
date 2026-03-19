# =============================================================================
# GlanceMind Agent-rs Makefile
# =============================================================================

.PHONY: help dev dev-build dev-down dev-logs dev-logs-agent dev-logs-scheduler \
        dev-restart dev-restart-agent dev-restart-scheduler dev-ps dev-clean \
        build test clippy fmt check e2e e2e-instagram e2e-reddit e2e-twitter e2e-facebook e2e-clean

# Default target
help:
	@echo "GlanceMind Agent-rs Development Commands"
	@echo ""
	@echo "Development Environment (Redis + Scheduler + Agent-rs):"
	@echo "  make dev                 - Start dev environment (uses host DB)"
	@echo "  make dev-build           - Rebuild and start dev environment"
	@echo "  make dev-down            - Stop dev environment"
	@echo "  make dev-logs            - Follow all logs"
	@echo "  make dev-logs-agent      - Follow agent-rs logs"
	@echo "  make dev-logs-scheduler  - Follow scheduler logs"
	@echo "  make dev-restart         - Restart all services"
	@echo "  make dev-restart-agent   - Restart agent-rs only"
	@echo "  make dev-restart-scheduler - Restart scheduler only"
	@echo "  make dev-ps              - Show running containers"
	@echo "  make dev-clean           - Stop and remove all containers/images"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build               - Build release binary"
	@echo "  make test                - Run Rust tests (includes live API/DB gates)"
	@echo "  make clippy              - Run clippy linter"
	@echo "  make fmt                 - Format code"
	@echo "  make check               - Run all checks (fmt, clippy, test, e2e-twitter, e2e-facebook)"
	@echo ""
	@echo "E2E Tests:"
	@echo "  make e2e                 - Run TikTok E2E test"
	@echo "  make e2e-instagram       - Run Instagram E2E test"
	@echo "  make e2e-reddit          - Run Reddit E2E test"
	@echo "  make e2e-twitter         - Run Twitter E2E test"
	@echo "  make e2e-facebook        - Run Facebook E2E test"
	@echo "  make e2e-clean           - Clean up E2E environment"

# =============================================================================
# Development Environment Commands
# =============================================================================

# Start dev environment (detached)
dev:
	@echo "Starting development environment..."
	@echo "Using host.docker.internal for database connection"
	docker compose -f docker-compose.dev.yml up -d
	@echo ""
	@echo "Services started! Use 'make dev-logs' to follow logs"
	@echo "Redis:     localhost:6379"
	@echo "Scheduler: dev-scheduler"
	@echo "Agent-rs:  dev-agent-rs"

# Rebuild and start dev environment
dev-build:
	@echo "Rebuilding and starting development environment..."
	docker compose -f docker-compose.dev.yml up -d --build

# Stop dev environment
dev-down:
	docker compose -f docker-compose.dev.yml down

# Follow all logs
dev-logs:
	docker compose -f docker-compose.dev.yml logs -f

# Follow agent-rs logs only
dev-logs-agent:
	docker compose -f docker-compose.dev.yml logs -f agent-rs

# Follow scheduler logs only
dev-logs-scheduler:
	docker compose -f docker-compose.dev.yml logs -f scheduler

# Restart all services
dev-restart:
	docker compose -f docker-compose.dev.yml restart

# Restart agent-rs only
dev-restart-agent:
	docker compose -f docker-compose.dev.yml restart agent-rs

# Restart scheduler only
dev-restart-scheduler:
	docker compose -f docker-compose.dev.yml restart scheduler

# Show running containers
dev-ps:
	docker compose -f docker-compose.dev.yml ps

# Clean up dev environment completely
dev-clean:
	docker compose -f docker-compose.dev.yml down -v --rmi local

# =============================================================================
# Build & Test Commands
# =============================================================================

# Build release binary
build:
	cargo build --release

# Run unit tests
test:
	cargo test -- --test-threads=1

# Run clippy linter
clippy:
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Run all checks
check: fmt clippy test e2e-twitter e2e-facebook

# =============================================================================
# E2E Test Commands
# =============================================================================

# Run TikTok E2E test (default)
e2e:
	cd e2e && ./scripts/run_platform_test.sh tiktok

# Run Instagram E2E test
e2e-instagram:
	cd e2e && ./scripts/run_platform_test.sh instagram

# Run Reddit E2E test
e2e-reddit:
	cd e2e && ./scripts/run_platform_test.sh reddit

# Run Twitter E2E test
e2e-twitter:
	cd e2e && ./scripts/run_platform_test.sh twitter

# Run Facebook E2E test
e2e-facebook:
	cd e2e && ./scripts/run_platform_test.sh facebook

# Clean up E2E environment
e2e-clean:
	cd e2e && ./scripts/cleanup.sh
