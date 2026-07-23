#!/usr/bin/env bash
# Run the direct Aura -> std.net primitive HTTP/loopback fixture.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cc="${CC:-cc}"
[[ -x target/debug/aura ]] || cargo build -q -p aura-cli
command -v "$cc" >/dev/null 2>&1 || {
  printf 'http aura smoke: C compiler not found: %s\n' "$cc" >&2
  exit 1
}

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-http-aura.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

case "$(uname -s)" in
  Linux)
    lib="$tmp/libaura_net_ffi.so"
    "$cc" -std=c11 -Wall -Wextra -Werror -fPIC -shared \
      -fsanitize=address,undefined -fno-omit-frame-pointer \
      -o "$lib" std/net/native/aura_net_ffi.c
    lib_path_var=LD_LIBRARY_PATH
    ;;
  Darwin)
    lib="$tmp/libaura_net_ffi.dylib"
    "$cc" -std=c11 -Wall -Wextra -Werror -fPIC -dynamiclib \
      -fsanitize=address,undefined -fno-omit-frame-pointer \
      -o "$lib" std/net/native/aura_net_ffi.c
    lib_path_var=DYLD_LIBRARY_PATH
    ;;
  *)
    printf 'http aura smoke: unsupported host: %s\n' "$(uname -s)" >&2
    exit 2
    ;;
esac

export LIBRARY_PATH="$tmp${LIBRARY_PATH:+:$LIBRARY_PATH}"
if [[ "$lib_path_var" == LD_LIBRARY_PATH ]]; then
  export LD_LIBRARY_PATH="$tmp${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
else
  export DYLD_LIBRARY_PATH="$tmp${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
fi

ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  target/debug/aura run examples/http-health-aura
