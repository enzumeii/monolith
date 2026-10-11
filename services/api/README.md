# API Service [services/api]

## Running locally

1. `just setup` (scaffolds `services/api/.env.local`)
2. Edit `.env.local` and set a real `JWT_SECRET` get from repo's secret
3. `just run` (starts Postgres, runs migrations, serves the API)

## Formatting & Linting

1. `just check`
2. `just fix`

## Testing & Verification

1. `just test` — runs unit tests (no Docker) + e2e tests (Postgres via `docker-compose-e2e.yml`)

## Useful commands

1. `just reset-db` — recreate the dev database (`sqlx database drop -y && sqlx database create`)
2. `sqlx migrate add ${migration name}` — create a new migration script
