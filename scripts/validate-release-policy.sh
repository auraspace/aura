#!/usr/bin/env bash
# Validate release targets, signing, and acceptance policy offline.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
die() { printf 'release policy: error: %s\n' "$*" >&2; exit 1; }
info() { printf 'release policy: %s\n' "$*"; }

manifest="${AURA_RELEASE_TARGETS_FILE:-scripts/release-targets.tsv}"
workflow="${AURA_RELEASE_WORKFLOW_FILE:-.github/workflows/release.yml}"
package_script="${AURA_PACKAGE_SCRIPT_FILE:-scripts/package-release.sh}"
installer="${AURA_INSTALLER_FILE:-scripts/install.sh}"
rfc="${AURA_RELEASE_RFC_FILE:-docs/rfc/RFC-013-binary-distribution.md}"
release_docs="${AURA_RELEASE_DOCS_FILE:-docs/releases/README.md}"
target_fixture_dir="${AURA_TARGET_POLICY_FIXTURE_DIR:-scripts/fixtures/target-policy}"
cross_target_validator="${AURA_CROSS_TARGET_VALIDATOR_FILE:-scripts/validate-cross-target-packaging.sh}"
for file in "$manifest" "$workflow" "$package_script" "$installer" "$rfc" "$release_docs" "$cross_target_validator"; do
  [[ -f "$file" ]] || die "missing policy input: $file"
done
[[ -x "$cross_target_validator" ]] || die "cross-target validator is not executable: $cross_target_validator"

required=0
while IFS=$'\t' read -r target tier runner format install acceptance; do
  [[ -z "${target:-}" || "${target:0:1}" == "#" ]] && continue
  [[ -n "${tier:-}" && -n "${runner:-}" && -n "${format:-}" && -n "${install:-}" && -n "${acceptance:-}" ]] \
    || die "malformed target row: $target"
  [[ "$target" =~ ^(linux|darwin|windows)-(amd64|arm64)$ ]] || die "invalid target: $target"
  case "$tier" in required|tier2) ;; *) die "invalid tier for $target: $tier" ;; esac
  platform="${target%-*}/${target##*-}"
  if [[ "$tier" == required ]]; then
    required=$((required + 1))
    rg -q "name: $target" "$workflow" || die "required target $target missing from release workflow"
    rg -q "$platform" "$package_script" || die "required target $target missing from package script"
    rg -q "$platform" "$installer" || die "required target $target missing from installer"
    rg -q "$target" "$rfc" || die "required target $target missing from RFC-013"
    rg -q "$target" "$release_docs" || die "required target $target missing from release docs"
    [[ "$format" == tar.gz && "$install" == supported ]] || die "required target $target is not install-supported"
  else
    rg -q "$target" "$rfc" || die "tier2 target $target missing from RFC-013"
    rg -q "$target" "$release_docs" || die "tier2 target $target missing from release docs"
    [[ "$install" == deferred && "$acceptance" == policy-only ]] || die "tier2 target $target needs deferred policy-only acceptance"
  fi
done < "$manifest"

[[ "$required" -gt 0 ]] || die "target manifest has no required targets"
[[ -d "$target_fixture_dir" ]] || die "missing target policy fixture directory: $target_fixture_dir"

# Tier2 rows are intentionally not release artifacts, but they still need a
# machine-checkable, fail-closed policy fixture.
python3 - "$manifest" "$target_fixture_dir" <<'PY'
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
fixture_dir = pathlib.Path(sys.argv[2])
rows = {}
for line in manifest_path.read_text(encoding="utf-8").splitlines():
    if not line.strip() or line.lstrip().startswith("#"):
        continue
    fields = line.split("\t")
    target, tier, _runner, _format, install, acceptance = fields
    rows[target] = {"tier": tier, "install": install, "acceptance": acceptance}

tier2 = {target for target, row in rows.items() if row["tier"] == "tier2"}
fixtures = {}
for path in sorted(fixture_dir.glob("*.json")):
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid target policy fixture {path.name}: {exc}") from exc
    target = record.get("target")
    if not isinstance(target, str) or target in fixtures:
        raise SystemExit(f"invalid or duplicate target policy fixture: {path.name}")
    fixtures[target] = record

if set(fixtures) != tier2:
    raise SystemExit(
        "target policy fixtures must exactly cover tier2 rows: "
        f"expected={sorted(tier2)} actual={sorted(fixtures)}"
    )

for target in sorted(tier2):
    record = fixtures[target]
    row = rows[target]
    if record.get("schema_version") != 1:
        raise SystemExit(f"unsupported target policy fixture schema: {target}")
    if record.get("tier") != "tier2":
        raise SystemExit(f"target policy fixture is not tier2: {target}")
    if record.get("artifact") != "not-produced":
        raise SystemExit(f"tier2 fixture must not claim an artifact: {target}")
    if record.get("acceptance") != "policy-only" or row["acceptance"] != "policy-only":
        raise SystemExit(f"tier2 fixture acceptance must be policy-only: {target}")
    if record.get("installer") != "deferred" or row["install"] != "deferred":
        raise SystemExit(f"tier2 fixture installer status must be deferred: {target}")
    if record.get("native_runner") is not False or record.get("production_claim") is not False:
        raise SystemExit(f"tier2 fixture makes an unsupported production claim: {target}")
    if not isinstance(record.get("reason"), str) or not record["reason"].strip():
        raise SystemExit(f"tier2 fixture needs a non-empty limitation reason: {target}")
PY
rg -q 'validate-release-policy\.sh' "$workflow" || die "release workflow does not run policy validation"
rg -q 'AURA_MINISIGN_SECRET_KEY' "$workflow" || die "release workflow has no minisign secret input"
rg -q 'AURA_MINISIGN_PUBLIC_KEY' "$workflow" || die "release workflow has no minisign public-key input"
rg -q 'minisign -Vm' "$workflow" || die "release workflow does not verify its signature"
rg -q 'SHA256SUMS\.minisig' "$workflow" || die "release workflow does not publish detached signature"
rg -q 'generate-release-manifest\.sh' "$workflow" || die "release workflow does not generate a release manifest"
rg -q 'release-manifest\.json' "$workflow" || die "release workflow does not carry the release manifest"
rg -q 'release-acceptance' "$workflow" || die "release workflow does not collect acceptance reports"
rg -q 'validate-release-bundle\.sh' "$workflow" || die "release workflow does not validate the release bundle"
rg -q -- '--require-signature' "$workflow" || die "release workflow does not require signed bundle verification"
rg -q 'AURA_VERIFY_SIGNATURE' scripts/release-signing.md || die "signing policy omits installer verification"
AURA_RELEASE_TARGETS_FILE="$manifest" \
AURA_RELEASE_WORKFLOW_FILE="$workflow" \
AURA_PACKAGE_SCRIPT_FILE="$package_script" \
  bash "$cross_target_validator" || die "cross-target/package validation failed"
info "validated $required required target(s), tier2 policy fixtures, and signing policy"
info "validated fail-closed minisign production path"
