#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-release-bundle-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
assets="$tmp/assets"
acceptance="$tmp/acceptance"
mkdir -p "$assets" "$acceptance"
version="0.1.1-alpha.fixture"

for target in linux-amd64 darwin-arm64 darwin-amd64; do
  printf 'fixture artifact for %s\n' "$target" >"$assets/aura-${version}-${target}.tar.gz"
  (cd "$assets" && sha256sum "aura-${version}-${target}.tar.gz" >"aura-${version}-${target}.tar.gz.sha256")
  mode=native
  [[ "$target" == darwin-amd64 ]] && mode=cross-file
  printf '{"schema_version":1,"target":"%s","mode":"%s","format":"fixture","outcome":"pass"}\n' \
    "$target" "$mode" >"$acceptance/$target-acceptance.json"
done

bash scripts/generate-release-manifest.sh \
  --dir "$assets" --acceptance-dir "$acceptance" --version "$version"

(cd "$assets" && {
  find . -maxdepth 1 -type f \
    ! -name 'SHA256SUMS' ! -name 'SHA256SUMS.minisig' ! -name 'minisign.pub' \
    -print | sed 's#^./##' | LC_ALL=C sort | while IFS= read -r file; do sha256sum "$file"; done
}) >"$assets/SHA256SUMS"

bash scripts/validate-release-bundle.sh \
  --dir "$assets" --acceptance-dir "$acceptance" --version "$version"

printf 'tampered artifact\n' >>"$assets/aura-${version}-linux-amd64.tar.gz"
if bash scripts/validate-release-bundle.sh \
  --dir "$assets" --acceptance-dir "$acceptance" --version "$version" \
  >/dev/null 2>&1; then
  printf 'release bundle test: artifact checksum drift was accepted\n' >&2
  exit 1
fi
printf 'fixture artifact for linux-amd64\n' >"$assets/aura-${version}-linux-amd64.tar.gz"

if bash scripts/validate-release-bundle.sh \
  --dir "$assets" --acceptance-dir "$acceptance" --version "$version" --require-signature \
  >/dev/null 2>&1; then
  printf 'release bundle test: unsigned fixture was accepted\n' >&2
  exit 1
fi

cp "$acceptance/linux-amd64-acceptance.json" "$acceptance/linux-amd64-acceptance.json.bak"
sed 's/linux-amd64/darwin-arm64/' "$acceptance/linux-amd64-acceptance.json.bak" >"$acceptance/linux-amd64-acceptance.json"
if bash scripts/generate-release-manifest.sh \
  --dir "$assets" --acceptance-dir "$acceptance" --version "$version" \
  >/dev/null 2>&1; then
  printf 'release bundle test: acceptance target drift was accepted\n' >&2
  exit 1
fi

printf 'release bundle tests: PASS\n'
