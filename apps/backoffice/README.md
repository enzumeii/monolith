# Backoffice

## Running locally

1. `just setup` — scaffold `.env` once (never overwrites) and report prerequisites.
2. Start the internal API `cd services/api && just run`
3. Start the backoffice web app `cd apps/backoffice && just serve` then open <http://127.0.0.1:8084>

## Useful commands

- `just build` — build the web client + BFF server bundle via `dx build`.
