#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../../../" && pwd)"

fail() {
  echo "ERROR: $*" 1>&2
  exit 1
}

note() {
  echo "OK: $*"
}

require_file() {
  local path="$1"
  [[ -f "${REPO_ROOT}/${path}" ]] || fail "Missing required file: ${path}"
}

require_file "docs/ARCHITECTURE.md"
require_file "docs/FOLDER_STRUCTURE.md"
require_file "docs/SYNTAX_DESIGN.md"

if command -v rg >/dev/null 2>&1; then
  if rg -n --hidden --glob '!**/.git/**' "\\bfn\\b" "${REPO_ROOT}/docs" >/dev/null 2>&1; then
    fail "Found banned keyword 'fn' in docs/. Aura syntax uses 'function'."
  fi
else
  if grep -RInw -- "fn" "${REPO_ROOT}/docs" >/dev/null 2>&1; then
    fail "Found 'fn' in docs/ (grep fallback). Aura syntax uses 'function'."
  fi
fi

if command -v rg >/dev/null 2>&1; then
  if ! rg -n "Keywords \\(Initial\\)" "${REPO_ROOT}/docs/SYNTAX_DESIGN.md" >/dev/null 2>&1; then
    fail "SYNTAX_DESIGN.md is missing the 'Keywords (Initial)' section."
  fi
  if ! rg -n "\\bfunction\\b" "${REPO_ROOT}/docs/SYNTAX_DESIGN.md" >/dev/null 2>&1; then
    fail "SYNTAX_DESIGN.md does not mention the 'function' keyword."
  fi
else
  if ! grep -n -- "Keywords (Initial)" "${REPO_ROOT}/docs/SYNTAX_DESIGN.md" >/dev/null 2>&1; then
    fail "SYNTAX_DESIGN.md is missing the 'Keywords (Initial)' section (grep fallback)."
  fi
  if ! grep -nw -- "function" "${REPO_ROOT}/docs/SYNTAX_DESIGN.md" >/dev/null 2>&1; then
    fail "SYNTAX_DESIGN.md does not mention the 'function' keyword (grep fallback)."
  fi
fi

note "Contract docs exist and basic guardrails checks pass."
