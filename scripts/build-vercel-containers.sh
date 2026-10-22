#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker build --file Dockerfile.api.vercel --tag monolith-api:vercel-local .
