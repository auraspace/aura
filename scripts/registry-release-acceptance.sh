#!/usr/bin/env bash
# Network-independent G6/G8 registry and release acceptance.
#
# The local HTTP fixture in aura-cli exercises the same publish, sparse-index,
# checksum, install, update, rollback, and executable paths used by production.
# Offline release metadata signing is verified in aura-cli with aura-sig-v1;
# this script also checks that the release workflow remains fail-closed for its
# separate production artifact-signing policy.
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
run "registry cryptographic trust regressions" \
  cargo test -p aura-cli registry_signature_v1 -- --nocapture
run "atomic update verification regressions" \
  cargo test -p aura-cli u7_ -- --nocapture
run "registry protocol and compatibility tests" \
  cargo test -p aura-cli package -- --nocapture
run "release signing and target policy" \
  bash scripts/validate-release-policy.sh

# These assertions are deliberately source-level: they make the release gate
# fail if a later workflow edit turns signature verification into an optional
# success or drops the cross-file target check.
grep -Eq 'AURA_MINISIGN_SECRET_KEY' .github/workflows/release.yml
grep -Eq 'AURA_MINISIGN_PUBLIC_KEY' .github/workflows/release.yml
grep -Eq 'minisign -Vm' .github/workflows/release.yml
grep -Eq 'file .*x86_64|file "\$bin"' .github/workflows/release.yml
grep -Eq 'required.*native|cross-file' scripts/release-targets.tsv
grep -Eq 'fn parse_receipt' crates/aura-cli/src/package/registry.rs
grep -Eq 'publish receipt does not match' crates/aura-cli/src/package/registry.rs
grep -Eq 'aura-sig-v1' crates/aura-cli/src/package/registry.rs
grep -Eq 'verify_package_signatures' crates/aura-cli/src/package/registry.rs

host="$(uname -s)-$(uname -m)"
cat >"$report" <<EOF
{"schema_version":2,"network":false,"production_claim":false,"production_credentials":"not-configured","registry_fixture":"u8_local_registry_release_acceptance","protocol":"rfc005-sparse-index-plus-api-v1","publish":{"http_status":201,"receipt":"verified-local-fixture","identity":"package/version/checksum"},"update":{"checksum":"verified-local-fixture","rollback":"verified-local-fixture","signature":"verified-aura-sig-v1"},"crypto":{"format":"aura-sig-v1","trusted_key_verification":true,"tamper_rejection":true,"replay_rejection":true,"fail_closed":true},"cross_host":"artifact-file-acceptance","host":"$host","outcome":"pass"}
EOF
bash scripts/validate-registry-acceptance.sh --report "$report"
printf 'registry/release acceptance: PASS (%s)\n' "$report"
