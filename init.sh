#!/usr/bin/env bash
# Install dev dependencies for the monolith repo.
# Run once after cloning:  ./init.sh
set -euo pipefail

# cargo is required to install everything else.
if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ cargo not found. Install Rust first: https://rustup.rs" >&2
  exit 1
fi

echo "→ Installing dev CLI tools (just, sqlx-cli, bacon)..."
cargo install just sqlx-cli bacon

echo ""
echo "✓ Done. Run 'just --list' to see the available commands."
