# Backoffice App

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust.

## Run locally

- `just setup` to initiate `.env`
- Start the API using its [README](../../services/api/README.md) or `cd ../../services/api && just run`
- Start Backoffice client `just serve`
- Open [http://localhost:8084](http://localhost:8084).

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `just build` | Build the client web bundle |
| `just test` | Shared contracts, domain WASM check and native backoffice tests |
| `just check` | Formatting and strict native/WASM lint |
| `just fix` | Apply formatting and native/WASM lint fixes |
