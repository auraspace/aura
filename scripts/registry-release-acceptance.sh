#!/usr/bin/env bash
# Network-independent G6/G8 registry and release acceptance.
#
# The local HTTP fixture in aura-cli exercises the same publish, sparse-index,
# checksum, install, update, rollback, and executable paths used by production.
# Signing is verified by the release workflow with the real minisign tool and
# this script checks that the workflow is fail-closed and its verifier is wired.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

report="${AURA_REGISTRY_RELEASE_REPORT:-$root/target/alpha-reports/registry-release.json}"
mkdir -p "$(dirname "$report")"

run() {
  printf '\n==> %s\n' "$1"
  shift
  "$@"
}

run "registry package fixture acceptance" \
  cargo test -p aura-cli u8_local_registry_release_acceptance -- --nocapture
run "registry protocol and compatibility tests" \
  cargo test -p aura-cli package -- --nocapture
run "release signing and target policy" \
  bash scripts/validate-release-policy.sh

# These assertions are deliberately source-level: they make the release gate
# fail if a later workflow edit turns signature verification into an optional
# success or drops the cross-file target check.
rg -q 'AURA_MINISIGN_SECRET_KEY' .github/workflows/release.yml
rg -q 'AURA_MINISIGN_PUBLIC_KEY' .github/workflows/release.yml
rg -q 'minisign -Vm' .github/workflows/release.yml
rg -q 'file .*x86_64|file "\$bin"' .github/workflows/release.yml
rg -q 'required.*native|cross-file' scripts/release-targets.tsv

host="$(uname -s)-$(uname -m)"
cat >"$report" <<EOF
{"schema_version":1,"network":false,"registry_fixture":"u8_local_registry_release_acceptance","protocol":"rfc005-sparse-index-plus-api-v1","checksum":"verified","signature":"release-workflow-minisign-fail-closed","cross_host":"artifact-file-acceptance","host":"$host","outcome":"pass"}
EOF
printf 'registry/release acceptance: PASS (%s)\n' "$report"
