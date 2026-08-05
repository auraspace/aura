#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
[[ -x target/debug/aura ]] || cargo build -q -p aura-cli

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-http-engine.XXXXXX")"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

target/debug/aura build examples/http-engine-aura -o "$tmp/http-engine-aura" >/dev/null
port="${AURA_HTTP_ENGINE_SMOKE_PORT:-$((20000 + RANDOM % 30000))}"
"$tmp/http-engine-aura" "$port" >"$tmp/server.log" 2>&1 &
server_pid=$!

for _ in $(seq 1 40); do
  if curl --silent --show-error --max-time 1 \
    "http://127.0.0.1:$port/health" >"$tmp/health" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$tmp/server.log" >&2
    exit 1
  fi
  sleep 0.1
done

[[ "$(cat "$tmp/health")" == "ok" ]]
curl --silent --show-error --max-time 2 \
  -D "$tmp/headers" "http://127.0.0.1:$port/api/health" >"$tmp/api-health"
[[ "$(cat "$tmp/api-health")" == "ok" ]]
grep -q '^X-Aura-Engine: on' "$tmp/headers"
[[ "$(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/users/42?view=full")" == "user:42" ]]
[[ "$(curl --silent --show-error --max-time 2 -X PUT \
  "http://127.0.0.1:$port/users/42")" == "updated:42" ]]
[[ "$(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/search?q=aura")" == '{"q":"aura"}' ]]
[[ "$(curl --silent --show-error --max-time 2 -X POST \
  --data-binary 'hello aura' "http://127.0.0.1:$port/echo")" == "hello aura" ]]
[[ "$(curl --silent --show-error --max-time 2 -X POST \
  -H 'Content-Type: application/json' --data-binary '{"message":"aura"}' \
  "http://127.0.0.1:$port/json-echo")" == '{"message":"aura"}' ]]
oversized_status="$(printf '%033d' 0 | tr 0 x | curl --silent --show-error --max-time 5 \
  -o "$tmp/oversized" -w '%{http_code}' -X POST -H 'Content-Length: 33' --data-binary @- \
  "http://127.0.0.1:$port/echo")"
[[ "$oversized_status" == "413" ]]
[[ "$(cat "$tmp/oversized")" == '{"error":"payload_too_large"}' ]]
[[ "$(curl --silent --show-error --max-time 2 -o "$tmp/missing" \
  -w '%{http_code}' "http://127.0.0.1:$port/missing")" == "404" ]]
[[ "$(curl --silent --show-error --max-time 2 -o "$tmp/method" \
  -w '%{http_code}' -X DELETE "http://127.0.0.1:$port/health")" == "405" ]]
[[ "$(curl --silent --show-error --max-time 2 -o "$tmp/head" \
  -w '%{http_code}' -X HEAD "http://127.0.0.1:$port/health")" == "200" ]]

printf '%s\n' 'http-engine aura.web smoke: passed'
