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

# Optional cross-compile: RUST_TARGET=x86_64-apple-darwin (GitHub no longer hosts macos-13 Intel).
# When unset, build for the host triple and name the artifact from uname.
if [[ -n "${RUST_TARGET:-}" ]]; then
  case "$RUST_TARGET" in
    x86_64-apple-darwin) OS=darwin; ARCH=amd64 ;;
    aarch64-apple-darwin|arm64-apple-darwin) OS=darwin; ARCH=arm64 ;;
    x86_64-unknown-linux-gnu) OS=linux; ARCH=amd64 ;;
    aarch64-unknown-linux-gnu) OS=linux; ARCH=arm64 ;;
    x86_64-pc-windows-msvc) OS=windows; ARCH=amd64 ;;
    aarch64-pc-windows-msvc) OS=windows; ARCH=arm64 ;;
    *)
      echo "error: unsupported RUST_TARGET=$RUST_TARGET" >&2
      exit 1
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
  esac

  cargo build -p aura-cli --release
  BIN="$ROOT/target/release/aura"
fi

NAME="aura-${TAG_VERSION}-${OS}-${ARCH}"
DIST="$ROOT/dist"
STAGE="$DIST/$NAME"

echo "packaging $NAME"

rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/share/aura/std"

if [[ ! -f "$BIN" ]]; then
  echo "error: missing $BIN" >&2
  exit 1
fi

cp "$BIN" "$STAGE/bin/aura"
cp "$ROOT/runtime/aura_rt.c" "$STAGE/share/aura/aura_rt.c"
# Std packages for import / auto-prelude outside the monorepo.
for pkg in io assert collections; do
  if [[ -d "$ROOT/std/$pkg" ]]; then
    mkdir -p "$STAGE/share/aura/std/$pkg"
    # Copy package tree without junk.
    if command -v rsync >/dev/null 2>&1; then
      rsync -a --exclude '.DS_Store' --exclude 'README.md' "$ROOT/std/$pkg/" "$STAGE/share/aura/std/$pkg/"
    else
      cp -R "$ROOT/std/$pkg/." "$STAGE/share/aura/std/$pkg/"
      find "$STAGE/share/aura/std/$pkg" -name '.DS_Store' -delete 2>/dev/null || true
    fi
  fi
done
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

Standard library:
  share/aura/std/{io,assert,collections} — used by auto-prelude and \`import std.*\`.
  Optional: export AURA_STD="\$PWD/share/aura/std"

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
