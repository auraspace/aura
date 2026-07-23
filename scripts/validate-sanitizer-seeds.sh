#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="${SANITIZER_SEEDS_MANIFEST:-$root/runtime/tests/sanitizer-seeds.tsv}"

[[ -f "$manifest" ]] || { printf 'sanitizer seeds: missing manifest\n' >&2; exit 1; }

awk -F '\t' '
  NR == 1 {
    if ($0 != "fixture\tseed\tminimized_case\tcommand") exit 2
    next
  }
  NF != 4 { exit 3 }
  $1 == "" || $2 !~ /^[0-9]+$/ || $3 == "" || $4 == "" { exit 4 }
  { seen[$1]++ }
  { if (seen[$1] > 1) { exit 5 } }
  { if (index($4, $1 ".c") == 0) { exit 6 } }
  { count++ }
  END { if (count == 0) exit 7 }
' "$manifest" || {
  printf 'sanitizer seeds: invalid manifest: %s\n' "$manifest" >&2
  exit 1
}

while IFS=$'\t' read -r fixture seed minimized command; do
  [[ "$fixture" == "fixture" ]] && continue
  [[ -f "$root/$minimized" ]] || {
    printf 'sanitizer seeds: missing minimized fixture: %s\n' "$minimized" >&2
    exit 1
  }
  printf 'sanitizer seed: %s=%s (%s)\n' "$fixture" "$seed" "$minimized"
done < "$manifest"

printf 'sanitizer seeds: manifest valid\n'
