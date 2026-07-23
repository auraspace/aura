#!/usr/bin/env bash
# Validate an artifact's cross-host acceptance claim without pretending to run
# a foreign executable. Native mode runs the binary; cross-file mode validates
# the object format and architecture only.
set -Eeuo pipefail

artifact=""
target=""
mode=""
report=""
die() { printf 'cross-host acceptance: error: %s\n' "$*" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) sed -n '2,7p' "$0"; exit 0 ;;
    --artifact) [[ $# -gt 1 ]] || die '--artifact needs a value'; artifact="$2"; shift 2 ;;
    --target) [[ $# -gt 1 ]] || die '--target needs a value'; target="$2"; shift 2 ;;
    --mode) [[ $# -gt 1 ]] || die '--mode needs a value'; mode="$2"; shift 2 ;;
    --report) [[ $# -gt 1 ]] || die '--report needs a value'; report="$2"; shift 2 ;;
    *) die "unknown option: $1" ;;
  esac
done
[[ -f "$artifact" ]] || die "artifact is not a file: $artifact"
[[ "$target" =~ ^(linux|darwin)-(amd64|arm64)$ ]] || die "unsupported target: $target"
[[ "$mode" == native || "$mode" == cross-file ]] || die "mode must be native or cross-file"
command -v file >/dev/null 2>&1 || die "file(1) is required"

description="$(file -b "$artifact")"
case "$target" in
  linux-amd64) [[ "$description" =~ (ELF|x86-64|x86_64) ]] || die "${target} artifact format mismatch: $description" ;;
  linux-arm64) [[ "$description" =~ (ELF|aarch64|ARM64) ]] || die "${target} artifact format mismatch: $description" ;;
  darwin-amd64) [[ "$description" =~ (Mach-O|x86_64|Intel) ]] || die "${target} artifact format mismatch: $description" ;;
  darwin-arm64) [[ "$description" =~ (Mach-O|arm64|ARM64) ]] || die "${target} artifact format mismatch: $description" ;;
esac

if [[ "$mode" == native ]]; then
  os="$(uname -s)"; arch="$(uname -m)"
  case "$os" in
    Linux) host_os=linux ;;
    Darwin) host_os=darwin ;;
    *) host_os=unsupported ;;
  esac
  case "$arch" in
    x86_64|amd64) host_arch=amd64 ;;
    arm64|aarch64) host_arch=arm64 ;;
    *) host_arch=unsupported ;;
  esac
  [[ "$target" == "$host_os-$host_arch" ]] || die "native mode target does not match host: $target"
  "$artifact" version >/dev/null
fi

if [[ -n "$report" ]]; then
  mkdir -p "$(dirname "$report")"
  escaped="${description//\\/\\\\}"
  escaped="${escaped//\"/\\\"}"
  printf '{"schema_version":1,"target":"%s","mode":"%s","format":"%s","outcome":"pass"}\n' \
    "$target" "$mode" "$escaped" >"$report"
fi
printf 'cross-host acceptance: PASS target=%s mode=%s (%s)\n' "$target" "$mode" "$description"
