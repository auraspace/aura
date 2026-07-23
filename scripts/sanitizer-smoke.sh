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

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

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

printf 'sanitizer smoke: http-parser-fuzz\n'
"$real_cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -fno-omit-frame-pointer -o "$tmp/http-parser-fuzz" runtime/tests/http_parser_fuzz.c
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
  UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/http-parser-fuzz"

printf 'sanitizer smoke: task-waiter\n'
"$real_cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -fno-omit-frame-pointer -o "$tmp/task-waiter" runtime/tests/task_waiter.c
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
  UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/task-waiter"

printf 'sanitizer smoke: task-dependency\n'
"$real_cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -fno-omit-frame-pointer -o "$tmp/task-dependency" runtime/tests/task_dependency.c
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
  UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/task-dependency"

printf 'sanitizer smoke: task-io-cleanup\n'
"$real_cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -fno-omit-frame-pointer -o "$tmp/task-io-cleanup" \
  runtime/tests/task_io_cleanup_sanitizer.c
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
  UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/task-io-cleanup"

run_aura() {
  local label="$1"
  shift
  printf 'sanitizer smoke: %s\n' "$label"
  AURA_SANITIZER_CC=1 \
    AURA_SANITIZER_REAL_CC="$real_cc" \
    CC="$0" \
    ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
    UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
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
run_aura lambdas run corpus/fun/lambda_memory_safety.aura
run_aura examples-wc run examples/wc -- "$tmp/wc-input.txt"

printf 'sanitizer smoke: all cases passed\n'
