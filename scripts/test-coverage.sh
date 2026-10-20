#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
crate=${1:?Expected crate name}
mode=${2:-}
case "$crate" in
  api) module=services/api; excluded='apps/|crates/domain/';;
  backoffice) module=apps/backoffice; excluded='services/|apps/client-ui/|crates/domain/';;
  client-ui) module=apps/client-ui; excluded='services/|apps/backoffice/|crates/domain/';;
  domain) module=crates/domain; excluded='services/|apps/';;
  *) echo "Unknown crate: $crate" >&2; exit 2;;
esac
if [[ $# -gt 2 || ( -n "$mode" && "$mode" != --coverage ) ]]; then
  echo 'Usage: just test [--coverage]' >&2
  exit 2
fi
if [[ -z "$mode" ]]; then
  exec just --justfile "$root/$module/justfile" _test
fi
cd "$root"
report_dir="$root/target/coverage/$crate"
mkdir -p "$report_dir"
rm -f "$report_dir/lcov.info"
cleanup_report() {
  if [[ $? -ne 0 ]]; then rm -f "$report_dir/lcov.info"; fi
}
trap cleanup_report EXIT
export CARGO_LLVM_COV_TARGET_DIR="${CARGO_LLVM_COV_TARGET_DIR:-$report_dir/build}"
export CARGO_TARGET_DIR="$CARGO_LLVM_COV_TARGET_DIR"
coverage_env=$(cargo llvm-cov show-env --sh --remap-path-prefix)
eval "$coverage_env"
cargo llvm-cov clean --workspace
just --justfile "$root/$module/justfile" _test
cargo llvm-cov report --locked --lcov --package "$crate" \
  --ignore-filename-regex "($excluded|/tests/|/e2e-test/)" \
  --output-path "$report_dir/lcov.info"
test -s "$report_dir/lcov.info"
printf 'Coverage: %s\n' "$report_dir/lcov.info"
