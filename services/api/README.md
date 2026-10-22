# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust, Docker, and Python 3. Python 3 is needed by `just setup` and `just check`.

Optionally install [Bitwarden CLI](https://bitwarden.com/help/cli/) with `npm install -g @bitwarden/cli`, then run `bw login`. `just setup` reads same-named credential custom fields from item `87323839-368f-4288-a4a5-9b0320699892`; a locked vault prompts for your master password in your terminal. Keep passwords and session tokens in your terminal, never in chat or `.env`.

Without Bitwarden CLI, setup uses local defaults. Setup installs pinned dprint under `.tools/`, caches its plugins, and fills missing/empty `.env` settings while preserving nonempty values. Bitwarden imports only `JWT_SECRET`, `CRON_SECRET`, `SHOPIFY_ACCESS_TOKEN`, `RESEND_API_KEY`, `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`, and `STRIPE_SECRET_KEY`; clear an assignment before rerunning to replace it. Missing fields use local defaults: random `JWT_SECRET` and `CRON_SECRET` values, synthetic Blob token/sender profile, and empty optional provider credentials. Vault failures leave `.env` unchanged. Use your real sender profile and Vercel token before production use; PayPal defaults to its sandbox URL in `application.toml`.

## Run locally

- `just setup` installs formatter tooling and fills missing/empty `.env` settings from Bitwarden when installed, otherwise local defaults.
- `just run` starts API at `localhost:3000` and local Blob at `localhost:3001`.
- Edit `application.local.toml` scheduler `implementation` (`vps` or `vercel`) to choose local cron execution. `RUN_ENV=prod` uses production overrides and forces Vercel cron.

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `RUST_LOG=info,api=debug just run` | Include API DEBUG events and original error diagnostics; error text can contain sensitive values |
| `just test` | Start isolated PostgreSQL and Blob containers, run API and TCP E2E, then remove test resources |
| `just check` | Setup helper tests, Rust/GraphQL/JSON formatting, and strict API lint |
| `just fix` | Apply Rust/GraphQL/JSON formatting and lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development Compose services (preserve volumes) |
| `just reset-db` | Drop and recreate the configured database |

## Coverage

`just test` runs the normal suite. `just test --coverage` writes `target/coverage/api/lcov.info` relative to the repository root. Install pinned cargo-llvm-cov via the root `init.sh`; the pinned Rust toolchain includes LLVM tools.
