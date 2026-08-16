# Enzumeii Monolith

## Pre-Requisites

- Docker
- Rust crates (just, sqlx-cli, bacon)

```bash
    cargo install just sqlx-cli bacon
```

## Useful command

1. lint/format check `bacon`
2. format code `bacon fmt`
3. lint fix `bacon clippy-fix`

## API Server [services/api]

### Running locally

1. cd services/api
2. create `.env.local` consisting of

```
RUN_ENV = local
POSTGRES_URL = ...
JWT_SECRET = ...
```

1. run `just api`

### Useful command

1. recreate database locally `sqlx database drop -y && sqlx database create` [In case you need to restart the migration]
2. run migration script `sqlx migrate run`
3. create new migration script `sqlx migrate add ${migration name}`
4. `cargo sqlx prepare` to trigger SQL migration locally
