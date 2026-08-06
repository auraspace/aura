#!/usr/bin/env bash
# Credentialed, network-required acceptance for a real public Aura package.
# The repository/tag are supplied by the release operator; no credentials are
# written to the temporary manifest, cache, lockfile, or evidence report.
# Set AURA_PUBLIC_SUBDIR for a package nested in a public monorepo.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

: "${AURA_PUBLIC_ORIGIN:?set AURA_PUBLIC_ORIGIN to an HTTPS/SSH Git repository}"
: "${AURA_PUBLIC_PACKAGE:?set AURA_PUBLIC_PACKAGE to the package name in aura.toml}"
: "${AURA_PUBLIC_TAG:?set AURA_PUBLIC_TAG to an immutable semver tag such as v1.0.0}"
: "${AURA_PUBLIC_SUBDIR:=}"
: "${AURA_PUBLIC_REPORT:=$root/target/alpha-reports/public-origin.json}"

[[ "$AURA_PUBLIC_ORIGIN" == https://* || "$AURA_PUBLIC_ORIGIN" == ssh://* || "$AURA_PUBLIC_ORIGIN" == git@* ]] || {
  printf 'public origin acceptance: origin must use HTTPS or SSH\n' >&2
  exit 2
}
[[ "$AURA_PUBLIC_PACKAGE" =~ ^[A-Za-z0-9_.-]+$ ]] || {
  printf 'public origin acceptance: package name contains unsafe characters\n' >&2
  exit 2
}
[[ "$AURA_PUBLIC_TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-.][A-Za-z0-9.-]+)?$ ]] || {
  printf 'public origin acceptance: tag is not an Aura semver tag\n' >&2
  exit 2
}
if [[ -n "$AURA_PUBLIC_SUBDIR" ]]; then
  [[ "$AURA_PUBLIC_SUBDIR" != /* && "$AURA_PUBLIC_SUBDIR" != *..* && "$AURA_PUBLIC_SUBDIR" != *//* ]] || {
    printf 'public origin acceptance: subdir must be normalized and relative\n' >&2
    exit 2
  }
  origin_source="$AURA_PUBLIC_ORIGIN"
else
  origin_source="$AURA_PUBLIC_ORIGIN"
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-public-origin.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/app/src" "$tmp/cache"

cat >"$tmp/app/aura.toml" <<EOF
[package]
name = "public.origin.acceptance"

[dependencies]
$AURA_PUBLIC_PACKAGE = { git = "$origin_source", subdir = "$AURA_PUBLIC_SUBDIR", tag = "$AURA_PUBLIC_TAG" }
EOF
cat >"$tmp/app/src/main.aura" <<EOF
package public.origin.acceptance
import $AURA_PUBLIC_PACKAGE
fun main() {}
EOF

AURA_REGISTRY_CACHE="$tmp/cache" cargo run -q -p aura-cli -- check "$tmp/app/aura.toml"
lock="$tmp/app/aura.lock"
[[ -s "$lock" ]] || { printf 'public origin acceptance: no lockfile was written\n' >&2; exit 1; }
grep -Eq "^$AURA_PUBLIC_PACKAGE = .*source = \\\"git\\+" "$lock"
grep -Eq 'rev = "[0-9a-fA-F]{40}"' "$lock"
grep -Eq 'checksum = "sha256:[0-9a-fA-F]{64}"' "$lock"
rev="$(sed -n "s/.*rev = \"\([0-9a-fA-F]\{40\}\)\".*/\1/p" "$lock" | head -1)"
checksum="$(sed -n "s/.*checksum = \"sha256:\([0-9a-fA-F]\{64\}\)\".*/\1/p" "$lock" | head -1)"
mkdir -p "$(dirname "$AURA_PUBLIC_REPORT")"
printf '{"schema_version":1,"network":true,"package":"%s","origin":"%s","subdir":"%s","tag":"%s","rev":"%s","checksum":"sha256:%s","outcome":"pass"}\n' \
  "$AURA_PUBLIC_PACKAGE" "$AURA_PUBLIC_ORIGIN" "$AURA_PUBLIC_SUBDIR" "$AURA_PUBLIC_TAG" "$rev" "$checksum" > "$AURA_PUBLIC_REPORT"

printf 'public origin acceptance: PASS (%s %s %s)\n' \
  "$AURA_PUBLIC_PACKAGE" "$AURA_PUBLIC_TAG" "$origin_source"
