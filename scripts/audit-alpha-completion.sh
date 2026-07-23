#!/usr/bin/env bash
# Fail-closed completion audit for the v0.1.1-alpha contract.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
matrix="$root/docs/plans/v0.1.1-alpha/contract-matrix.tsv"
report="${AURA_ALPHA_REPORT:-$root/target/alpha-reports/report.json}"

bash "$root/scripts/validate-alpha-contract.sh" >/dev/null

mapfile -t incomplete < <(awk -F '\t' 'NR > 1 && $7 != "implemented" { print $1 "\t" $7 "\t" $9 }' "$matrix")
if ((${#incomplete[@]} > 0)); then
  printf 'alpha completion audit: FAIL (%d incomplete contract row(s))\n' "${#incomplete[@]}" >&2
  printf '  %s\n' "${incomplete[@]}" >&2
  exit 1
fi

if [[ -f "$report" ]] && grep -q '"status":"deferred"' "$report"; then
  printf 'alpha completion audit: FAIL (harness report contains deferred stage)\n' >&2
  exit 1
fi

printf 'alpha completion audit: PASS (all contract rows implemented)\n'
