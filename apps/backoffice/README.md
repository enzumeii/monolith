# Backoffice

## Running locally

1. Start the internal API `cd services/api && just run`
2. Start the backoffice web app `cd apps/backoffice && just serve`
3. Open http://127.0.0.1:8084

## Commands

- `just setup` — scaffold `.env` once (never overwrites) and report prerequisites.
- `just build` — build the web client + BFF server bundle via `dx build`.
- `just run` — build then run the DX-bundled BFF server directly (no hot reload).
- `just test` — run the backoffice tests (config loading/validation).
```
