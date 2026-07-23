#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-registry-release-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

report="$tmp/registry-release.json"
AURA_REGISTRY_RELEASE_REPORT="$report" bash scripts/registry-release-acceptance.sh
sed 's/"production_claim":false/"production_claim":true/' "$report" >"$tmp/production-claim.json"
if bash scripts/validate-registry-acceptance.sh --report "$tmp/production-claim.json" >/dev/null 2>&1; then
  printf 'registry/release acceptance script: validator accepted a production claim from an offline fixture\n' >&2
  exit 1
fi
sed 's/"protocol":"rfc005-sparse-index-plus-api-v1"/"protocol":"unknown"/' "$report" >"$tmp/protocol-drift.json"
if bash scripts/validate-registry-acceptance.sh --report "$tmp/protocol-drift.json" >/dev/null 2>&1; then
  printf 'registry/release acceptance script: validator accepted protocol drift\n' >&2
  exit 1
fi
bash scripts/tests/release-bundle.sh
bash scripts/cross-host-acceptance.sh --help >/dev/null

printf 'registry/release acceptance script: PASS\n'
