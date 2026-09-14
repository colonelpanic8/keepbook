#!/usr/bin/env bash
# Build the library under each declared feature in isolation.
#
# Default/all-feature builds cannot catch a module that imports a gated module
# without declaring the dependency, because some other feature happens to pull
# it in. Every feature listed here must build on its own.
set -euo pipefail

cd "$(dirname "$0")/.."

FEATURES=(
  app
  config
  credentials
  git
  market_data
  portfolio
  staleness
  sync
  tray
  tui
)

failures=()

run() {
  local label="$1"
  shift
  printf '==> %s\n' "$label"
  if ! "$@"; then
    failures+=("$label")
  fi
}

run "no-default-features" cargo check -p keepbook --lib --no-default-features
for feature in "${FEATURES[@]}"; do
  run "$feature" cargo check -p keepbook --lib --no-default-features --features "$feature"
done
run "cli" cargo check -p keepbook --lib --no-default-features --features cli

if ((${#failures[@]})); then
  printf '\nfeature matrix failures: %s\n' "${failures[*]}" >&2
  exit 1
fi

printf '\nfeature matrix ok\n'
