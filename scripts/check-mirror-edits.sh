#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/check-mirror-edits.sh [--base <git-rev>]

Checks that mirrored Rust/TypeScript source areas are edited together in the
same change set.
EOF
}

BASE_REF=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --base)
      if [[ $# -lt 2 ]]; then
        echo "Missing value for --base" >&2
        usage
        exit 2
      fi
      BASE_REF="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

resolve_base_ref() {
  local requested="$1"
  if [[ -n "$requested" ]]; then
    if git rev-parse --verify "${requested}^{commit}" >/dev/null 2>&1; then
      printf '%s\n' "$requested"
      return 0
    fi
    echo "Warning: requested base ref is not available: $requested" >&2
  fi

  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    local remote_ref="origin/${GITHUB_BASE_REF}"
    if ! git rev-parse --verify "${remote_ref}^{commit}" >/dev/null 2>&1; then
      git fetch --no-tags --depth=1 origin "${GITHUB_BASE_REF}:${remote_ref}" >/dev/null 2>&1 || true
    fi
    if git rev-parse --verify "${remote_ref}^{commit}" >/dev/null 2>&1; then
      git merge-base HEAD "${remote_ref}"
      return 0
    fi
  fi

  if git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
    printf '%s\n' "HEAD~1"
    return 0
  fi

  return 1
}

if ! BASE_REF="$(resolve_base_ref "$BASE_REF")"; then
  echo "Mirror edit guard skipped: unable to determine a comparison base." >&2
  exit 0
fi

mapfile -t CHANGED_FILES < <(git diff --name-only --diff-filter=ACMR "${BASE_REF}...HEAD")
if [[ ${#CHANGED_FILES[@]} -eq 0 ]]; then
  echo "Mirror edit guard: no changed files in ${BASE_REF}...HEAD."
  exit 0
fi

matches_pattern() {
  local file="$1"
  local pattern="$2"
  if [[ "$pattern" == */ ]]; then
    [[ "$file" == "$pattern"* ]]
  else
    [[ "$file" == "$pattern" ]]
  fi
}

collect_hits() {
  local pattern="$1"
  local -n out_ref="$2"
  out_ref=()
  local file
  for file in "${CHANGED_FILES[@]}"; do
    if matches_pattern "$file" "$pattern"; then
      out_ref+=("$file")
    fi
  done
}

MIRROR_PAIRS=(
  "app|src/app/|ts/src/app/"
  "models|src/models/|ts/src/models/"
  "storage|src/storage/|ts/src/storage/"
  "portfolio|src/portfolio/|ts/src/portfolio/"
  "market_data|src/market_data/|ts/src/market-data/"
  "config|src/config.rs|ts/src/config.ts"
  "clock|src/clock.rs|ts/src/clock.ts"
  "duration|src/duration.rs|ts/src/duration.ts"
  "staleness|src/staleness.rs|ts/src/staleness.ts"
  "git|src/git.rs|ts/src/git.ts"
  "format|src/format.rs|ts/src/format/decimal.ts"
  "cli_entry|src/main.rs|ts/src/cli/main.ts"
)

# src/sync is intentionally excluded because TS sync support is currently partial.

violations=0
for pair in "${MIRROR_PAIRS[@]}"; do
  IFS='|' read -r pair_name rust_pattern ts_pattern <<< "$pair"

  rust_hits=()
  ts_hits=()
  collect_hits "$rust_pattern" rust_hits
  collect_hits "$ts_pattern" ts_hits

  if [[ ${#rust_hits[@]} -gt 0 && ${#ts_hits[@]} -eq 0 ]]; then
    violations=1
    echo "Mirror edit guard: '${pair_name}' changed only on Rust side."
    printf '  Rust changes:\n'
    printf '    - %s\n' "${rust_hits[@]}"
    echo "  Expected at least one matching TS change under '${ts_pattern}'."
  fi

  if [[ ${#ts_hits[@]} -gt 0 && ${#rust_hits[@]} -eq 0 ]]; then
    violations=1
    echo "Mirror edit guard: '${pair_name}' changed only on TypeScript side."
    printf '  TypeScript changes:\n'
    printf '    - %s\n' "${ts_hits[@]}"
    echo "  Expected at least one matching Rust change under '${rust_pattern}'."
  fi
done

if [[ $violations -ne 0 ]]; then
  cat <<'EOF'
Mirror edit guard failed.
If a one-sided change is intentional, either update the mirror implementation or
adjust the mirror map in scripts/check-mirror-edits.sh to reflect the new design.
EOF
  exit 1
fi

echo "Mirror edit guard passed."
