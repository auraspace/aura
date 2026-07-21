#!/usr/bin/env bash
# Compiler and CLI regression matrix.
#
# This is intentionally separate from sanitizer-smoke.sh: it checks language
# coverage, CLI command paths, and expected diagnostics without sanitizer flags.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [[ -n "${AURA_BIN:-}" ]]; then
  bin="$AURA_BIN"
elif [[ -x target/debug/aura ]]; then
  bin=target/debug/aura
elif [[ -x target/release/aura ]]; then
  bin=target/release/aura
else
  bin=(cargo run -q -p aura-cli --)
fi

run_aura() {
  if [[ -n "${AURA_BIN:-}" ]] || [[ -x target/debug/aura ]] || [[ -x target/release/aura ]]; then
    "$bin" "$@"
  else
    cargo run -q -p aura-cli -- "$@"
  fi
}

pass_count=0
fail() {
  printf 'compiler regression failure: %s\n' "$*" >&2
  exit 1
}

pass() {
  pass_count=$((pass_count + 1))
  printf 'ok: %s\n' "$1"
}

expect_success() {
  local label="$1"
  shift
  local output
  if ! output="$(run_aura "$@" 2>&1)"; then
    printf '%s\n' "$output" >&2
    fail "$label (expected success)"
  fi
  pass "$label"
}

expect_output() {
  local label="$1"
  local expected="$2"
  shift 2
  local output
  if ! output="$(run_aura "$@" 2>&1)"; then
    printf '%s\n' "$output" >&2
    fail "$label (expected success)"
  fi
  [[ "$output" == *"$expected"* ]] || {
    printf 'expected output fragment: %s\nactual output:\n%s\n' "$expected" "$output" >&2
    fail "$label (output mismatch)"
  }
  pass "$label"
}

expect_command_output() {
  local label="$1"
  local expected="$2"
  shift 2
  local output
  if ! output="$("$@" 2>&1)"; then
    printf '%s\n' "$output" >&2
    fail "$label (expected success)"
  fi
  [[ "$output" == *"$expected"* ]] || {
    printf 'expected output fragment: %s\nactual output:\n%s\n' "$expected" "$output" >&2
    fail "$label (output mismatch)"
  }
  pass "$label"
}

expect_diagnostic() {
  local label="$1"
  local expected="$2"
  shift 2
  local output rc
  set +e
  output="$(run_aura "$@" 2>&1)"
  rc=$?
  set -e
  [[ "$rc" -eq 1 ]] || fail "$label (expected exit 1, got $rc)"
  [[ "$output" == *"$expected"* ]] || {
    printf 'expected diagnostic fragment: %s\nactual output:\n%s\n' "$expected" "$output" >&2
    fail "$label (diagnostic mismatch)"
  }
  pass "$label (exit $rc)"
}

printf '%s\n' '== Green corpus typecheck =='
if ! bash scripts/check-corpus.sh; then
  fail 'green corpus (expected success)'
fi
pass 'green corpus'

printf '%s\n' '== Language feature checks =='
expect_success 'generics' check corpus/generic/id.aura
expect_success 'interfaces' check corpus/iface/named.aura
expect_success 'nullable flow' check corpus/class/nullable.aura
expect_success 'enum and match' check corpus/enum/color.aura
expect_success 'exceptions' check corpus/control/try_catch.aura
expect_success 'package imports' check corpus/import/app
expect_success 'collections' check corpus/std_collections/app
expect_success 'lambdas' check corpus/fun/lambda_basic.aura

printf '%s\n' '== CLI command smoke =='
expect_success 'aura version' version
expect_success 'aura check standalone' check corpus/hello/main.aura
expect_success 'aura check package' check corpus/import/app
expect_output 'aura run' 'Hello, Aura' run corpus/generic/id.aura
expect_output 'forwarded CLI args' 'args ok' run corpus/std_io/args -- hello
expect_output 'aura test' '3 passed' test corpus/test/smoke.aura

build_dir="$(mktemp -d "${TMPDIR:-/tmp}/aura-regression.XXXXXX")"
trap 'rm -rf "$build_dir"' EXIT
expect_success 'aura build' build corpus/hello/main.aura -o "$build_dir/hello"
[[ -x "$build_dir/hello" ]] || fail 'aura build (missing executable)'
pass 'aura build (executable created)'
expect_command_output 'built executable' 'Hello, Aura' "$build_dir/hello"

printf '%s\n' '== Expected diagnostics =='
expect_diagnostic 'undefined name' 'undefined name `missing`' check corpus/diag/undefined.aura
expect_diagnostic 'undefined name suggestion' 'did you mean `count`' check corpus/diag/undefined_typo.aura
expect_diagnostic 'assignment mismatch' 'expected Int, found String' check corpus/diag/assign_mismatch.aura
expect_diagnostic 'unsupported array interface' 'Array` of interface `Named` is not supported yet' check corpus/diag/array_interface.aura
expect_diagnostic 'generic interface arity' 'interface `Iterable` expects 1 type argument(s)' check corpus/diag/generic_iface.aura
expect_diagnostic 'multiple declaration errors' 'duplicate field `a`' check corpus/diag/multi_decl.aura
expect_diagnostic 'multiple body errors' 'undefined name `missing_one`' check corpus/diag/multi_error.aura

printf 'compiler regression matrix passed: %d checks\n' "$pass_count"
