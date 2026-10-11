#!/usr/bin/env bash
# Install dev CLI tools for the monolith repo.
# Run once after cloning:  ./init.sh
set -euo pipefail

# cargo is required to install everything else.
if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ cargo not found. Install Rust first: https://rustup.rs" >&2
  exit 1
fi

echo "→ Installing dev CLI tools (just, sqlx-cli)..."
cargo install just sqlx-cli

echo ""
echo "✓ Done. Next steps:"
