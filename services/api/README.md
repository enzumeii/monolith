# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust and Docker.

## Run locally

- `just setup` then access `.env` to update `JWT_SECRET`
- `just run` and access API server at `localhost:3000`

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `just test` | API native tests, Docker PostgreSQL, TCP E2E and retention tests; delete test DB afterward |
| `just check` | Formatting and strict API lint |
| `just fix` | Apply formatting/lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development PostgreSQL |
| `just reset-db` | Drop and recreate the configured database |

