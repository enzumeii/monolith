#!/usr/bin/env bash
set -euo pipefail
mode=${1:?Expected build or serve}
environment=${2:-local}
case "$mode" in build|serve) ;; *) echo 'Expected build or serve' >&2; exit 2 ;; esac
case "$environment" in local|qa|prod) ;; *) echo 'Expected local, qa or prod environment' >&2; exit 2 ;; esac
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root/apps/client-ui"
backup=$(mktemp)
cp application.toml "$backup"
restore_config() {
  cp "$backup" application.toml
  rm -f "$backup"
}
trap restore_config EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
node "$root/scripts/compose-vercel-ui-config.mjs" "$backup" "application.$environment.toml" application.toml
if [[ "$mode" == build ]]; then
  dx build --web --package client-ui
else
  dx serve --web --addr 127.0.0.1 --port 8085
fi
