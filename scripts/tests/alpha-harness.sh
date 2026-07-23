#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-alpha-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

list="$(scripts/alpha-harness.sh --list)"
for stage in frontend backend runtime async io http build registry ffi sanitizer release; do
  printf '%s\n' "$list" | rg -qx "$stage"
done

set +e
scripts/alpha-harness.sh --stage runtime --report "$tmp/runtime.json" >/dev/null 2>&1
runtime_rc=$?
scripts/alpha-harness.sh --stage registry --offline --report "$tmp/registry.json" >/dev/null 2>&1
registry_rc=$?
scripts/alpha-harness.sh --stage frontend --target windows-amd64 --report "$tmp/unsupported.json" >/dev/null 2>&1
unsupported_rc=$?
set -e

[[ "$runtime_rc" -eq 0 ]] || { printf 'runtime stage failed: %d\n' "$runtime_rc" >&2; exit 1; }
[[ "$registry_rc" -eq 0 ]] || { printf 'offline registry stage failed: %d\n' "$registry_rc" >&2; exit 1; }
[[ "$unsupported_rc" -eq 3 ]] || { printf 'unsupported target returned: %d\n' "$unsupported_rc" >&2; exit 1; }

python3 -m json.tool "$tmp/runtime.json" >/dev/null
python3 -m json.tool "$tmp/registry.json" >/dev/null
rg -q '"schema_version":1' "$tmp/runtime.json"
rg -q '"status":"passed"' "$tmp/runtime.json"
rg -q '"status":"passed"' "$tmp/registry.json"

printf 'alpha harness tests: PASS\n'
