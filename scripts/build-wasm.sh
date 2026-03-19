#!/usr/bin/env bash
# Build the Aura compiler as a WASM package for use in the playground.
#
# Prerequisites:
#   cargo install wasm-pack
#
# Output: website/src/wasm/  (aura_bg.wasm, aura.js, aura.d.ts)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/website/src/wasm}"

echo "Building Aura WASM package..."
echo "  Root : $ROOT_DIR"
echo "  Output: $OUT_DIR"
echo ""

cd "$ROOT_DIR"

wasm-pack build \
  --target web \
  --out-dir "$OUT_DIR" \
  --out-name aura \
  --features wasm

# wasm-pack adds a package.json; remove node_modules hint if present
rm -f "$OUT_DIR/.gitignore"

echo ""
echo "Done! Generated files:"
ls -lh "$OUT_DIR"
