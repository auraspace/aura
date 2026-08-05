#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
[[ -x target/debug/aura ]] || cargo build -q -p aura-cli

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-todo-app.XXXXXX")"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

target/debug/aura build examples/todo-app -o "$tmp/todo-app" >/dev/null
port="${AURA_TODO_APP_PORT:-$((20000 + RANDOM % 30000))}"
"$tmp/todo-app" "$port" >"$tmp/server.log" 2>&1 &
server_pid=$!

for _ in $(seq 1 100); do
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

grep -q '"status":"ok"' "$tmp/health"
created="$(curl --silent --show-error --max-time 2 -X POST \
  -H 'Content-Type: application/json' \
  --data-binary '{"title":"Ship Todo API"}' \
  "http://127.0.0.1:$port/api/todos")"
grep -q '"id":1' <<<"$created"
grep -q '"title":"Ship Todo API"' <<<"$created"
grep -q '"status":"Active"' <<<"$created"
grep -q '"id":1' <(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/todos?completed=false")
grep -q '"id":1' <(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/todos")
updated="$(curl --silent --show-error --max-time 2 -X PATCH \
  -H 'Content-Type: application/json' \
  --data-binary '{"completed":true}' \
  "http://127.0.0.1:$port/api/todos/1")"
grep -q '"completed":true' <<<"$updated"
grep -q '"status":"Completed"' <<<"$updated"
grep -q '"id":1' <(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/todos?completed=true")
[[ "$(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/todos?completed=false")" == "[]" ]]
invalid_filter="$(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/todos?completed=maybe")"
grep -q '"error":"invalid_filter"' <<<"$invalid_filter"
grep -q '"completed":1' <(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/stats")
grep -q '"id":1' <(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/v1/todos")
curl --silent --show-error --max-time 2 -X DELETE \
  "http://127.0.0.1:$port/api/todos/1" -o /dev/null
[[ "$(curl --silent --show-error --max-time 2 \
  "http://127.0.0.1:$port/api/todos")" == "[]" ]]

printf '%s\n' 'todo app smoke: passed'
