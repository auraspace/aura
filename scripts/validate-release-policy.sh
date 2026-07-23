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
for file in "$manifest" "$workflow" "$package_script" "$installer" "$rfc" "$release_docs"; do
  [[ -f "$file" ]] || die "missing policy input: $file"
done

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
rg -q 'validate-release-policy\.sh' "$workflow" || die "release workflow does not run policy validation"
rg -q 'AURA_MINISIGN_SECRET_KEY' "$workflow" || die "release workflow has no minisign secret input"
rg -q 'AURA_MINISIGN_PUBLIC_KEY' "$workflow" || die "release workflow has no minisign public-key input"
rg -q 'minisign -Vm' "$workflow" || die "release workflow does not verify its signature"
rg -q 'SHA256SUMS\.minisig' "$workflow" || die "release workflow does not publish detached signature"
rg -q 'AURA_VERIFY_SIGNATURE' scripts/release-signing.md || die "signing policy omits installer verification"
info "validated $required required target(s) and tier2 policy rows"
info "validated fail-closed minisign production path"
