# List workspace CI utility commands; application recipes remain in their modules.
default:
    @just --list

# Install the pinned Conventional Commits reference parser (Node 24+ required).
setup:
    npm ci --prefix .github/scripts --ignore-scripts --no-audit --no-fund

# Validate GitHub workflows, Bash scripts, and PR/release/coverage behavior (Node, actionlint, shellcheck required).
check:
    actionlint
    shellcheck scripts/test-coverage.sh init.sh
    bash -n scripts/test-coverage.sh init.sh
    node --test .github/scripts/*.test.cjs
