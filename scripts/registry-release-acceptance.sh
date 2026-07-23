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
u8_report="$(mktemp "${TMPDIR:-/tmp}/aura-u8-report.XXXXXX")"
trap 'rm -f "$u8_report"' EXIT

run() {
  printf '\n==> %s\n' "$1"
  shift
  "$@"
}

run "registry package fixture acceptance" \
  env AURA_U8_REPORT="$u8_report" cargo test -p aura-cli u8_local_registry_release_acceptance -- --nocapture
[[ -s "$u8_report" ]] || { printf 'registry acceptance: U8 test produced no evidence report\n' >&2; exit 1; }
run "publish receipt and update verification regressions" \
  cargo test -p aura-cli publish_fixture -- --nocapture
run "atomic update verification regressions" \
  cargo test -p aura-cli u7_ -- --nocapture
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
rg -q 'fn parse_receipt' crates/aura-cli/src/package/registry.rs
rg -q 'publish receipt does not match' crates/aura-cli/src/package/registry.rs
rg -q 'signature=deferred' crates/aura-cli/src/package/registry.rs

host="$(uname -s)-$(uname -m)"
cat >"$report" <<EOF
{"schema_version":1,"network":false,"production_claim":false,"production_credentials":"not-configured","registry_fixture":"u8_local_registry_release_acceptance","protocol":"rfc005-sparse-index-plus-api-v1","publish":{"http_status":201,"receipt":"verified-local-fixture","identity":"package/version/checksum"},"update":{"checksum":"verified-local-fixture","rollback":"verified-local-fixture","signature":"deferred-alpha-primitive"},"cross_host":"artifact-file-acceptance","host":"$host","outcome":"pass"}
EOF
bash scripts/validate-registry-acceptance.sh --report "$report"
printf 'registry/release acceptance: PASS (%s)\n' "$report"
