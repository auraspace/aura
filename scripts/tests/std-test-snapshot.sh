#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-snapshot.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

run_snapshot() {
  AURA_SNAPSHOT_DIR="$tmp" cargo run -q -p aura-cli -- run "$root/corpus/std_test/snapshot/aura.toml"
}

AURA_UPDATE_SNAPSHOTS=1 AURA_SNAPSHOT_DIR="$tmp" \
  cargo run -q -p aura-cli -- run "$root/corpus/std_test/snapshot/aura.toml" >/dev/null
[[ "$(<"$tmp/smoke.snap")" == "value" ]]
[[ "$(run_snapshot)" == "snapshot-ok" ]]

printf 'changed\n' >"$tmp/smoke.snap"
if run_snapshot >/dev/null 2>&1; then
  echo "snapshot: mismatch was accepted" >&2
  exit 1
fi

echo "std.test snapshot: PASS"
