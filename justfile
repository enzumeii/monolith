# List workspace CI utility commands; application recipes remain in their modules.
default:
    @just --list

# Install the pinned Conventional Commits reference parser (Node 24+ required).
setup:
    npm ci --prefix .github/scripts --ignore-scripts --no-audit --no-fund
    npm ci --ignore-scripts --no-audit --no-fund

# Validate GitHub workflows, Bash scripts, and PR/release/coverage behavior (Node, actionlint, shellcheck required).
check:
    actionlint
    shellcheck scripts/*.sh init.sh
    for script in scripts/*.sh init.sh; do bash -n "$script"; done
    node --test .github/scripts/*.test.cjs
    npm run check:vercel

# Build both static UI bundles and the API image locally (Rust, dx and Docker required).
vercel-build:
    bash scripts/build-vercel-ui.sh backoffice backoffice
    bash scripts/build-vercel-ui.sh client-ui root
    bash scripts/build-vercel-containers.sh

# Run all services on port 18027 using API .env and local TOML cron selection.
vercel-dev *args:
    npm run dev:vercel -- {{args}}
