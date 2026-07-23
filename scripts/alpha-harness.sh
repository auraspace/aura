#!/usr/bin/env bash
# Deterministic v0.1.1-alpha acceptance harness.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

stages=(frontend backend runtime async io http build registry ffi sanitizer release)
stage="" fixture="" target="" profile=dev network=0 keep_temp=0 report=""
failed=0 report_first=1

usage() {
  printf '%s\n' "Usage: scripts/alpha-harness.sh [options]" \
    "  --list                         list stages" \
    "  --stage NAME                   run one stage" \
    "  --fixture ID                   select a stage fixture" \
    "  --target TRIPLE                linux-amd64, darwin-arm64, darwin-amd64" \
    "  --profile PROFILE              dev, test, or release" \
    "  --offline                      skip network stages (default)" \
    "  --network                      enable network-required stages" \
    "  --keep-temp                    retain temporary files" \
    "  --report PATH                  write JSON report to PATH"
}
die_usage() { printf 'alpha harness: error: %s\n' "$*" >&2; exit 2; }
contains_stage() { local x="$1" s; for s in "${stages[@]}"; do [[ "$s" == "$x" ]] && return 0; done; return 1; }
fixture_exists() {
  [[ -n "$fixture" ]] || return 0
  awk -F '\t' -v wanted="$fixture" 'NR > 1 && $1 == wanted { found = 1 } END { exit !found }' \
    "$root/docs/plans/v0.1.1-alpha/contract-matrix.tsv"
}
fixture_stage() {
  case "$fixture" in
    COMP-*|DIAG-*) echo backend ;;
    RUNTIME-*) echo runtime ;;
    ASYNC-*) echo async ;;
    IO-*) echo io ;;
    HTTP-*) echo http ;;
    BUILD-*) echo build ;;
    REG-*) echo registry ;;
    FFI-*) echo ffi ;;
    SAN-*) echo sanitizer ;;
    REL-*) echo release ;;
    *) return 1 ;;
  esac
}
host_target() {
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64|Linux/amd64) echo linux-amd64 ;;
    Darwin/arm64|Darwin/aarch64) echo darwin-arm64 ;;
    Darwin/x86_64|Darwin/amd64) echo darwin-amd64 ;;
    *) echo unsupported ;;
  esac
}
target_supported() { case "$1" in linux-amd64|darwin-arm64|darwin-amd64) return 0 ;; *) return 1 ;; esac; }
json_escape() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\\"/g; s/[[:cntrl:]]/ /g'; }

report_init() {
  mkdir -p "$(dirname "$report")"
  printf '{"schema_version":1,"target":"%s","profile":"%s","offline":%s,"results":[' \
    "$(json_escape "$target")" "$profile" "$([[ "$network" -eq 0 ]] && echo true || echo false)" >"$report"
}
report_result() {
  local name="$1" status="$2" rc="$3" command="$4" rerun="$5" duration="$6"
  [[ "$report_first" -eq 1 ]] || printf ',' >>"$report"
  report_first=0
  printf '{"stage":"%s","fixture":"%s","status":"%s","exit_code":%d,"command":"%s","duration_ms":%d,"rerun":"%s"}' \
    "$(json_escape "$name")" "$(json_escape "${fixture:-$name}")" "$(json_escape "$status")" "$rc" \
    "$(json_escape "$command")" "$duration" "$(json_escape "$rerun")" >>"$report"
}
report_close() { printf ']}\n' >>"$report"; }

