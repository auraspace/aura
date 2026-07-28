#!/usr/bin/env bash
# Validate the historical v0.1.1-alpha contract matrix.
# This is an evidence audit for that published release, not a current CI gate.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
matrix="${1:-$root/docs/plans/v0.1.1-alpha/contract-matrix.tsv}"

die() {
  printf 'alpha contract validation: error: %s\n' "$*" >&2
  exit 1
}

[[ -f "$matrix" ]] || die "matrix not found: $matrix"

expected_header=$'id\tdomain\trfc\towner\tfixture\texecution_mode\tstatus\trelease_claim\treason'
header="$(sed -n '1p' "$matrix")"
[[ "$header" == "$expected_header" ]] || die 'invalid TSV header'

seen_ids=()
valid_statuses=' implemented partial blocked deferred out_of_scope '
row=1
rows=0

while IFS=$'\t' read -r id domain rfc owner fixture execution_mode status release_claim reason extra; do
  row=$((row + 1))
  [[ -z "${extra:-}" ]] || die "line $row: expected exactly 9 TSV fields"
  [[ -n "$id" ]] || die "line $row: missing id"
  [[ -n "$domain" ]] || die "line $row ($id): missing domain"
  [[ -n "$rfc" ]] || die "line $row ($id): missing rfc"
  [[ -n "$owner" ]] || die "line $row ($id): missing owner"
  [[ -n "$fixture" ]] || die "line $row ($id): missing fixture"
  [[ -n "$execution_mode" ]] || die "line $row ($id): missing execution_mode"
  [[ -n "$status" ]] || die "line $row ($id): missing status"
  [[ -n "$release_claim" ]] || die "line $row ($id): missing release_claim"
  [[ -n "$reason" ]] || die "line $row ($id): missing reason"
  if ((${#seen_ids[@]})); then
    for seen_id in "${seen_ids[@]}"; do
      [[ "$seen_id" != "$id" ]] || die "line $row: duplicate id: $id"
    done
  fi
  seen_ids+=("$id")
  [[ "$valid_statuses" == *" $status "* ]] || die "line $row ($id): invalid status: $status"
  [[ "$fixture" != /* && "$fixture" != *'..'* ]] \
    || die "line $row ($id): fixture must be repository-relative: $fixture"
  [[ -e "$root/$fixture" ]] || die "line $row ($id): fixture path not found: $fixture"
  rows=$((rows + 1))
done < <(tail -n +2 "$matrix")

(( rows > 0 )) || die 'matrix has no data rows'
printf 'alpha contract validation: PASS (%d rows)\n' "$rows"
