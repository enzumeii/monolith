# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust and Docker.

`just setup` installs pinned dprint with Cargo under `.tools/`, caches its GraphQL/JSON plugins, and creates `.env` when missing.

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
| `just fix` | Apply Rust/GraphQL/JSON formatting and lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development PostgreSQL |
| `just reset-db` | Drop and recreate the configured database |

