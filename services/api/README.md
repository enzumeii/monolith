# API Service [services/api]

The sole credential holder and HTTP backend for the monolith. Pure-functional:
free functions over `&PgPool`, no domain traits/structs. See `../../ARCHITECTURE.md`
for the service graph and credential-isolation rules.

## Pre-Requisites

- Docker
- Rust (cargo)

After cloning the repo, run the root init script once to install the dev CLI
tools (just, sqlx-cli, bacon) and scaffold `.env.local`:

```bash
./init.sh
```

## Running locally

The `just` recipes live in this directory's `justfile`. Run them from here.

1. Run `./init.sh` (installs dev tools and scaffolds `services/api/.env.local`)
2. Edit `services/api/.env.local` and set a real `JWT_SECRET`
3. `just api-run` (starts Postgres, runs migrations, serves the API)

`.env.local`:
```
RUN_ENV=local
DATABASE_URL=postgres://user:pass@127.0.0.1:5432/core
JWT_SECRET=change-me
```

## Formatter & Linter

1. `just api-check` — format check + lint check
2. `just api-fix` — apply format + lint fixes

## Testing & Verification

1. `just api-test` — runs unit tests (no Docker) + e2e tests (Postgres via
   `docker-compose-e2e.yml`)

## Useful commands

1. `just api-reset-db` — recreate the dev database (`sqlx database drop -y && sqlx database create`)
2. `sqlx migrate run` — run migration scripts
3. `sqlx migrate add ${migration name}` — create a new migration script
4. `cargo sqlx prepare` — regenerate the offline SQLx query cache
