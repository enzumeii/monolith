# API Service

## Running locally

1. `just setup` (scaffolds `services/api/.env`)
2. Keep the generated `JWT_SECRET` private (hex encoding of at least 32 random bytes). Existing deployments must rotate older text/placeholder keys; users must sign in again.
3. `just run` (starts Postgres, runs migrations, serves the API)

## Testing & Verification

1. `just test` — runs unit tests (no Docker) + e2e tests (Postgres via `docker-compose-e2e.yml`)

## Useful commands

1. `just reset-db` — recreate the dev database (`sqlx database drop -y && sqlx database create`)
2. `sqlx migrate add ${migration name}` — create a new migration script

Usernames contain 1–20 Unicode characters without surrounding whitespace. New passwords contain 12–128 characters; existing shorter passwords still work at login. Invalid fields return 400 and duplicate usernames return 409.
