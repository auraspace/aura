#!/usr/bin/env bash
# Build the Aura compiler as a WASM package and run its tests.
#
# Prerequisite: Node.js (with modules support)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WASM_LIB_DIR="$ROOT_DIR/tests/wasm/lib"

echo "Building Aura WASM library for testing..."
"$SCRIPT_DIR/build-wasm.sh" "$WASM_LIB_DIR"

echo ""
echo "Running WASM tests using Node.js..."
cd "$ROOT_DIR/tests/wasm"

# Ensure runtime dependencies or environment (if any) are set here
# For now, we just run the test runner
node run.js "$@"
