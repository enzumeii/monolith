# API

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust and Docker.

`just setup` installs pinned dprint with Cargo under `.tools/`, caches its GraphQL/JSON plugins, and creates `.env` when missing. It adds empty `RESEND_API_KEY`, `PAYPAL_CLIENT_ID` and `PAYPAL_CLIENT_SECRET` entries if absent. Supply optional provider credentials there; PayPal uses the sandbox URL in `application.toml` by default.

## Run locally

- `just setup` installs pinned dprint, caches plugins, and scaffolds `.env` with a random signing key and synthetic sender profile while preserving nonempty values.
- `just run` and access API server at `localhost:3000`

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `RUST_LOG=info,api=debug just run` | Include API DEBUG events and original error diagnostics; error text can contain sensitive values |
| `just test` | Start Docker PostgreSQL, run API/retention tests and TCP E2E once, then delete test DB |
| `just check` | Rust/GraphQL/JSON formatting and strict API lint |
| `just fix` | Apply Rust/GraphQL/JSON formatting and lint fixes and prepare SQLx metadata |
| `just db-down` | Stop development PostgreSQL |
| `just reset-db` | Drop and recreate the configured database |


## Coverage

`just test` runs the normal suite. `just test --coverage` writes `target/coverage/api/lcov.info` relative to the repository root. Install pinned cargo-llvm-cov via the root `init.sh`; the pinned Rust toolchain includes LLVM tools.
