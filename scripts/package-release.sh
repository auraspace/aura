#!/usr/bin/env bash
# Build a portable aura toolchain tarball (RFC-013 layout, alpha).
# Usage: scripts/package-release.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(grep -E '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')"
VERSION="${VERSION:-0.1.0}"

# Prefer explicit TAG_VERSION, then git tag (v0.1.0-alpha → 0.1.0-alpha), then Cargo + -alpha.
if [[ -z "${TAG_VERSION:-}" ]]; then
  if [[ -n "${GITHUB_REF_NAME:-}" && "${GITHUB_REF_NAME}" == v* ]]; then
    TAG_VERSION="${GITHUB_REF_NAME#v}"
  else
    TAG_VERSION="${VERSION}-alpha"
  fi
else
  # Allow callers to pass v0.1.0-alpha
  TAG_VERSION="${TAG_VERSION#v}"
fi

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
esac

NAME="aura-${TAG_VERSION}-${OS}-${ARCH}"
DIST="$ROOT/dist"
STAGE="$DIST/$NAME"

echo "packaging $NAME"

rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/share/aura"

cargo build -p aura-cli --release
BIN="$ROOT/target/release/aura"
if [[ ! -x "$BIN" ]]; then
  echo "error: missing $BIN" >&2
  exit 1
fi

cp "$BIN" "$STAGE/bin/aura"
cp "$ROOT/runtime/aura_rt.c" "$STAGE/share/aura/aura_rt.c"
cp "$ROOT/LICENSE" "$STAGE/LICENSE"
cat >"$STAGE/README.txt" <<EOF
Aura toolchain ${TAG_VERSION} (${OS}/${ARCH})

Install:
  export PATH="\$PWD/bin:\$PATH"
  aura version
  aura new hello && aura run hello

Runtime:
  share/aura/aura_rt.c is included; the CLI also embeds a copy.
  Optional: export AURA_RUNTIME="\$PWD/share/aura/aura_rt.c"

Docs: https://aura.fadosoft.com
Freeze: docs/releases/0.1.0-alpha.md
EOF

TAR="$DIST/${NAME}.tar.gz"
mkdir -p "$DIST"
tar -C "$DIST" -czf "$TAR" "$NAME"
echo "wrote $TAR"
if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "$TAR" | tee "$TAR.sha256"
elif command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$TAR" | tee "$TAR.sha256"
fi
