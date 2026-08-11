#!/usr/bin/env bash
# Build the production site and reject emitted JS/CSS assets over 500 KiB.
set -Eeuo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
pnpm site:test
pnpm site:build
while IFS= read -r -d '' asset; do
  size=$(wc -c <"$asset")
  (( size <= 512000 )) || { echo "website bundle exceeds 500 KiB: $asset ($size bytes)" >&2; exit 1; }
done < <(find site/dist/assets -type f \( -name '*.js' -o -name '*.css' \) -print0)
echo 'website bundle gate: PASS'
