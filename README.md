# Enzumeii Monolith

## API Server [services/api]

### Pre-Requisites

- Docker
- Rust (cargo)

After cloning, run the init script once to install the dev CLI tools (just, sqlx-cli, bacon):

```bash
./init.sh
```

### Running locally

1. cd services/api
2. create `.env.local` contains
```
RUN_ENV = local
POSTGRES_URL = ...
JWT_SECRET = ...
```
3. run `just api`

### Formatter & Linter

1. `just check` to do format/lint check
2. `just fix` to do format/lint fix

### Testing & Verification

1. `just test` running both unit test and e2e test with postgreSQL database

### Useful command

1. recreate database locally `sqlx database drop -y && sqlx database create` [In case you need to restart the migration]
2. run migration script `sqlx migrate run`
3. create new migration script `sqlx migrate add ${migration name}`
4. `cargo sqlx prepare` to trigger SQL migration locally
