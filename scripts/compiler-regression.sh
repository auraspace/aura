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
expect_output 'aura test' 'Passed: 3 tests' test corpus/test/smoke.aura

printf '%s\n' '== Executable corpus matrix =='
expect_output 'array push and iteration' 'array push ok' run corpus/generic/array_push.aura
expect_output 'struct copy semantics' 'copy' run corpus/struct/point.aura
expect_output 'enum match' 'green' run corpus/enum/color.aura
expect_output 'enum String payload cleanup' 'enum-string' run corpus/enum/string_payload_cleanup.aura
expect_output 'exclusive range loop' 'for-range ok' run corpus/control/for_range.aura
expect_output 'inclusive range loop' 'for-inclusive ok' run corpus/control/for_inclusive.aura
expect_output 'break and continue' 'while-hit' run corpus/control/break_continue.aura
expect_output 'string split' 'ok' run corpus/expr/string_split.aura
expect_output 'string trimming' 'ok' run corpus/expr/string_trim.aura
expect_output 'string integer parsing' 'ov-' run corpus/expr/string_toint.aura
expect_output 'string interpolation' 'mid' run corpus/expr/string_interp.aura
expect_output 'string concatenation' 'mix' run corpus/expr/string_concat.aura
expect_output 'class greeting' 'Hello, Aura' run corpus/class/greeter.aura
expect_output 'class identity' 'distinct' run corpus/class/identity.aura
expect_output 'nullable safe call' 'null' run corpus/class/safe_call.aura
expect_output 'primitive optional values' 'xnone' run corpus/types/opt_prim.aura
expect_output 'null coalescing' 'hi' run corpus/types/coalesce.aura
expect_success 'higher-order lambda execution' run corpus/fun/lambda_hof.aura
expect_success 'captured lambda execution' run corpus/fun/lambda_capture.aura
expect_success 'captured class lambda execution' run corpus/fun/lambda_capture_class.aura
expect_success 'captured array lambda execution' run corpus/fun/lambda_capture_array.aura
expect_success 'mutable capture lambda execution' run corpus/fun/lambda_capture_var.aura
expect_output 'async no-await execution' 'async' run corpus/async/no_await.aura
expect_output 'async task lifecycle' 'resumed' run corpus/async/task_lifecycle.aura
expect_output 'async multi-await execution' 'four-await-ok' run corpus/async/multi_await_four.aura
expect_output 'async mutable capture execution' $'2\n2' run corpus/async/mutable_spawn_capture.aura
expect_output 'std.test assertions' 'tests-ok' run corpus/std_test/assertions
expect_output 'std.json validation' 'invalid' run corpus/std_json/basic

expect_output 'generic collection live-entry returns' 'generic-int-entry-iteration-ok' run corpus/std_collections/hashmap_int
expect_output 'generic string-map live-entry returns' 'generic-string-entry-iteration-ok' run corpus/std_collections/hashmap_str
expect_output 'generic hashset generic returns' 'generic-set-hof-ok' run corpus/std_collections/hashset_int
expect_output 'generic collection join' 'ok' run corpus/std_collections/join

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
expect_success 'array interface' check corpus/diag/array_interface.aura
expect_diagnostic 'generic interface arity' 'interface `Iterable` expects 1 type argument(s)' check corpus/diag/generic_iface.aura
expect_diagnostic 'multiple declaration errors' 'duplicate field `a`' check corpus/diag/multi_decl.aura
expect_diagnostic 'multiple body errors' 'undefined name `missing_one`' check corpus/diag/multi_error.aura
expect_diagnostic 'missing return' 'missing return: expected Int' check corpus/diag/missing_return.aura
expect_diagnostic 'return type mismatch' 'return type mismatch: expected Int, got String' check corpus/diag/return_mismatch.aura
expect_diagnostic 'non-bool condition' 'if condition must be Bool, got Int' check corpus/diag/non_bool_condition.aura
expect_diagnostic 'break outside loop' '`break` is only valid inside a loop' check corpus/diag/break_outside_loop.aura

printf 'compiler regression matrix passed: %d checks\n' "$pass_count"
