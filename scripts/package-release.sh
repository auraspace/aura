#!/usr/bin/env bash
# Build a portable aura toolchain tarball (RFC-013 layout, alpha).
# Usage: scripts/package-release.sh [--validate-target TARGET]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

die() {
  echo "error: $*" >&2
  exit 1
}

# Host-independent contract check. This never invokes a compiler or runs a
# foreign executable; it only answers whether this script supports a release
# target declared by the policy.
if [[ "${1:-}" == "--validate-target" ]]; then
  [[ $# -eq 2 ]] || die "usage: $0 --validate-target TARGET"
  case "$2" in
    linux-amd64|darwin-arm64|darwin-amd64)
      printf 'package target: supported %s (build capability not exercised)\n' "$2"
      exit 0
      ;;
    linux-arm64|windows-amd64|windows-arm64)
      die "target $2 is policy-only in alpha; no package artifact is produced"
      ;;
    *) die "unknown release target: $2" ;;
  esac
fi
[[ $# -eq 0 ]] || die "unknown option: $1"

VERSION="$(grep -E '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')"
VERSION="${VERSION:-0.1.1-alpha}"

# Prefer explicit TAG_VERSION, then the pushed tag, then Cargo + -alpha.
if [[ -z "${TAG_VERSION:-}" ]]; then
  if [[ -n "${GITHUB_REF_NAME:-}" && "${GITHUB_REF_NAME}" == v* ]]; then
    TAG_VERSION="${GITHUB_REF_NAME#v}"
  else
    if [[ "$VERSION" == *-* ]]; then
      TAG_VERSION="$VERSION"
    else
      TAG_VERSION="${VERSION}-alpha"
    fi
  fi
else
  # Allow callers to pass a tag with a leading v.
  TAG_VERSION="${TAG_VERSION#v}"
fi

# Optional cross-compile: RUST_TARGET=x86_64-apple-darwin (GitHub no longer hosts macos-13 Intel).
# When unset, build for the host triple and name the artifact from uname.
if [[ -n "${RUST_TARGET:-}" ]]; then
  case "$RUST_TARGET" in
    x86_64-apple-darwin) OS=darwin; ARCH=amd64 ;;
    aarch64-apple-darwin|arm64-apple-darwin) OS=darwin; ARCH=arm64 ;;
    x86_64-unknown-linux-gnu) OS=linux; ARCH=amd64 ;;
    *)
      die "unsupported RUST_TARGET=$RUST_TARGET (supported: x86_64-unknown-linux-gnu, aarch64-apple-darwin, x86_64-apple-darwin)"
      ;;
  esac
  echo "cross-compiling for $RUST_TARGET → ${OS}/${ARCH}"
  rustup target add "$RUST_TARGET" >/dev/null
  cargo build -p aura-cli --release --target "$RUST_TARGET"
  BIN="$ROOT/target/${RUST_TARGET}/release/aura"
  # Windows produces aura.exe; alpha matrix is Unix-only today.
  if [[ ! -x "$BIN" && -f "${BIN}.exe" ]]; then
    BIN="${BIN}.exe"
  fi
else
  OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
  # Normalize OS names used in artifact filenames (uname -s on Linux is "linux").
  case "$OS" in
    linux*) OS=linux ;;
    darwin*) OS=darwin ;;
    mingw*|msys*|cygwin*) OS=windows ;;
  esac

  ARCH="$(uname -m)"
  case "$ARCH" in
    x86_64|amd64) ARCH=amd64 ;;
    aarch64|arm64) ARCH=arm64 ;;
    *) die "unsupported host architecture: $(uname -m)" ;;
  esac

  case "$OS/$ARCH" in
    linux/amd64|darwin/arm64|darwin/amd64) ;;
    *) die "unsupported host platform: ${OS}/${ARCH} (supported: linux/amd64, darwin/arm64, darwin/amd64)" ;;
  esac

  cargo build -p aura-cli --release
  BIN="$ROOT/target/release/aura"
fi

NAME="aura-${TAG_VERSION}-${OS}-${ARCH}"
DIST="${AURA_DIST_DIR:-$ROOT/dist}"
STAGE="$DIST/$NAME"

echo "packaging $NAME"

rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/share/aura/runtime" "$STAGE/share/aura/std"

[[ -x "$BIN" ]] || die "missing executable $BIN"

