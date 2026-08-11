#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
aura_bin="${AURA_BIN:-target/debug/aura}"
test -x "$aura_bin"

coverage_dir="target/aura/coverage-assertions_edges"
rm -rf "$coverage_dir"
report="$(mktemp "${TMPDIR:-/tmp}/aura-coverage.XXXXXX.json")"
trap 'rm -f "$report"' EXIT

"$aura_bin" test corpus/test/assertions_edges.aura --coverage --format json >"$report"
rg -q '"coverage"' "$report"
test -s "$coverage_dir/aura.lcov"
test -f "$coverage_dir/html/index.html"
rg -q '^SF:' "$coverage_dir/aura.lcov"

printf 'coverage: JSON, LCOV, and HTML artifacts verified\n'
