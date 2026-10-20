# List workspace CI utility commands; application recipes remain in their modules.
default:
    @just --list

# Validate GitHub workflows, Bash scripts, and PR/release/coverage behavior (Node, actionlint, shellcheck required).
check:
    actionlint
    shellcheck scripts/test-coverage.sh init.sh
    bash -n scripts/test-coverage.sh init.sh
    node --test .github/scripts/*.test.cjs
