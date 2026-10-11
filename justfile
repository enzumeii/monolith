# Default target
default:
    @just --list

# Check: format check + lint check across the whole workspace (no changes)
check:
    cargo fmt --check --all
    cargo clippy --workspace --all-targets -- -D warnings

# Fix: apply format fix + lint fix across the whole workspace
fix:
    cargo fmt --all
    cargo clippy --fix --workspace --allow-dirty --allow-staged -- -D warnings

# Test: run all workspace tests (unit + integration). The api e2e suite needs
# the compose Postgres, so bring it up first and tear down after.
test:
    docker compose -p e2e -f services/api/docker-compose-e2e.yml up -d --wait
    cargo test --workspace
    docker compose -p e2e -f services/api/docker-compose-e2e.yml down -v
