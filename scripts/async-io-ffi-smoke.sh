#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cc="${CC:-cc}"
[[ -x target/debug/aura ]] || cargo build -q -p aura-cli
command -v "$cc" >/dev/null 2>&1

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-async-io-ffi.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

"$cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -fno-omit-frame-pointer -o "$tmp/async-io-ffi-handles" \
  runtime/tests/async_io_ffi_handles.c
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/async-io-ffi-handles"

case "$(uname -s)" in
  Linux)
    lib="$tmp/libaura_async_io_ffi.so"
    "$cc" -std=c11 -Wall -Wextra -Werror -fPIC -shared \
      -fsanitize=address,undefined -fno-omit-frame-pointer \
      -o "$lib" examples/async-io-ffi-aura/native/aura_async_io_ffi.c
    export LD_LIBRARY_PATH="$tmp${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    ;;
  Darwin)
    lib="$tmp/libaura_async_io_ffi.dylib"
    "$cc" -std=c11 -Wall -Wextra -Werror -fPIC -dynamiclib \
      -fsanitize=address,undefined -fno-omit-frame-pointer \
      -o "$lib" examples/async-io-ffi-aura/native/aura_async_io_ffi.c
    export DYLD_LIBRARY_PATH="$tmp${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
    ;;
  *)
    printf 'async io ffi smoke: unsupported host: %s\n' "$(uname -s)" >&2
    exit 2
    ;;
esac

export LIBRARY_PATH="$tmp${LIBRARY_PATH:+:$LIBRARY_PATH}"
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  target/debug/aura run examples/async-io-ffi-aura

printf 'async io ffi smoke: passed\n'
