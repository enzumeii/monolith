# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust, Docker, and Python 3. Python 3 is needed by `just setup` and `just check`.

Optionally install [Bitwarden CLI](https://bitwarden.com/help/cli/) with `npm install -g @bitwarden/cli`, then run `bw login`. `just setup --local` reads same-named credential custom fields from item `87323839-368f-4288-a4a5-9b0320699892`; `just setup --qa` uses item `0a291e2c-4ae4-436d-8730-ddf301a544d7`; a locked vault prompts for your master password in your terminal. Keep passwords and session tokens in your terminal, never in chat or `.env`.

Setup installs pinned dprint under `.tools/`, caches its plugins and writes ignored `.env` atomically with owner-only permissions. It selects `RUN_ENV` and imports provider credentials plus the three `CHECKOUT_*` fields from the selected item. Same-mode setup preserves nonempty values; switching modes replaces imported fields. Without Bitwarden CLI, setup uses safe defaults; vault failures leave `.env` unchanged. See [configuration and imported fields](../../docs/development.md#configuration).

## Run locally

- `just setup` or `just setup --local` installs formatter tooling and selects local credentials in `.env`.
- `just setup --qa` selects QA credentials in the same `.env`.
- `just run` reads `RUN_ENV` from the environment or `.env` and selects `application.local.toml` or the required `application.qa.toml`; absent `RUN_ENV` defaults to local. Exported variables take precedence over `.env`.
- Local and QA start API at `localhost:3000`, local S3 at `localhost:3001` and allow backoffice `http://localhost:8084` and customer UI `http://localhost:8085`. QA enables checkout with Stripe test mode and PayPal Sandbox; matching credentials are required. See [checkout configuration](../../docs/development.md#checkout-configuration-values) and the [sandbox exercise](../../docs/development.md#stripe-hosted-checkout).
- Edit the selected overlay's scheduler `implementation` (`vps` or `vercel`) to choose cron execution. `RUN_ENV=prod` reads `application.prod.toml`, which selects Vercel cron.

## Vercel cron timing

Vercel calls `/_cron` daily at **07:00 Bangkok time**. Report schedules also use Bangkok time: occurrences due by that invocation are queued for processing; those due afterward wait until the next day's trigger, rather than being discarded. Failures or invocation capacity can delay processing further.

For example, `0 0 1 * *` (midnight on the 1st) runs around 07:00 on the 1st, while `0 8 1 * *` (08:00 on the 1st) runs around 07:00 on the 2nd. This daily cutoff applies to Vercel mode; VPS mode polls every 10 seconds. See [cron operations](../../docs/cron.md).

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `RUST_LOG=info,api=debug just run` | Include API DEBUG events and original error diagnostics; error text can contain sensitive values |
| `just test` | Start isolated PostgreSQL and private S3 containers, run API and TCP E2E, then remove test resources |
| `just check` | Setup helper tests, Rust/GraphQL/JSON formatting, and strict API lint |
| `just fix` | Apply Rust/GraphQL/JSON formatting and lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development Compose services (preserve volumes) |
| `just reset-db` | Drop and recreate the configured database |

## Coverage

`just test` runs the normal suite. `just test --coverage` writes `target/coverage/api/lcov.info` relative to the repository root. Install pinned cargo-llvm-cov via the root `init.sh`; the pinned Rust toolchain includes LLVM tools.
