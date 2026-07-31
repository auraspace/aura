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
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

case "$(uname -s)" in
  Linux)
    lib="$tmp/libaura_net_ffi.so"
    "$cc" -D_POSIX_C_SOURCE=200809L -std=c11 -Wall -Wextra -Werror -fPIC -shared \
      -fsanitize=address,undefined -fno-omit-frame-pointer \
      -o "$lib" std/net/native/aura_net_ffi.c
    lib_path_var=LD_LIBRARY_PATH
    ;;
  Darwin)
    lib="$tmp/libaura_net_ffi.dylib"
    "$cc" -D_POSIX_C_SOURCE=200809L -std=c11 -Wall -Wextra -Werror -fPIC -dynamiclib \
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

asan_options="${ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}"
if [[ "$(uname -s)" == Darwin && -z "${ASAN_OPTIONS+x}" ]]; then
  asan_options='detect_leaks=0:halt_on_error=1'
fi
ASAN_OPTIONS="$asan_options" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  target/debug/aura build examples/http-health-aura -o "$tmp/http-health-aura"

# Choose a high, per-run port so this smoke does not collide with a developer's
# local server. Callers can override it when a fixed port is required.
port="${AURA_HTTP_SMOKE_PORT:-$((20000 + RANDOM % 30000))}"
ASAN_OPTIONS="$asan_options" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/http-health-aura" "$port" >"$tmp/server.log" 2>&1 &
server_pid=$!

for _ in $(seq 1 40); do
  if curl --silent --show-error --max-time 1 \
    "http://127.0.0.1:$port/health" >"$tmp/health" 2>/dev/null; then
    # A successful transfer can still contain a transient/non-health response
    # while the server task is coming up; only accept the expected body.
    [[ "$(cat "$tmp/health")" == "ok" ]] && break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$tmp/server.log" >&2
    exit 1
  fi
  sleep 0.1
done

[[ "$(cat "$tmp/health")" == "ok" ]] || {
  printf 'http aura smoke: unexpected health body\n' >&2
  exit 1
}
client_pids=()
for i in $(seq 1 16); do
  curl --silent --show-error --max-time 2 \
    "http://127.0.0.1:$port/health" >"$tmp/health-$i" 2>"$tmp/health-$i.err" &
  client_pids+=("$!")
done
for pid in "${client_pids[@]}"; do
  wait "$pid"
done
for i in $(seq 1 16); do
  [[ "$(cat "$tmp/health-$i")" == "ok" ]] || {
    printf 'http aura smoke: concurrent client %s returned an unexpected body\n' "$i" >&2
    exit 1
  }
done
[[ "$(curl --silent --show-error --max-time 1 --output "$tmp/not-found" --write-out '%{http_code}' "http://127.0.0.1:$port/missing")" == "404" ]] || exit 1
[[ "$(curl --silent --show-error --max-time 1 -X POST --output "$tmp/method" --write-out '%{http_code}' "http://127.0.0.1:$port/health")" == "405" ]] || exit 1
[[ "$(curl --silent --show-error --max-time 1 -X POST -H 'Transfer-Encoding: chunked' --data-binary 'streamed-body' --output "$tmp/chunked-stream" --write-out '%{http_code}' "http://127.0.0.1:$port/stream")" == "200" ]] || exit 1
[[ "$(cat "$tmp/chunked-stream")" == "streamed-body" ]] || {
  printf 'http aura smoke: chunked streaming body mismatch\n' >&2
  exit 1
}
[[ "$(curl --silent --show-error --max-time 1 -X POST --data-binary 'streamed-body' --output "$tmp/streamed" --write-out '%{http_code}' "http://127.0.0.1:$port/stream")" == "200" ]] || exit 1
[[ "$(cat "$tmp/streamed")" == "streamed-body" ]] || {
  printf 'http aura smoke: streaming body mismatch\n' >&2
  exit 1
}
[[ "$(curl --silent --show-error --max-time 1 --output "$tmp/stream-response" --write-out '%{http_code}' "http://127.0.0.1:$port/stream-response")" == "200" ]] || exit 1
[[ "$(cat "$tmp/stream-response")" == "onetwo" ]] || {
  printf 'http aura smoke: streaming response body mismatch\n' >&2
  exit 1
}

ASAN_OPTIONS="$asan_options" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  target/debug/aura build corpus/std_http/client -o "$tmp/http-client"
client_response="$(ASAN_OPTIONS="$asan_options" UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" "$tmp/http-client" "$port")"
[[ "$client_response" == $'200\nok' ]] || {
  printf 'http aura smoke: Aura client GET failed\n' >&2
  exit 1
}

ASAN_OPTIONS="$asan_options" \
UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  target/debug/aura build corpus/std_http/client_post -o "$tmp/http-client-post"
client_post_response="$(ASAN_OPTIONS="$asan_options" UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" "$tmp/http-client-post" "$port")"
[[ "$client_post_response" == $'200\naura-client-post' ]] || {
  printf 'http aura smoke: Aura client POST failed\n' >&2
  exit 1
}

printf 'http aura smoke: passed\n'
