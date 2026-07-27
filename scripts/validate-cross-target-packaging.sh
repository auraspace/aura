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

rows=()
while IFS= read -r row; do
  rows+=("$row")
done < <(awk -F '\t' '!/^([[:space:]]*#|[[:space:]]*$)/ { print }' "$manifest")
[[ ${#rows[@]} -gt 0 ]] || die "target manifest is empty"

required=()
contains_value() {
  local wanted="$1" value
  shift
  for value in "$@"; do
    [[ "$value" == "$wanted" ]] && return 0
  done
  return 1
}
manifest_field() {
  local wanted="$1" field="$2"
  awk -F '\t' -v target="$wanted" -v column="$field" '$1 == target { print $column; exit }' "$manifest"
}
for row in "${rows[@]}"; do
  IFS=$'\t' read -r target target_tier target_runner target_package _installer target_acceptance extra <<<"$row"
  [[ -z "${extra:-}" && -n "${target:-}" && -n "${target_tier:-}" && -n "${target_package:-}" && -n "${target_acceptance:-}" ]] \
    || die "malformed target row: $row"
  contains_value "$target" "${seen_targets[@]:-}" && die "duplicate target row: $target"
  seen_targets+=("$target")
  [[ "$target_tier" == required ]] && required+=("$target")
done
[[ ${#required[@]} -gt 0 ]] || die "manifest has no required targets"

workflow_target_pairs() {
  local file="$1" job="$2"
  awk -v job="$job" '
    $0 == "  " job ":" { in_job=1; next }
    in_job && /^  [A-Za-z0-9_-]+:/ { exit }
    in_job && /^[[:space:]]*-[[:space:]]+os:[[:space:]]*/ {
      os = $0
      sub(/^[[:space:]]*-[[:space:]]+os:[[:space:]]*/, "", os)
      sub(/[[:space:]]*#.*$/, "", os)
      sub(/[[:space:]]*$/, "", os)
      in_entry=1
      next
    }
    in_entry && /^[[:space:]]+name:[[:space:]]*/ {
      name = $0
      sub(/^[[:space:]]+name:[[:space:]]*/, "", name)
      sub(/[[:space:]]*#.*$/, "", name)
      sub(/[[:space:]]*$/, "", name)
      print name "\t" os
      in_entry=0
    }
  ' "$file"
}

# Compare target/runner pairs, not substring presence. This catches removed,
# duplicated, unapproved, and silently re-homed native targets.
workflow_rows=()
while IFS=$'\t' read -r target target_runner; do
  [[ -n "$target" && -n "$target_runner" ]] || die "malformed release workflow matrix entry"
  contains_value "$target" "${workflow_targets[@]:-}" && die "duplicate release workflow target: $target"
  workflow_targets+=("$target")
  workflow_rows+=("$target"$'\t'"$target_runner")
done < <(workflow_target_pairs "$workflow" build)
workflow_runner_for() {
  local wanted="$1" row
  for row in "${workflow_rows[@]}"; do
    [[ "${row%%$'\t'*}" == "$wanted" ]] && printf '%s\n' "${row#*$'\t'}" && return 0
  done
  return 1
}
expected_sorted="$(printf '%s\n' "${required[@]}" | sort -u)"
actual_sorted="$(printf '%s\n' "${workflow_targets[@]}" | sed '/^$/d' | sort -u)"
[[ "$expected_sorted" == "$actual_sorted" ]] \
  || die "workflow target set differs: expected=[$(tr '\n' ' ' <<<"$expected_sorted")] actual=[$(tr '\n' ' ' <<<"$actual_sorted")]"

# PR CI has a separate platform-contract matrix. Keep it aligned with the
# release matrix so a green pull request cannot validate a different artifact
# set from the tag workflow. Parse only that job's matrix; unrelated CI jobs
# (for example FFI-native) are intentionally outside this comparison.
ci_rows=()
while IFS=$'\t' read -r target target_runner; do
  [[ -n "$target" && -n "$target_runner" ]] || die "malformed CI platform-contract matrix entry"
  contains_value "$target" "${ci_targets[@]:-}" && die "duplicate CI platform-contract target: $target"
  ci_targets+=("$target")
  ci_rows+=("$target"$'\t'"$target_runner")
done < <(workflow_target_pairs "$ci_workflow" platform-contract)
ci_runner_for() {
  local wanted="$1" row
  for row in "${ci_rows[@]}"; do
    [[ "${row%%$'\t'*}" == "$wanted" ]] && printf '%s\n' "${row#*$'\t'}" && return 0
  done
  return 1
}
ci_sorted="$(printf '%s\n' "${ci_targets[@]}" | sed '/^$/d' | sort -u)"
[[ "$expected_sorted" == "$ci_sorted" ]] \
  || die "CI platform-contract target set differs: expected=[$(tr '\n' ' ' <<<"$expected_sorted")] actual=[$(tr '\n' ' ' <<<"$ci_sorted")]"

for target in "${required[@]}"; do
  expected_runner="$(manifest_field "$target" 3)"
  [[ "$(manifest_field "$target" 4)" == tar.gz ]] || die "required target $target is not tar.gz packaged"
  [[ "$(workflow_runner_for "$target")" == "$expected_runner" ]] \
    || die "release workflow runner differs for $target"
  [[ "$(ci_runner_for "$target")" == "$expected_runner" ]] \
    || die "CI platform-contract runner differs for $target"
  "$package_script" --validate-target "$target" >/dev/null \
    || die "package script rejected required target $target"
done

for row in "${rows[@]}"; do
  IFS=$'\t' read -r target target_tier _target_runner _target_package _installer target_acceptance _extra <<<"$row"
  if [[ "$target_tier" == tier2 ]]; then
    [[ "$target_acceptance" == policy-only ]] \
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
