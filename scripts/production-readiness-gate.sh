#!/usr/bin/env bash
# Run repository-only production readiness evidence in one deterministic gate.
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
repeat="${AURA_SOAK_ROUNDS:-3}"
[[ "$repeat" =~ ^[1-9][0-9]*$ ]] || { echo "invalid AURA_SOAK_ROUNDS: $repeat" >&2; exit 2; }
cargo fmt --all -- --check
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p aura-cli
AURA_BIN="$root/target/debug/aura" bash scripts/check-corpus.sh
bash scripts/compiler-regression.sh
bash scripts/async-production-gate.sh
bash scripts/ffi-regression.sh
bash scripts/tests/registry-release.sh
bash scripts/tests/cli-compatibility.sh
bash scripts/tests/website-bundle.sh
for ((round = 1; round <= repeat; round++)); do
  echo "soak round $round/$repeat"
  AURA_BIN="$root/target/debug/aura" bash scripts/check-corpus.sh >/dev/null
  bash scripts/compiler-regression.sh >/dev/null
done
echo 'production readiness gate: PASS'
