# Client App

## Prerequisites

Install Rust and Node.js 24, then run `../../init.sh` for pinned tools. `just setup` checks Dioxus and the WASM target.

## Run and verify

Run `just serve`, then open http://localhost:8085. Local public API settings are in `application.local.toml`; the command composes them with the shared baseline and restores it on exit.

| Command | Purpose |
| --- | --- |
| `just setup` | Check CLI and installed WASM target |
| `just build` | Build the local browser bundle |
| `just build prod` | Build with production public settings |
| `just serve qa` | Serve with QA public settings |
| `just test` | Run portal, cart, configuration and browser-bridge tests |
| `just check` | Check formatting and strict native/WASM lint |
| `just fix` | Apply formatting and lint fixes |
| `just test --coverage` | Write target/coverage/client-ui/lcov.info from repository root |
