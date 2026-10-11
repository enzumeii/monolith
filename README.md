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

1. Run `./init.sh` (installs dev tools and scaffolds `services/api/.env.local`)
2. Edit `services/api/.env.local` and set a real `JWT_SECRET`
3. Run `just api` (starts Postgres, runs migrations, serves the API)

`.env.local`:
```
RUN_ENV=local
DATABASE_URL=postgres://user:pass@127.0.0.1:5432/core
JWT_SECRET=change-me
```

### Formatter & Linter

1. `just check` to do format/lint check
2. `just fix` to do format/lint fix

### Testing & Verification

1. `just test` running both unit test and e2e test with postgreSQL database

### Useful command

1. `just reset-db` recreate the dev database (`sqlx database drop -y && sqlx database create`)
2. `sqlx migrate run` run migration scripts
3. `sqlx migrate add ${migration name}` create a new migration script
4. `cargo sqlx prepare` regenerate the offline SQLx query cache
