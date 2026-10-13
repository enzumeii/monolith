# API

Prerequisites: Rust 1.99.0, Docker Compose, just 1.58.0, SQLx CLI 0.9.0.

Run these commands from this directory:

```sh
just setup
just run
just test
just check
```

`just test` bootstraps the existing Docker PostgreSQL database, launches the API locally, waits for readiness, runs TCP endpoint tests, and stops that API process. It preserves the database and unrelated records. Set `DOCKER_CONTEXT` when using a non-default Docker context. Optional test port override: `E2E_API_PORT`.

Additional commands:

```sh
just e2e-db
just db-down
just bootstrap-admin USERNAME SUPER_ADMIN < password-input
```

Bootstrap reads a password from stdin and rejects duplicates. Keep input private; `password-input` is a placeholder for your input stream.
