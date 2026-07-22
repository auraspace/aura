#!/usr/bin/env bash
# R5 race-detector CLI contract and report fixtures.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [[ -n "${AURA_BIN:-}" ]]; then
  bin="$AURA_BIN"
elif [[ -x target/debug/aura ]]; then
  bin=target/debug/aura
else
  cargo build -q -p aura-cli
  bin=target/debug/aura
fi

pass_count=0
pass() {
  pass_count=$((pass_count + 1))
  printf 'ok: %s\n' "$1"
}
fail() {
  printf 'race regression failure: %s\n' "$1" >&2
  exit 1
}

output="$($bin race corpus/test/smoke.aura 2>&1)" || fail 'aura race should pass'
[[ "$output" == *'race: pass (detector=on)'* ]] || fail 'missing stable race pass line'
pass 'aura race pass and detector contract'

json="$($bin race corpus/test/smoke.aura --format json 2>&1)" || fail 'JSON race run should pass'
[[ "$json" == '{"mode":"race","detector":true,"result":'* ]] || fail 'unstable JSON race envelope'
pass 'aura race JSON envelope'

set +e
$bin race corpus/test/smoke.aura --unknown >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 ]] || fail "invalid race option exit behavior: got $rc"
pass 'invalid race option exits 2'

set +e
$bin race corpus/hello/main.aura >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 1 ]] || fail "race failure exit behavior: got $rc"
pass 'race failure exits 1'

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-race-regression.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
  -o "$tmp/race-report" runtime/tests/race_report.c
"$tmp/race-report" >"$tmp/report.txt"
pass 'planted-race, race-free, channel, and suppression fixtures (C asserts)'

for fixture in race_tracker ffi_owned; do
  cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined \
    -o "$tmp/$fixture" "runtime/tests/$fixture.c"
  "$tmp/$fixture"
done
pass 'cancellation and GC lifecycle fixtures'

release_c="$tmp/release.c"
$bin emit-c corpus/hello/main.aura >"$release_c" 2>/dev/null || fail 'release-shaped emit-c failed'
if grep -q '__aura_race_tracker = aura_race_tracker_new' "$release_c"; then
  fail 'default/release-shaped artifact contains active detector state'
fi
pass 'release-shaped artifact has no active detector state'

printf 'race regression matrix passed: %d checks\n' "$pass_count"
