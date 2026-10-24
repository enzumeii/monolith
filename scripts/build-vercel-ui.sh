#!/usr/bin/env bash
set -euo pipefail
package=${1:?Expected UI package}
prefix=${2:?Expected public prefix}
config=${3:-application.prod.toml}
root=$(cd "$(dirname "$0")/.." && pwd)
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
case "$package:$prefix" in
  backoffice:backoffice|client-ui:root) ;;
  *) echo 'Unknown UI package or public prefix' >&2; exit 2 ;;
esac
if command -v dx >/dev/null 2>&1 && [[ $(dx --version 2>/dev/null) == "dioxus 0.7.10 "* ]]; then
  dx_command=$(command -v dx)
else
  bash "$root/scripts/install-vercel-dx.sh"
  dx_command="${VERCEL_DX_ROOT:-$root/.tools/vercel-dx}/bin/dx"
fi
cd "$root/apps/$package"
test -f "$config"
backup=$(mktemp)
cp application.toml "$backup"
restore_config() {
  cp "$backup" application.toml
  rm -f "$backup"
}
trap restore_config EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
node "$root/scripts/compose-vercel-ui-config.mjs" "$backup" "$config" application.toml
export CARGO_TARGET_DIR="$PWD/target/vercel"
bundle="$CARGO_TARGET_DIR/dx/$package/release/web/public"
rm -rf "${bundle%/public}"
base_path="$prefix"
output="vercel/public/$prefix"
if [[ "$prefix" == root ]]; then
  base_path=/
  output=vercel/public
fi
"$dx_command" build --web --release --package "$package" --base-path "$base_path" --debug-symbols false --locked
test -f "$bundle/index.html"
test -f "$bundle/assets/application.toml"
rm -rf vercel/public
mkdir -p "$output"
cp -R "$bundle/." "$output/"
