# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust, Docker, Node.js 24 and npm.

`just setup` installs pinned GraphQL formatting tools and creates `.env` when missing.

## Run locally

- `just setup`
- `just run` and access API server at `localhost:3000`

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `RUST_LOG=info,api=debug just run` | Include API DEBUG events and original error diagnostics; error text can contain sensitive values |
| `just test` | API native tests, Docker PostgreSQL, TCP E2E and retention tests; delete test DB afterward |
| `just check` | Rust/GraphQL formatting and strict API lint |
| `just fix` | Apply Rust/GraphQL formatting and lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development PostgreSQL |
| `just reset-db` | Drop and recreate the configured database |

