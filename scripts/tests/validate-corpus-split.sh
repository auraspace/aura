#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

bash scripts/validate-corpus-split.sh

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-corpus-split.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
cp docs/plans/v0.1.1-alpha/corpus-split.tsv "$tmp/duplicate.tsv"
# Use a backup suffix so this works with both BSD and GNU sed.
sed -i.bak '3s/^COMP-002/COMP-001/' "$tmp/duplicate.tsv"
rm -f "$tmp/duplicate.tsv.bak"
if bash scripts/validate-corpus-split.sh "$tmp/duplicate.tsv" >/dev/null 2>&1; then
  printf 'corpus split tests: duplicate IDs were accepted\n' >&2
  exit 1
fi

printf 'corpus split tests: PASS\n'
