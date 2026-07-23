#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-registry-release-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

bash scripts/registry-release-acceptance.sh
bash scripts/tests/release-bundle.sh
bash scripts/cross-host-acceptance.sh --help >/dev/null

printf 'registry/release acceptance script: PASS\n'
