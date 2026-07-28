#!/usr/bin/env bash
# Historical completion audit for the published v0.1.1-alpha contract.
# Release-infrastructure CI uses generic policy/bundle checks instead.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
matrix="$root/docs/plans/v0.1.1-alpha/contract-matrix.tsv"
report="${AURA_ALPHA_REPORT:-$root/target/alpha-reports/report.json}"
profile="full"

if [[ "${1:-}" == "--profile" ]]; then
  [[ $# -eq 2 ]] || { printf 'usage: %s [--profile bounded|full]\n' "$0" >&2; exit 2; }
  profile="$2"
fi
case "$profile" in
  bounded|full) ;;
  *) printf 'alpha completion audit: error: unsupported profile: %s\n' "$profile" >&2; exit 2 ;;
esac

bash "$root/scripts/validate-alpha-contract.sh" >/dev/null

if [[ "$profile" == "bounded" ]]; then
  # Bounded alpha ships implemented rows and records broader work as follow-up.
  incomplete="$(awk -F '\t' 'NR > 1 && $8 == "alpha-required" && $7 != "implemented" { print $1 "\t" $7 "\t" $9 }' "$matrix")"
else
  incomplete="$(awk -F '\t' 'NR > 1 && $7 != "implemented" { print $1 "\t" $7 "\t" $9 }' "$matrix")"
fi
if [[ -n "$incomplete" ]]; then
  incomplete_count="$(printf '%s\n' "$incomplete" | awk 'END { print NR }')"
  printf 'alpha completion audit: FAIL (%d incomplete contract row(s))\n' "$incomplete_count" >&2
  printf '  %s\n' "$incomplete" >&2
  exit 1
fi

if [[ "$profile" == "full" && -f "$report" ]] && grep -q '"status":"deferred"' "$report"; then
  printf 'alpha completion audit: FAIL (harness report contains deferred stage)\n' >&2
  exit 1
fi

if [[ "$profile" == "bounded" ]]; then
  printf 'alpha completion audit: PASS (bounded release rows implemented; follow-up rows remain partial)\n'
else
  printf 'alpha completion audit: PASS (all contract rows implemented)\n'
fi
