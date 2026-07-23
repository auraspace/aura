#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
bash scripts/validate-release-policy.sh >/dev/null
tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-release-policy.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
cp .github/workflows/release.yml "$tmp/release.yml"
sed -i '/name: darwin-amd64/d' "$tmp/release.yml"
if AURA_RELEASE_WORKFLOW_FILE="$tmp/release.yml" bash scripts/validate-release-policy.sh >/dev/null 2>&1; then
  printf 'release policy test: validator missed workflow target drift\n' >&2
  exit 1
fi
printf 'release policy tests: PASS\n'
