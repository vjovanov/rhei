#!/usr/bin/env bash
set -euo pipefail

# Profile-guided-optimization build of the `rhei` release binary. The training
# run is intentionally read-only and covers the CLI paths used in local and CI
# loops: version, validate, list, render, states, templates, and next --peek.
# §FS-rhei-distribution.4
#
# Output: target/release/rhei, optimized against the merged profile.
# Requires: the `llvm-tools-preview` rustup component (`llvm-profdata`).

cd "$(dirname "$0")/.."
repo="$PWD"
pgo_dir="$repo/target/pgo-data"
profdata="$pgo_dir/merged.profdata"
host="$(rustc -vV | awk '/^host:/ { print $2 }')"

rustc_path() {
  local path="$1"
  if [[ "$host" == *windows* ]] && command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$path"
  else
    printf '%s\n' "$path"
  fi
}

llvm_profdata="$(find "$(rustc --print sysroot)" -type f -name 'llvm-profdata*' | head -n1)"
if [ -z "$llvm_profdata" ]; then
  echo "error: llvm-profdata not found; run: rustup component add llvm-tools-preview" >&2
  exit 1
fi

rm -rf "$pgo_dir"
mkdir -p "$pgo_dir"

pgo_dir_rustc="$(rustc_path "$pgo_dir")"
profdata_rustc="$(rustc_path "$profdata")"

echo "==> 1/3 build instrumented binary (-Cprofile-generate)"
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-generate=$pgo_dir_rustc" \
  cargo build --release --locked -p rhei-cli

exe_suffix=""
case "$host" in
  *windows*) exe_suffix=".exe" ;;
esac
rhei="$repo/target/release/rhei$exe_suffix"
plan="$repo/examples/spec-implementation-example/index.rhei.md"

echo "==> 2/3 training run"
set +e
for _ in 1 2 3; do
  "$rhei" version                                >/dev/null 2>&1
  "$rhei" validate "$plan"                      >/dev/null 2>&1
  "$rhei" list "$plan"                          >/dev/null 2>&1
  "$rhei" list "$plan" --json --ready           >/dev/null 2>&1
  "$rhei" render "$plan" --format json          >/dev/null 2>&1
  "$rhei" render "$plan" --format progress --no-color >/dev/null 2>&1
  "$rhei" states                                >/dev/null 2>&1
  "$rhei" states --json                         >/dev/null 2>&1
  "$rhei" templates --json                      >/dev/null 2>&1
  "$rhei" next "$plan" --peek                   >/dev/null 2>&1
done
set -e

shopt -s nullglob
profraws=("$pgo_dir"/*.profraw)
if [ ${#profraws[@]} -eq 0 ]; then
  echo "error: PGO training produced no .profraw files in $pgo_dir" >&2
  echo "       the instrumented '$rhei' did not run successfully" >&2
  exit 1
fi

"$llvm_profdata" merge -o "$profdata" "${profraws[@]}"

echo "==> 3/3 rebuild optimized (-Cprofile-use)"
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-use=$profdata_rustc -Cllvm-args=-pgo-warn-missing-function" \
  cargo build --release --locked -p rhei-cli

echo "==> done: $rhei"
"$rhei" version
