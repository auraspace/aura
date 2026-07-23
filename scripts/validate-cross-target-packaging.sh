#!/usr/bin/env bash
# Validate target/package wiring without compiling or executing a foreign
# artifact. This is host-only evidence and must not be described as a
# cross-compilation result.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
die() { printf 'cross-target packaging: error: %s\n' "$*" >&2; exit 1; }
info() { printf 'cross-target packaging: %s\n' "$*"; }

manifest="${AURA_RELEASE_TARGETS_FILE:-scripts/release-targets.tsv}"
workflow="${AURA_RELEASE_WORKFLOW_FILE:-.github/workflows/release.yml}"
ci_workflow="${AURA_CI_WORKFLOW_FILE:-.github/workflows/ci.yml}"
package_script="${AURA_PACKAGE_SCRIPT_FILE:-scripts/package-release.sh}"
[[ -f "$manifest" ]] || die "missing target manifest: $manifest"
[[ -f "$workflow" ]] || die "missing release workflow: $workflow"
[[ -f "$ci_workflow" ]] || die "missing CI workflow: $ci_workflow"
[[ -f "$package_script" ]] || die "missing package script: $package_script"
[[ -x "$package_script" ]] || die "package script is not executable: $package_script"

mapfile -t rows < <(awk -F '\t' '!/^([[:space:]]*#|[[:space:]]*$)/ { print }' "$manifest")
[[ ${#rows[@]} -gt 0 ]] || die "target manifest is empty"

declare -A tier acceptance package
required=()
for row in "${rows[@]}"; do
  IFS=$'\t' read -r target target_tier _runner target_package _installer target_acceptance extra <<<"$row"
  [[ -z "${extra:-}" && -n "${target:-}" && -n "${target_tier:-}" && -n "${target_package:-}" && -n "${target_acceptance:-}" ]] \
    || die "malformed target row: $row"
  [[ -z "${tier[$target]+x}" ]] || die "duplicate target row: $target"
  tier["$target"]="$target_tier"
  acceptance["$target"]="$target_acceptance"
  package["$target"]="$target_package"
  [[ "$target_tier" == required ]] && required+=("$target")
done
[[ ${#required[@]} -gt 0 ]] || die "manifest has no required targets"

# Compare sets, not substring presence. This catches removed, duplicated, or
# unapproved workflow targets.
mapfile -t workflow_targets < <(
  awk '/^[[:space:]]*-[[:space:]]+os:/ { in_entry=1; next }
       in_entry && /^[[:space:]]+name:[[:space:]]*/ { sub(/^[[:space:]]+name:[[:space:]]*/, ""); print; in_entry=0 }' "$workflow" \
    | sed 's/[[:space:]]*#.*$//' | sed 's/[[:space:]]*$//' | sort -u
)
expected_sorted="$(printf '%s\n' "${required[@]}" | sort -u)"
actual_sorted="$(printf '%s\n' "${workflow_targets[@]}" | sed '/^$/d' | sort -u)"
[[ "$expected_sorted" == "$actual_sorted" ]] \
  || die "workflow target set differs: expected=[$(tr '\n' ' ' <<<"$expected_sorted")] actual=[$(tr '\n' ' ' <<<"$actual_sorted")]"

# PR CI has a separate platform-contract matrix. Keep it aligned with the
# release matrix so a green pull request cannot validate a different artifact
# set from the tag workflow. Parse only that job's matrix; unrelated CI jobs
# (for example FFI-native) are intentionally outside this comparison.
mapfile -t ci_targets < <(
  awk '
    /^  platform-contract:/ { in_job=1; next }
    in_job && /^  [A-Za-z0-9_-]+:/ { exit }
    in_job && /^      matrix:/ { in_matrix=1; next }
    in_matrix && /^      steps:/ { exit }
    in_matrix && /^[[:space:]]+name:[[:space:]]*/ {
      sub(/^[[:space:]]+name:[[:space:]]*/, "")
      sub(/[[:space:]]*#.*$/, "")
      sub(/[[:space:]]*$/, "")
      print
    }
  ' "$ci_workflow" | sort -u
)
ci_sorted="$(printf '%s\n' "${ci_targets[@]}" | sed '/^$/d' | sort -u)"
[[ "$expected_sorted" == "$ci_sorted" ]] \
  || die "CI platform-contract target set differs: expected=[$(tr '\n' ' ' <<<"$expected_sorted")] actual=[$(tr '\n' ' ' <<<"$ci_sorted")]"

for target in "${required[@]}"; do
  [[ "${package[$target]}" == tar.gz ]] || die "required target $target is not tar.gz packaged"
  "$package_script" --validate-target "$target" >/dev/null \
    || die "package script rejected required target $target"
done

for target in "${!tier[@]}"; do
  if [[ "${tier[$target]}" == tier2 ]]; then
    [[ "${acceptance[$target]}" == policy-only ]] \
      || die "tier2 target $target has an artifact/acceptance claim"
    if "$package_script" --validate-target "$target" >/dev/null 2>&1; then
      die "package script accepted policy-only target $target"
    fi
    if grep -Eq "name:[[:space:]]*$target([[:space:]]|$)" "$workflow"; then
      die "workflow publishes policy-only target $target"
    fi
  fi
done

grep -Eq 'RUST_TARGET' "$package_script" || die "package script has no explicit cross-target input"
grep -Eq 'unsupported RUST_TARGET' "$package_script" || die "package script does not fail closed for unknown Rust targets"
info "PASS: ${#required[@]} required mappings and ${#rows[@]} policy rows validated across release and CI matrices (no cross build executed)"
