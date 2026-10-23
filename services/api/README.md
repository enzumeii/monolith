# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust, Docker, and Python 3. Python 3 is needed by `just setup` and `just check`.

Optionally install [Bitwarden CLI](https://bitwarden.com/help/cli/) with `npm install -g @bitwarden/cli`, then run `bw login`. `just setup --local` reads same-named credential custom fields from item `87323839-368f-4288-a4a5-9b0320699892`; `just setup --qa` uses item `0a291e2c-4ae4-436d-8730-ddf301a544d7`; a locked vault prompts for your master password in your terminal. Keep passwords and session tokens in your terminal, never in chat or `.env`.

Setup installs pinned dprint under `.tools/`, caches its plugins and writes the existing `.env` atomically with owner-only permissions. It sets `RUN_ENV` to the selected mode. Same-mode setup preserves nonempty values; switching modes replaces the imported credential allowlist from the selected item or defaults, while preserving database, S3, sender and unrelated values. The allowlist is `JWT_SECRET`, `CRON_SECRET`, `SHOPIFY_ACCESS_TOKEN`, `RESEND_API_KEY`, `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET` and `STRIPE_SECRET_KEY`. Without Bitwarden CLI, the same preservation/switch policy uses generated signing/cron defaults and empty optional provider defaults. Vault failures leave `.env` unchanged. Keep real sender profiles and S3 credentials private.

## Run locally

- `just setup` or `just setup --local` installs formatter tooling and selects local credentials in `.env`.
- `just setup --qa` selects QA credentials in the same `.env`.
- `just run` reads `RUN_ENV` from the environment or `.env` and selects `application.local.toml` or the required `application.qa.toml`; absent `RUN_ENV` defaults to local. Exported variables take precedence over `.env`.
- Local and QA start API at `localhost:3000`, local S3 at `localhost:3001` and allow browser origin `http://localhost:8084`. Stripe uses `https://api.stripe.com/v1/` in both modes; use the QA item's sandbox credential for test payments. See [sandbox exercise](../../docs/development.md#stripe-hosted-checkout). QA mode alone does not select a provider sandbox; the common PayPal URL remains production.
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
