#!/usr/bin/env bash
# Validate the reviewable positive/negative fixture split for C5.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="${1:-$root/docs/plans/v0.1.1-alpha/corpus-split.tsv}"

die() {
  printf 'corpus split validation: error: %s\n' "$*" >&2
  exit 1
}

[[ -f "$manifest" ]] || die "manifest not found: $manifest"
expected=$'id\tpositive_fixture\tnegative_fixture\tnote'
[[ "$(sed -n '1p' "$manifest")" == "$expected" ]] || die 'invalid TSV header'

declare -A seen=()
rows=0
line=1
while IFS=$'\t' read -r id positive negative note extra; do
  line=$((line + 1))
  [[ -n "${extra:-}" ]] && die "line $line: expected exactly four fields"
  [[ -n "$id" && -n "$positive" && -n "$negative" && -n "$note" ]] \
    || die "line $line: all fields are required"
  [[ -z "${seen[$id]:-}" ]] || die "line $line: duplicate id $id"
  seen["$id"]="$line"
  [[ "$positive" != "$negative" ]] || die "line $line: fixtures must differ"
  [[ "$positive" != /* && "$negative" != /* ]] \
    || die "line $line: fixtures must be repository-relative"
  [[ -e "$root/$positive" ]] || die "line $line: missing positive fixture $positive"
  [[ -e "$root/$negative" ]] || die "line $line: missing negative fixture $negative"
  rows=$((rows + 1))
done < <(tail -n +2 "$manifest")

(( rows > 0 )) || die 'manifest has no rows'
printf 'corpus split validation: PASS (%d rows)\n' "$rows"
