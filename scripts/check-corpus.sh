#!/usr/bin/env bash
# Typecheck the green corpus (excludes corpus/diag/* expected failures).
# - Packages with aura.toml are checked as package roots
# - Standalone .aura files outside package trees are checked individually
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [[ -n "${AURA_BIN:-}" ]]; then
  bin="$AURA_BIN"
elif [[ -x target/debug/aura ]]; then
  bin=target/debug/aura
elif [[ -x target/release/aura ]]; then
  bin=target/release/aura
else
  bin=(cargo run -q -p aura-cli --)
fi

run_check() {
  if [[ -n "${AURA_BIN:-}" ]] || [[ -x target/debug/aura ]] || [[ -x target/release/aura ]]; then
    "$bin" check "$1"
  else
    cargo run -q -p aura-cli -- check "$1"
  fi
}

# Package roots
while IFS= read -r -d '' toml; do
  run_check "$(dirname "$toml")"
done < <(find corpus -name 'aura.toml' -print0 | sort -z)

# Standalone .aura files outside any package tree
while IFS= read -r -d '' f; do
  dir=$(dirname "$f")
  pkg=0
  p="$dir"
  while [[ "$p" != "corpus" && "$p" != "." && "$p" != "/" ]]; do
    if [[ -f "$p/aura.toml" ]]; then
      pkg=1
      break
    fi
    p=$(dirname "$p")
  done
  if [[ "$pkg" -eq 0 ]]; then
    run_check "$f"
  fi
done < <(find corpus -name '*.aura' ! -path 'corpus/diag/*' -print0 | sort -z)
