#!/usr/bin/env bash
# Verify stable CLI command names, exit classes, and machine-readable outputs.
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
bin="${AURA_BIN:-$root/target/debug/aura}"
[[ -x "$bin" ]] || { echo "build aura first: $bin" >&2; exit 2; }
expect_status() {
  local expected="$1"; shift
  set +e
  "$@" >/dev/null 2>&1
  local actual=$?
  set -e
  [[ "$actual" -eq "$expected" ]] || { echo "expected exit $expected, got $actual: $*" >&2; exit 1; }
}
for command in check build run test bench race update add remove fmt fix clean doc tree toolchain emit-c emit-llvm language-server new init version help; do
  "$bin" help 2>&1 | grep -Eq "(^|[[:space:]])${command}([[:space:]]|$)" || { echo "command missing from help: $command" >&2; exit 1; }
done
expect_status 2 "$bin" does-not-exist
expect_status 2 "$bin" toolchain --format nope
isolated_home="$(mktemp -d)"
trap 'rm -rf "$isolated_home"' EXIT
expect_status 1 env AURA_HOME="$isolated_home" "$bin" toolchain current
json="$(AURA_HOME="$isolated_home" "$bin" toolchain list --format json)"
[[ "$json" == '{"ok":true,"current":'*'"versions":['* ]] || { echo "toolchain JSON contract mismatch: $json" >&2; exit 1; }
tree="$($bin tree corpus/std_test/assertions --format json)"
[[ "$tree" == '{"package":'*'"dependencies":['* ]] || { echo "tree JSON contract mismatch: $tree" >&2; exit 1; }
echo 'CLI compatibility gate: PASS'
