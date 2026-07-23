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
  host_os=darwin
  host_arch=arm64
  execution=not-run
  if [[ "$mode" == native ]]; then
    execution=ran
    case "$target" in
      linux-amd64) host_os=linux; host_arch=amd64 ;;
      darwin-arm64) host_os=darwin; host_arch=arm64 ;;
    esac
  fi
  printf '{"schema_version":2,"target":"%s","mode":"%s","format":"fixture","host":{"os":"%s","arch":"%s"},"execution":"%s","outcome":"pass"}\n' \
    "$target" "$mode" "$host_os" "$host_arch" "$execution" >"$acceptance/$target-acceptance.json"
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
mv "$acceptance/linux-amd64-acceptance.json.bak" "$acceptance/linux-amd64-acceptance.json"

cp "$acceptance/darwin-arm64-acceptance.json" "$acceptance/darwin-arm64-acceptance.json.bak"
sed 's/"execution":"ran"/"execution":"not-run"/' "$acceptance/darwin-arm64-acceptance.json.bak" >"$acceptance/darwin-arm64-acceptance.json"
if bash scripts/generate-release-manifest.sh \
  --dir "$assets" --acceptance-dir "$acceptance" --version "$version" \
  >/dev/null 2>&1; then
  printf 'release bundle test: native acceptance without execution was accepted\n' >&2
  exit 1
fi

printf 'release bundle tests: PASS\n'
