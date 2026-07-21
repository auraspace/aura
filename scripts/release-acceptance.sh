#!/usr/bin/env bash
# Run the production release acceptance gate from a clean, isolated environment.
#
# Usage:
#   bash scripts/release-acceptance.sh             # complete offline gate
#   bash scripts/release-acceptance.sh --dry-run   # show stages without running
#   bash scripts/release-acceptance.sh --network   # also smoke the published CDN
#   bash scripts/release-acceptance.sh --keep-temp # retain temporary HOME/cache
#
# The default gate needs no GitHub credentials or registry access. The optional
# network stage is deliberately separate because it checks the published
# install.sh rather than the local release artifact.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

dry_run=0
network=0
keep_temp=0

usage() {
  sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
}

die() {
  printf 'release acceptance: error: %s\n' "$*" >&2
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) dry_run=1 ;;
    --network) network=1 ;;
    --keep-temp) keep_temp=1 ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

tmp=""
cleanup() {
  if [[ "$keep_temp" -eq 1 ]]; then
    printf 'release acceptance: retained temp root: %s\n' "$tmp"
  elif [[ -n "$tmp" ]]; then
    rm -rf "$tmp"
  fi
}
trap cleanup EXIT

run_stage() {
  local name="$1"
  shift
  printf '\n==> %s\n' "$name"
  printf '    command:'
  printf ' %q' "$@"
  printf '\n'
  if [[ "$dry_run" -eq 1 ]]; then
    printf '    dry-run: skipped\n'
    return 0
  fi
  if "$@"; then
    printf '    PASS: %s\n' "$name"
  else
    local status=$?
    printf '    FAIL: %s (exit %d)\n' "$name" "$status" >&2
    printf '    Re-run this stage from %s with the command above.\n' "$root" >&2
    exit "$status"
  fi
}

report_native_scope() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os/$arch" in
    Linux/x86_64|Linux/amd64|Darwin/arm64|Darwin/aarch64|Darwin/x86_64|Darwin/amd64)
      printf '    native target exercised by this run: %s/%s\n' "$os" "$arch"
      printf '    other supported targets require their own native host; no cross-target runtime claim is made\n'
      ;;
    *)
      printf '    unsupported host for native release smoke: %s/%s\n' "$os" "$arch" >&2
      printf '    no supported native target is being claimed by this run\n' >&2
      ;;
  esac
}

if [[ "$dry_run" -eq 0 ]]; then
  caller_home="${HOME:-}"
  caller_rustup_home="${RUSTUP_HOME:-}"
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-release-acceptance.XXXXXX")"
  export HOME="$tmp/home"
  export XDG_CACHE_HOME="$tmp/cache"
  export AURA_REGISTRY_CACHE="$tmp/registry-cache"
  # Keep an existing Cargo installation/cache available, while isolating
  # application and registry state from the user's real home directory.
  if [[ -z "${CARGO_HOME:-}" && -n "$caller_home" ]]; then
    export CARGO_HOME="$caller_home/.cargo"
  fi
  if [[ -z "$caller_rustup_home" && -n "$caller_home" ]]; then
    export RUSTUP_HOME="$caller_home/.rustup"
  fi
  mkdir -p "$HOME" "$XDG_CACHE_HOME" "$AURA_REGISTRY_CACHE"
  printf 'release acceptance: isolated HOME/cache at %s\n' "$tmp"
fi

run_stage "native host scope" report_native_scope

run_stage "workspace tests" cargo test --workspace
run_stage "Clippy warnings gate" cargo clippy --workspace --all-targets -- -D warnings
run_stage "corpus gate" bash scripts/check-corpus.sh
run_stage "compiler regression matrix" bash scripts/compiler-regression.sh
run_stage "debug CLI build for sanitizer gate" cargo build -p aura-cli
run_stage "sanitizer smoke gate" bash scripts/sanitizer-smoke.sh
run_stage "local release package and install smoke" bash scripts/install-smoke.sh --local-pkg

if [[ "$network" -eq 1 ]]; then
  run_stage "published installer network smoke" bash scripts/install-smoke.sh --from-release
else
  printf '\n==> published installer network smoke\n'
  printf '    skipped (opt in with --network; offline gate remains credential-free)\n'
fi

run_stage "working-tree whitespace check" git diff --check

printf '\nrelease acceptance: all requested offline gates passed\n'