cp "$BIN" "$STAGE/bin/aura"
cp "$ROOT/runtime/runtime.c" "$STAGE/share/aura/runtime/runtime.c"
cp "$ROOT/runtime/aura_ffi.h" "$STAGE/share/aura/runtime/aura_ffi.h"
cp -R "$ROOT/runtime/src" "$STAGE/share/aura/runtime/src"
[[ -s "$ROOT/runtime/runtime.c" ]] || die "runtime source is missing or empty"
# Std packages for import / auto-prelude outside the monorepo.
shopt -s nullglob
std_packages=("$ROOT"/std/*)
(( ${#std_packages[@]} > 0 )) || die "no std packages found under $ROOT/std"
for package_dir in "${std_packages[@]}"; do
  [[ -d "$package_dir" ]] || continue
  pkg="$(basename "$package_dir")"
  [[ -f "$package_dir/aura.toml" ]] || die "std package manifest is missing: std/$pkg/aura.toml"
  find "$package_dir" -type f -print -quit | grep -q . || die "required std package is empty: std/$pkg"
  mkdir -p "$STAGE/share/aura/std/$pkg"
  # Copy package tree without junk.
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --exclude '.DS_Store' --exclude 'README.md' "$package_dir/" "$STAGE/share/aura/std/$pkg/"
  else
    cp -R "$package_dir/." "$STAGE/share/aura/std/$pkg/"
    find "$STAGE/share/aura/std/$pkg" -name '.DS_Store' -delete 2>/dev/null || true
  fi
  find "$STAGE/share/aura/std/$pkg" -type f -print -quit | grep -q . \
    || die "required std package copied no files: std/$pkg"
done
cp "$ROOT/LICENSE" "$STAGE/LICENSE"
cat >"$STAGE/README.txt" <<EOF
Aura toolchain ${TAG_VERSION} (${OS}/${ARCH})

Install:
  export PATH="\$PWD/bin:\$PATH"
  aura version
  aura new hello && aura run hello

Runtime:
  share/aura/runtime/ is included; the CLI also embeds a copy.
  Optional: export AURA_RUNTIME="\$PWD/share/aura/runtime/runtime.c"

Standard library:
  share/aura/std/* — all standard-library packages used by auto-prelude and \`import std.*\`.
  Optional: export AURA_STD="\$PWD/share/aura/std"

Docs: https://aura.fadosoft.com
Release notes: docs/releases/${TAG_VERSION}.md
EOF

TAR="$DIST/${NAME}.tar.gz"
CHECKSUM="$TAR.sha256"
[[ ! -e "$TAR" && ! -e "$CHECKSUM" ]] || rm -f "$TAR" "$CHECKSUM"

# Normalize metadata before archiving. The sorted file list and fixed ownership,
# timestamps, and gzip header keep repeated builds byte-for-byte stable.
find "$STAGE" -exec touch -t 197001010000 {} +
TAR_TMP="$(mktemp "$DIST/.${NAME}.tar.XXXXXX")"
GZ_TMP="$(mktemp "$DIST/.${NAME}.tar.gz.XXXXXX")"
trap 'rm -f "$TAR_TMP" "$GZ_TMP"' EXIT

TAR_METADATA=(--uid 0 --gid 0 --uname root --gname root)
if tar --version 2>/dev/null | grep -q 'GNU tar'; then
  TAR_METADATA=(--format=ustar --owner=0 --group=0 --numeric-owner --mtime='1970-01-01 00:00:00 UTC' --sort=name)
fi
(cd "$DIST" && find "$NAME" -print | LC_ALL=C sort) | tar -C "$DIST" -cf "$TAR_TMP" "${TAR_METADATA[@]}" -T -
gzip -n -c "$TAR_TMP" >"$GZ_TMP"
mv "$GZ_TMP" "$TAR"
rm -f "$TAR_TMP"

write_checksum() {
  local archive="$1" output="$2"
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$(dirname "$archive")" && sha256sum "$(basename "$archive")") >"$output"
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$(dirname "$archive")" && shasum -a 256 "$(basename "$archive")") >"$output"
  else
    die "no SHA-256 utility found (need sha256sum or shasum)"
  fi
}

verify_checksum() {
  local checksum="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$(dirname "$checksum")" && sha256sum --check "$(basename "$checksum")")
  else
    (cd "$(dirname "$checksum")" && shasum -a 256 --check "$(basename "$checksum")")
  fi
}

write_checksum "$TAR" "$CHECKSUM"
verify_checksum "$CHECKSUM" >/dev/null

# Verify the release contract against the archive, not only the staging tree.
ARCHIVE_LISTING="$(tar -tzf "$TAR")"
archive_has_path() {
  local path="$1"
  printf '%s\n' "$ARCHIVE_LISTING" | awk -v path="$path" \
    '$0 == path || $0 == path "/" || index($0, path "/") == 1 { found = 1 } END { exit !found }'
}

for required in \
  "$NAME/bin/aura" \
  "$NAME/share/aura/runtime/runtime.c" \
  "$NAME/LICENSE" \
  "$NAME/README.txt"; do
  archive_has_path "$required" || die "archive is missing $required"
done
for package_dir in "${std_packages[@]}"; do
  [[ -d "$package_dir" ]] || continue
  pkg="$(basename "$package_dir")"
  archive_has_path "$NAME/share/aura/std/$pkg" || die "archive is missing std/$pkg"
done
README_CONTENT="$(tar -xOzf "$TAR" "$NAME/README.txt")"
[[ "$README_CONTENT" == *"Aura toolchain ${TAG_VERSION} (${OS}/${ARCH})"* ]] \
  || die "archive README has incorrect version or platform metadata"

echo "wrote $TAR"
echo "wrote $CHECKSUM"
