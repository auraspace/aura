#!/usr/bin/env bash
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
bash scripts/validate-release-policy.sh >/dev/null
bash scripts/validate-cross-target-packaging.sh >/dev/null
if ! PATH=/usr/bin:/bin bash scripts/validate-release-policy.sh >/dev/null; then
  printf 'release policy test: validator requires optional rg dependency\n' >&2
  exit 1
fi
tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-release-policy.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
edit_file() {
  local expression="$1" file="$2" temporary="${2}.tmp"
  sed "$expression" "$file" >"$temporary"
  mv "$temporary" "$file"
}
append_rogue_target() {
  local file="$1" temporary="${1}.tmp"
  awk '{ print } /name: darwin-amd64/ { print "          - os: ubuntu-latest"; print "            name: rogue-target" }' "$file" >"$temporary"
  mv "$temporary" "$file"
}
replace_first() {
  local old="$1" new="$2" file="$3" temporary="${3}.tmp"
  awk -v old="$old" -v new="$new" '
    !done && index($0, old) { sub(old, new); done = 1 }
    { print }
  ' "$file" >"$temporary"
  mv "$temporary" "$file"
}
cp .github/workflows/release.yml "$tmp/release.yml"
edit_file '/name: darwin-amd64/d' "$tmp/release.yml"
if AURA_RELEASE_WORKFLOW_FILE="$tmp/release.yml" bash scripts/validate-release-policy.sh >/dev/null 2>&1; then
  printf 'release policy test: validator missed workflow target drift\n' >&2
  exit 1
fi
cp .github/workflows/release.yml "$tmp/signing.yml"
edit_file '/--require-signature/d' "$tmp/signing.yml"
if AURA_RELEASE_WORKFLOW_FILE="$tmp/signing.yml" bash scripts/validate-release-policy.sh >/dev/null 2>&1; then
  printf 'release policy test: validator missed mandatory signing drift\n' >&2
  exit 1
fi
cp .github/workflows/release.yml "$tmp/manifest.yml"
edit_file '/release-manifest\.json/d' "$tmp/manifest.yml"
if AURA_RELEASE_WORKFLOW_FILE="$tmp/manifest.yml" bash scripts/validate-release-policy.sh >/dev/null 2>&1; then
  printf 'release policy test: validator missed manifest wiring drift\n' >&2
  exit 1
fi

cp -R scripts/fixtures/target-policy "$tmp/target-policy"
rm "$tmp/target-policy/linux-arm64.json"
if AURA_TARGET_POLICY_FIXTURE_DIR="$tmp/target-policy" bash scripts/validate-release-policy.sh >/dev/null 2>&1; then
  printf 'release policy test: validator missed missing linux-arm64 policy fixture\n' >&2
  exit 1
fi
cp scripts/fixtures/target-policy/linux-arm64.json "$tmp/target-policy/linux-arm64.json"
edit_file 's/"production_claim": false/"production_claim": true/' "$tmp/target-policy/windows-amd64.json"
if AURA_TARGET_POLICY_FIXTURE_DIR="$tmp/target-policy" bash scripts/validate-release-policy.sh >/dev/null 2>&1; then
  printf 'release policy test: validator accepted an unsupported target claim\n' >&2
  exit 1
fi

cp scripts/package-release.sh "$tmp/package-release.sh"
chmod +x "$tmp/package-release.sh"
edit_file 's/linux-amd64|darwin-arm64|darwin-amd64/linux-amd64|darwin-arm64|darwin-amd64|linux-arm64/' "$tmp/package-release.sh"
if AURA_PACKAGE_SCRIPT_FILE="$tmp/package-release.sh" bash scripts/validate-cross-target-packaging.sh >/dev/null 2>&1; then
  printf 'release policy test: validator accepted a policy-only target in package support\n' >&2
  exit 1
fi

cp .github/workflows/release.yml "$tmp/extra-target.yml"
append_rogue_target "$tmp/extra-target.yml"
if AURA_RELEASE_WORKFLOW_FILE="$tmp/extra-target.yml" bash scripts/validate-cross-target-packaging.sh >/dev/null 2>&1; then
  printf 'release policy test: validator accepted an unmanifested workflow target\n' >&2
  exit 1
fi

cp .github/workflows/ci.yml "$tmp/ci-drift.yml"
append_rogue_target "$tmp/ci-drift.yml"
if AURA_CI_WORKFLOW_FILE="$tmp/ci-drift.yml" bash scripts/validate-cross-target-packaging.sh >/dev/null 2>&1; then
  printf 'release policy test: validator accepted CI platform-contract target drift\n' >&2
  exit 1
fi

cp .github/workflows/release.yml "$tmp/runner-drift.yml"
# Change only the linux-amd64 matrix runner. Target names still match, so this
# proves the validator binds each required target to its declared native host.
replace_first '- os: ubuntu-latest' '- os: ubuntu-22.04' "$tmp/runner-drift.yml"
if AURA_RELEASE_WORKFLOW_FILE="$tmp/runner-drift.yml" bash scripts/validate-cross-target-packaging.sh >/dev/null 2>&1; then
  printf 'release policy test: validator accepted release runner drift\n' >&2
  exit 1
fi
printf 'release policy tests: PASS\n'
