# API Service [services/api]

## Running locally

2. `just setup` (scaffolds `services/api/.env.local`)
3. Edit `services/api/.env.local` and set a real `JWT_SECRET` get from repo's secret
4. `just api-run` (starts Postgres, runs migrations, serves the API)

## Testing & Verification

1. `just api-test` — runs unit tests (no Docker) + e2e tests (Postgres via
   `docker-compose-e2e.yml`)

## Useful commands

1. `just api-reset-db` — recreate the dev database (`sqlx database drop -y && sqlx database create`)
2. `sqlx migrate run` — run migration scripts
3. `sqlx migrate add ${migration name}` — create a new migration script
4. `cargo sqlx prepare` — regenerate the offline SQLx query cache
