# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust, Docker and curl (macOS/Linux, ARM64/x64).

`just setup` installs pinned standalone Biome for GraphQL/JSON formatting and creates `.env` when missing.

## Run locally

- `just setup`
- `just run` and access API server at `localhost:3000`

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `RUST_LOG=info,api=debug just run` | Include API DEBUG events and original error diagnostics; error text can contain sensitive values |
| `just test` | API native tests, Docker PostgreSQL, TCP E2E and retention tests; delete test DB afterward |
| `just check` | Rust/GraphQL/JSON formatting and strict API lint |
| `just format` | Format Rust and repository GraphQL/JSON files |
| `just format-check` | Check formatting without applying changes |
| `just setup-formatter` | Install only the pinned standalone formatter |
| `just fix` | Apply Rust/GraphQL/JSON formatting and lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development PostgreSQL |
| `just reset-db` | Drop and recreate the configured database |

