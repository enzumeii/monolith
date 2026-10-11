# Enzumeii Monolith

A Rust monolith: one HTTP API service plus two Dioxus fullstack apps over a shared domain crate.

- **`services/api`** — the backend. See [README](services/api/README.md) for setup, running, testing, and the available `just` commands.
- **`apps/client-ui`**, **`apps/backoffice`** — Dioxus fullstack apps.
- **`crates/domain`** — shared types (DTOs + DB entities) at the app ↔ api boundary.

Each service owns its own `justfile` and CI runs only that service's `check` +
`test` when its files change (see `.github/workflows/ci.yml`).

## Quick start (API)

```bash
./init.sh                          # installs just, sqlx-cli, bacon
cd services/api && just setup      # scaffold .env.local
cd services/api && just api-run    # start Postgres, run migrations, serve the API
```

## Commands

`services/api/justfile` (run from `services/api`):

- `just setup` — scaffold `services/api/.env.local`
- `just api-run` — start Postgres, run migrations, serve the API
- `just api-check` / `just api-fix` — API-only format/lint
- `just api-test` — API unit + e2e tests
- `just api-reset-db` — recreate the dev database
- `just api-db-down` — stop the dev Postgres

`apps/<app>/justfile` (run from `apps/backoffice` or `apps/client-ui`):

- `just app-check` / `just app-fix` — format/lint for that app
- `just app-test` — run that app's tests

See `services/api/README.md` for API setup details.
