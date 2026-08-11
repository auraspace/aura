#!/usr/bin/env bash
# Exercise the complete offline release/install/rollback path in isolation.
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
bash scripts/release-acceptance.sh
echo 'release rehearsal gate: PASS'
