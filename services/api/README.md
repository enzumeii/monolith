# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust and Docker.

For Shopify calls, set the versioned URL at `client.shopify.admin_api_url` in `application.toml` and `SHOPIFY_ACCESS_TOKEN` in the ignored `.env`. Shopify credentials are optional for ordinary API startup and native tests.

## Run locally

- `just setup`
- `just run` and access API server at `localhost:3000`

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `RUST_LOG=info,api=debug just run` | Include API DEBUG events and original error diagnostics; error text can contain sensitive values |
| `just test` | API native tests, Docker PostgreSQL, TCP E2E and retention tests; delete test DB afterward |
| `just test-native` | Native API tests without Docker or Shopify access |
| `just check` | Formatting and strict API lint |
| `just fix` | Apply formatting/lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development PostgreSQL |
| `just reset-db` | Drop and recreate the configured database |

