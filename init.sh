#!/usr/bin/env bash
# Install dev CLI tools for the monolith repo.
# Run once after cloning:  ./init.sh
set -euo pipefail

# cargo is required to install everything else.
if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ cargo not found. Install Rust first: https://rustup.rs" >&2
  exit 1
fi

echo "→ Installing dev CLI tools (just, sqlx-cli, dioxus-cli)..."
cargo install --locked just --version 1.58.0
cargo install --locked sqlx-cli --version 0.9.0
cargo install --locked dioxus-cli --version 0.7.10

echo ""
echo "✓ Done. Next steps:"
