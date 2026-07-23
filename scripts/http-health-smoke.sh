#!/usr/bin/env bash
# Bounded native companion for the HTTP health-server journey.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cc="${CC:-cc}"
if ! command -v "$cc" >/dev/null 2>&1; then
  printf 'http health: C compiler not found: %s\n' "$cc" >&2
  exit 1
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-http-health.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

"$cc" -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -fno-omit-frame-pointer -o "$tmp/http-health" examples/http-health/http_health.c
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0:halt_on_error=1}" \
  UBSAN_OPTIONS="${UBSAN_OPTIONS:-halt_on_error=1:print_stacktrace=1}" \
  "$tmp/http-health"
