#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
validator="$root/scripts/validate-sanitizer-seeds.sh"
manifest="$root/runtime/tests/sanitizer-seeds.tsv"

bash "$validator" >/dev/null

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-sanitizer-seeds.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
cp "$manifest" "$tmp/duplicate.tsv"
awk 'NR == 2 { print; }' "$manifest" >> "$tmp/duplicate.tsv"
if SANITIZER_SEEDS_MANIFEST="$tmp/duplicate.tsv" bash "$validator" >/dev/null 2>&1; then
  printf 'sanitizer seeds test: duplicate fixture accepted\n' >&2
  exit 1
fi

printf 'sanitizer seeds tests: PASS\n'
