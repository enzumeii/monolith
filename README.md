# Enzumeii Monolith

A Rust monolith: one HTTP API service plus two Dioxus fullstack apps (currently
stubs) over a shared domain crate.

- **`services/api`** — the backend. See `services/api/README.md` for setup,
  running, testing, and the available `just` commands.
- **`apps/client-ui`**, **`apps/backoffice`** — Dioxus fullstack apps (stubs).
- **`crates/domain`** — shared types (DTOs + DB entities) at the app↔api
  boundary.
- **`ARCHITECTURE.md`** — service graph and credential-isolation rules.

## Quick start

```bash
./init.sh                     # installs just, sqlx-cli, bacon; scaffolds services/api/.env.local
cd services/api && just api-run   # start Postgres, run migrations, serve the API
```

## Commands

Root `justfile` (run from the repo root) — workspace-wide:

- `just check` — format + lint check across the whole monorepo
- `just fix` — apply format + lint fixes across the whole monorepo
- `just test` — run all workspace tests (api e2e uses the compose Postgres)

`services/api/justfile` (run from `services/api`) — API-specific:

- `just api-run` — start Postgres, run migrations, serve the API
- `just api-check` / `just api-fix` — API-only format/lint
- `just api-test` — API unit + e2e tests
- `just api-reset-db` — recreate the dev database
- `just api-db-down` — stop the dev Postgres

See `services/api/README.md` for API setup details.
