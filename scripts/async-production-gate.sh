#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo test -p aura-codegen --lib -- \
  builds_and_runs_general_cfg_spawn_with_branch_and_loop_awaits \
  builds_and_runs_general_cfg_for_range_await_with_gc_repeated_join_and_cancel \
  builds_and_runs_nested_branch_await_state_machine \
  builds_and_runs_general_four_await_array_state_machine

bin="${AURA_BIN:-target/debug/aura}"
if [[ ! -x "$bin" ]]; then
  cargo build -p aura-cli
fi

"$bin" check corpus/async/multi_await_four.aura >/dev/null
"$bin" check corpus/async/mutable_spawn_capture.aura >/dev/null
"$bin" check corpus/async/task_outcome/aura.toml >/dev/null
output="$("$bin" run corpus/async/task_outcome/aura.toml 2>&1)"
grep -q '^7$' <<<"$output"

echo "async production gate: PASS"
