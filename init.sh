#!/usr/bin/env bash
# Install dev dependencies and scaffold local config for the monolith repo.
# Run once after cloning:  ./init.sh
set -euo pipefail

# cargo is required to install everything else.
if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ cargo not found. Install Rust first: https://rustup.rs" >&2
  exit 1
fi

echo "→ Installing dev CLI tools (just, sqlx-cli, bacon)..."
cargo install just sqlx-cli bacon

# Scaffold services/api/.env.local if missing. The app reads DATABASE_URL and
# JWT_SECRET from env (the justfile loads this file via set dotenv-filename).
ENV_FILE="services/api/.env.local"
if [ ! -f "$ENV_FILE" ]; then
  echo "→ Scaffolding $ENV_FILE from template..."
  cat > "$ENV_FILE" <<'EOF'
RUN_ENV=local
DATABASE_URL=postgres://user:pass@127.0.0.1:5432/core
JWT_SECRET=change-me
EOF
  echo "  Edit $ENV_FILE and set a real JWT_SECRET before running the server."
else
  echo "→ $ENV_FILE already exists, leaving it untouched."
fi

echo ""
echo "✓ Done. Run 'just --list' to see the available commands."