run_command() {
  local name="$1" command="$2" rerun="$3" started ended rc status
  started="$(date +%s)"
  set +e; bash -c "$command"; rc=$?; set -e
  ended="$(date +%s)"
  [[ "$rc" -eq 0 ]] && status=passed || status=failed
  report_result "$name" "$status" "$rc" "$command" "$rerun" "$(((ended - started) * 1000))"
  if [[ "$rc" -ne 0 ]]; then failed=1; printf 'FAIL [%s] fixture=%s exit=%d\n' "$name" "${fixture:-$name}" "$rc" >&2
  else printf 'PASS [%s] fixture=%s\n' "$name" "${fixture:-$name}"; fi
}
run_deferred() {
  local name="$1" reason="$2"
  report_result "$name" deferred 0 "deferred: $reason" "scripts/alpha-harness.sh --stage $name" 0
  printf 'DEFERRED [%s] %s\n' "$name" "$reason"
}
run_stage() {
  local name="$1"
  case "$name" in
    frontend) run_command frontend 'cargo test --workspace' 'cargo test --workspace' ;;
    backend)
      if [[ -z "$fixture" || "$fixture" == green-corpus ]]; then
        run_command backend 'bash scripts/check-corpus.sh' 'scripts/alpha-harness.sh --stage backend --fixture green-corpus'
      else
        run_command backend 'bash scripts/compiler-regression.sh' 'scripts/alpha-harness.sh --stage backend --fixture compiler-regression'
      fi ;;
    runtime) run_command runtime 'cc -std=c11 -Wall -Wextra -Werror -c runtime/aura_rt.c -o "$TEMP_RUNTIME_OBJECT"' 'scripts/alpha-harness.sh --stage runtime' ;;
    async) run_command async 'cargo test -p aura-cli async' 'cargo test -p aura-cli async' ;;
    io) run_command io 'bash scripts/compiler-regression.sh && bash scripts/async-io-ffi-smoke.sh' 'scripts/alpha-harness.sh --stage io' ;;
    http) run_command http 'bash scripts/http-aura-smoke.sh' 'scripts/alpha-harness.sh --stage http' ;;
    build) run_command build '"$AURA_BIN" build corpus/hello/main.aura -o "$BUILD_OUTPUT" && "$BUILD_OUTPUT"' 'scripts/alpha-harness.sh --stage build' ;;
    registry) run_command registry 'bash scripts/registry-release-acceptance.sh' 'bash scripts/registry-release-acceptance.sh' ;;
    ffi) run_command ffi 'bash scripts/ffi-regression.sh' 'scripts/alpha-harness.sh --stage ffi' ;;
    sanitizer) run_command sanitizer 'bash scripts/sanitizer-smoke.sh' 'scripts/alpha-harness.sh --stage sanitizer' ;;
    release) run_command release 'bash scripts/release-acceptance.sh --dry-run' 'scripts/alpha-harness.sh --stage release' ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --list) printf '%s\n' "${stages[@]}"; exit 0 ;;
    --stage) [[ $# -ge 2 ]] || die_usage '--stage needs a value'; stage="$2"; shift 2 ;;
    --fixture) [[ $# -ge 2 ]] || die_usage '--fixture needs a value'; fixture="$2"; shift 2 ;;
    --target) [[ $# -ge 2 ]] || die_usage '--target needs a value'; target="$2"; shift 2 ;;
    --profile) [[ $# -ge 2 ]] || die_usage '--profile needs a value'; profile="$2"; shift 2 ;;
    --offline) network=0; shift ;;
    --network) network=1; shift ;;
    --keep-temp) keep_temp=1; shift ;;
    --report) [[ $# -ge 2 ]] || die_usage '--report needs a value'; report="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) die_usage "unknown option: $1" ;;
  esac
done
[[ -z "$stage" ]] || contains_stage "$stage" || die_usage "unknown stage: $stage"
if [[ -n "$fixture" ]]; then
  fixture_exists || die_usage "unknown fixture: $fixture"
  if [[ -z "$stage" ]]; then stage="$(fixture_stage)" || die_usage "cannot infer stage for fixture: $fixture"; fi
fi
case "$profile" in dev|test|release) ;; *) die_usage "unsupported profile: $profile" ;; esac
target="${target:-$(host_target)}"
target_supported "$target" || { printf 'alpha harness: unsupported target: %s\n' "$target" >&2; exit 3; }

temp="$(mktemp -d "${TMPDIR:-/tmp}/aura-alpha.XXXXXX")"
report="${report:-$root/target/alpha-reports/report.json}"
export TEMP_RUNTIME_OBJECT="$temp/aura_rt.o" BUILD_OUTPUT="$temp/hello"
if [[ -n "${AURA_BIN:-}" ]]; then :;
elif [[ -x target/debug/aura ]]; then export AURA_BIN="$root/target/debug/aura";
else cargo build -q -p aura-cli; export AURA_BIN="$root/target/debug/aura"; fi

report_init
if [[ -n "$stage" ]]; then
  run_stage "$stage"
else
  for current in "${stages[@]}"; do
    run_stage "$current"
  done
fi
report_close
if [[ "$keep_temp" -eq 1 ]]; then printf 'alpha harness: report retained at %s\n' "$report"
else printf 'alpha harness: report written to %s\n' "$report"; rm -rf "$temp"; fi
[[ "$failed" -eq 0 ]]
