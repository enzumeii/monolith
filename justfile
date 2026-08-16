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
