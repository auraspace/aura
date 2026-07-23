#!/usr/bin/env bash
# Build and run representative Aura programs under AddressSanitizer and UBSan.
#
# The Aura compiler invokes the C compiler through CC. This script can therefore
# act as a CC wrapper while also serving as the smoke-test entry point.
set -euo pipefail

if [[ "${AURA_SANITIZER_CC:-}" == "1" ]]; then
  : "${AURA_SANITIZER_REAL_CC:?AURA_SANITIZER_REAL_CC is required in compiler-wrapper mode}"
  exec "$AURA_SANITIZER_REAL_CC" \
    -fsanitize=address,undefined \
    -fno-omit-frame-pointer \
    "$@"
fi

native_only=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --native-only)
      native_only=1
      shift
      ;;
    *)
      printf 'sanitizer smoke: unknown argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
manifest="${SANITIZER_SEEDS_MANIFEST:-$root/runtime/tests/sanitizer-seeds.tsv}"

# Keep every sanitizer fixture tied to a deterministic seed and minimized
# reproducer before running the smoke matrix.
bash scripts/validate-sanitizer-seeds.sh >/dev/null

bin="${AURA_BIN:-target/debug/aura}"
if [[ ! -x "$bin" ]]; then
  printf 'sanitizer smoke: Aura binary not found: %s\n' "$bin" >&2
  printf 'build it first with: cargo build -p aura-cli\n' >&2
  exit 1
fi

real_cc="${AURA_SANITIZER_REAL_CC:-${CC:-cc}}"
if ! command -v "$real_cc" >/dev/null 2>&1; then
  printf 'sanitizer smoke: C compiler not found: %s\n' "$real_cc" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

native_asan_options="${AURA_SANITIZER_NATIVE_ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}"
aura_asan_options="${AURA_SANITIZER_AURA_ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}"
ubsan_options="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}"

run_native_fixture() {
  local fixture="$1"
  local source="$2"
  local output="$tmp/$fixture"
  local extra=()
  if ! grep -q 'AURA_RUNTIME_NO_MAIN' "$source"; then
    extra=(-D AURA_RUNTIME_NO_MAIN)
  fi
  printf 'sanitizer smoke: %s\n' "$fixture"
  "$real_cc" "${extra[@]}" -std=c11 -Wall -Wextra -Werror \
    -fsanitize=address,undefined -fno-omit-frame-pointer \
    -o "$output" "$source"
  ASAN_OPTIONS="$native_asan_options" \
    UBSAN_OPTIONS="$ubsan_options" \
    "$output"
}

while IFS=$'\t' read -r fixture seed source command; do
  [[ "$fixture" == "fixture" ]] && continue
  run_native_fixture "$fixture" "$source"
done < "$manifest"

ASAN_OPTIONS="$native_asan_options" \
  UBSAN_OPTIONS="$ubsan_options" \
  bash scripts/async-io-ffi-smoke.sh

if [[ "$native_only" == "1" ]]; then
  printf 'sanitizer smoke: native fixtures passed\n'
  exit 0
fi

run_aura() {
  local label="$1"
  shift
  printf 'sanitizer smoke: %s\n' "$label"
  AURA_SANITIZER_CC=1 \
    AURA_SANITIZER_REAL_CC="$real_cc" \
    CC="$0" \
    ASAN_OPTIONS="$aura_asan_options" \
    UBSAN_OPTIONS="$ubsan_options" \
    "$bin" "$@"
}

cat >"$tmp/wc-input.txt" <<'EOF'
one two
three	four
EOF

run_aura hello run corpus/hello/main.aura
run_aura array-ownership run corpus/generic/array_memory_safety.aura
run_aura gc run corpus/class/gc_nested_churn.aura
run_aura exceptions run corpus/control/exception_payload_cleanup.aura
run_aura async-no-await run corpus/async/no_await.aura
run_aura async-lifecycle run corpus/async/task_lifecycle.aura
run_aura async-multi-await run corpus/async/multi_await_four.aura
run_aura std-io-files run corpus/std_io/files/aura.toml
run_aura lambdas run corpus/fun/lambda_memory_safety.aura
run_aura examples-wc run examples/wc -- "$tmp/wc-input.txt"
run_aura http-health-cli run examples/http-health-cli
ASAN_OPTIONS="$aura_asan_options" \
  UBSAN_OPTIONS="$ubsan_options" \
  bash scripts/http-aura-smoke.sh

printf 'sanitizer smoke: all cases passed\n'
