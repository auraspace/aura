#!/usr/bin/env bash
# F6 native FFI acceptance matrix for supported POSIX hosts.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

host="$(uname -s)"
if [[ "$host" != "Linux" && "$host" != "Darwin" ]]; then
  printf 'ffi regression: unsupported native host: %s\n' "$host" >&2
  exit 2
fi

cc="${CC:-cc}"
if ! command -v "$cc" >/dev/null 2>&1; then
  printf 'ffi regression: C compiler not found: %s\n' "$cc" >&2
  exit 1
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-ffi-regression.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

for fixture in ffi_owned ffi_handles ffi_callbacks; do
  printf 'ffi regression: %s\n' "$fixture"
  "$cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
    -fno-omit-frame-pointer -o "$tmp/$fixture" "runtime/tests/$fixture.c"
  ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
    UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
    "$tmp/$fixture"
done

printf 'ffi regression: compiler primitive native fixture\n'
cargo test -q -p aura-codegen native_ffi_primitive_fixture_calls_and_static_links
printf 'ffi regression: %s native acceptance passed\n' "$host"
