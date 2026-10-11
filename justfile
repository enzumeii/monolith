set export := true
set dotenv-filename := "services/api/.env.local"

# Default target
default:
    @just --list

# Start Postgres, run migrations, and start the API service
api *args:
    @echo "🚀 Starting PostgreSQL container..."
    cd services/api && docker compose -f docker-compose.yml up -d

    @echo "⏳ Waiting for PostgreSQL to be ready..."
    @until cd services/api && docker compose -f docker-compose.yml exec -T postgres pg_isready -U postgres > /dev/null 2>&1; do \
        sleep 1; \
    done

    @echo "⚡ Running database migrations..."
    cd services/api && cargo sqlx migrate run

    @echo "🔥 Starting API service..."
    cd services/api && cargo run -- {{args}}

# Stop all database containers
db-down:
    cd services/api && docker compose -f docker-compose.yml down

# Run all tests: unit tests (no Docker) + e2e tests (Postgres via docker-compose-e2e.yml)
test:
	cargo test -p api --lib
	docker compose -p e2e -f services/api/docker-compose-e2e.yml up -d --wait
	cargo e2e
	docker compose -p e2e -f services/api/docker-compose-e2e.yml down -v

# Check: format check + lint check (no changes)
check:
	cargo fmt --check -p api
	cargo clippy -p api --all-targets -- -D warnings

# Format: apply format fix + lint fix
fix:
	cargo fmt -p api
	cargo clippy -p api --fix --allow-dirty --allow-staged -- -D warnings
