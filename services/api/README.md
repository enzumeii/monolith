# API

## Prerequisites and setup

Rust 1.99.0 with Clippy, rustfmt and `wasm32-unknown-unknown`; just 1.58.0; SQLx CLI 0.9.0; running Docker with Compose; Python 3; Bash. Run `../../init.sh` to install the CLI packages after installing Rust and Docker.

From `services/api`:

```sh
just setup
```

`application.toml` holds listener/browser settings. Only `DATABASE_URL` and `JWT_SECRET` override settings from the environment; the signing key must be at least 32 bytes encoded as hex.

Setup creates `.env` if missing, including the local database URL and a random JWT signing key. It preserves an existing `.env`; it does not install prerequisites. Optional admin bootstrap requires psql.

## Run locally

```sh
just run
```

Starts Docker PostgreSQL on port 5432, applies migrations and runs the API at [http://127.0.0.1:3000](http://127.0.0.1:3000). Check [/healthz](http://127.0.0.1:3000/healthz). Set `DOCKER_CONTEXT` if your Docker installation needs a non-default context.

## Useful commands

| Command | Purpose |
| --- | --- |
| `just --list` | List recipes |
| `just test` | API native tests, Docker PostgreSQL, TCP E2E and retention tests; delete test DB afterward |
| `just check` | Formatting and strict API lint |
| `just fix` | Apply formatting/lint fixes and prepare SQLx metadata |
| `just e2e-db` | Start existing test PostgreSQL on port 5433 |
| `just db-down` | Stop development PostgreSQL |
| `just bootstrap-admin USERNAME ARGON2_HASH_FILE SUPER_ADMIN` | Create an admin from an operator-supplied Argon2 PHC hash file |
| `just reset-db` | Drop and recreate the configured database |

`just test` runs the API locally against Docker PostgreSQL and deletes the test database afterward, including on failure. `E2E_API_PORT` overrides its default API port 3100.
