#!/usr/bin/env bash
set -euo pipefail

# Deterministic fuzz-target build gate for environments without cargo-fuzz.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cargo check --manifest-path "$repo_root/fuzz/Cargo.toml" --bins
echo 'fuzz target build: PASS'
