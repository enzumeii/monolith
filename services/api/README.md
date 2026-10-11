# API Service [services/api]

## Running locally

1. `./init.sh` (from the repo root — installs just, sqlx-cli)
2. `just setup` (scaffolds `services/api/.env.local`)
3. Edit `services/api/.env.local` and set a real `JWT_SECRET` (retrieve from the repo's secret store)
4. `just api-run` (starts Postgres, runs migrations, serves the API)

## Testing & Verification

1. `just api-test` — runs unit tests (no Docker) + e2e tests (Postgres via
   `docker-compose-e2e.yml`)

## Useful commands

All run from `services/api` and load `.env.local` (so `DATABASE_URL` is set).

- `just api-migrate` — run pending migration scripts (`sqlx migrate run`)
- `just api-migrate-add <name>` — create a new migration script (`sqlx migrate add <name>`)
- `just api-reset-db` — recreate the dev database (`sqlx database drop -y && sqlx database create`)
- `just api-prepare` — regenerate the offline sqlx query cache (`cargo sqlx prepare`)

`just api-fix` (format + lint fix) also runs `cargo sqlx prepare` at the end, so
the `.sqlx/` cache stays fresh whenever you fix lint locally. It needs the dev
Postgres up and migrated — run `just api-run` (or `just api-migrate`) first.

> If `just api-check` ever fails with *"no cached data for this query, run
> `cargo sqlx prepare`"*, a `query!` SQL string changed but the cache wasn't
> refreshed. Run `just api-fix` (or `just api-prepare`), then commit `.sqlx/`.
