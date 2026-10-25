#!/usr/bin/env bash
set -euo pipefail
package=${1:?Expected client-ui or backoffice}
mode=${2:?Expected build or serve}
environment=${3:-${RUN_ENV:-local}}
case "$package" in client-ui) port=8085 ;; backoffice) port=8084 ;; *) echo 'Expected client-ui or backoffice' >&2; exit 2 ;; esac
case "$mode" in build|serve) ;; *) echo 'Expected build or serve' >&2; exit 2 ;; esac
if [[ ! "$environment" =~ ^[a-zA-Z0-9_-]+$ ]]; then echo 'Expected a valid RUN_ENV environment name' >&2; exit 2; fi
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root/apps/$package"
export RUN_ENV="$environment"
unset UI_CONFIG_OVERRIDE
if [[ "$mode" == build ]]; then
  exec dx build --web --package "$package"
else
  exec dx serve --web --addr 127.0.0.1 --port "$port"
fi
