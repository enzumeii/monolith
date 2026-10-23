#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
version=0.7.10
toolchain=$(sed -n 's/^channel = "\([^"]*\)"/\1/p' "$root/rust-toolchain.toml")
: "${VERCEL_DX_ROOT:=$root/.tools/vercel-dx}"
export PATH="$VERCEL_DX_ROOT/bin:$PATH"
if ! command -v rustup >/dev/null 2>&1; then
  temporary=$(mktemp -d)
  trap 'rm -rf "$temporary"' EXIT
  curl --fail --location --retry 3 https://sh.rustup.rs --output "$temporary/rustup-init.sh"
  bash "$temporary/rustup-init.sh" -y --profile minimal --default-toolchain "$toolchain"
  export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi
rustup toolchain install "$toolchain" --profile minimal
rustup target add --toolchain "$toolchain" wasm32-unknown-unknown
if [[ -x "$VERCEL_DX_ROOT/bin/dx" ]] && [[ $("$VERCEL_DX_ROOT/bin/dx" --version) == "dioxus $version "* ]]; then
  echo "Using cached Dioxus CLI $version"
else
  # Compile on the build host so its libc, rather than a release-binary libc, defines compatibility.
  cargo +"$toolchain" install dioxus-cli --version "$version" --locked --root "$VERCEL_DX_ROOT"
fi
"$VERCEL_DX_ROOT/bin/dx" --version
