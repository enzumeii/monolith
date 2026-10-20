# Monolith

[![codecov](https://codecov.io/gh/enzumeii/monolith/graph/badge.svg?token=oVPc8gRQLl)](https://codecov.io/gh/enzumeii/monolith)

## Prerequisites

Install Rust and Docker, then run the prerequisite CLI installer from the repository root:

```sh
./init.sh
```

`init.sh` installs the pinned just, SQLx CLI and Dioxus CLI packages.
Rustup uses the repository's `rust-toolchain.toml` for Rust, Clippy, rustfmt and the WASM target.

| Module | Setup, local execution and commands |
| --- | --- |
| API | [services/api](services/api/README.md) |
| Backoffice | [apps/backoffice](apps/backoffice/README.md) |
| Client UI | [apps/client-ui](apps/client-ui/README.md) |
| Domain | [crates/domain](crates/domain/README.md) |

See [docs/](docs/) for architecture, development and verification documentation.
