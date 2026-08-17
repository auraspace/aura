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
    linux-amd64|linux-arm64|darwin-arm64|darwin-amd64)
      printf 'package target: supported %s (build capability not exercised)\n' "$2"
      exit 0
      ;;
    windows-amd64|windows-arm64)
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
    aarch64-unknown-linux-gnu) OS=linux; ARCH=arm64 ;;
    *)
      die "unsupported RUST_TARGET=$RUST_TARGET (supported: x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, aarch64-apple-darwin, x86_64-apple-darwin)"
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
    linux/amd64|linux/arm64|darwin/arm64|darwin/amd64) ;;
    *) die "unsupported host platform: ${OS}/${ARCH} (supported: linux/amd64, linux/arm64, darwin/arm64, darwin/amd64)" ;;
  esac

  cargo build -p aura-cli --release
  BIN="$ROOT/target/release/aura"
fi

NAME="aura-${TAG_VERSION}-${OS}-${ARCH}"
DIST="${AURA_DIST_DIR:-$ROOT/dist}"
STAGE="$DIST/$NAME"
TARGET_NAME="${OS}-${ARCH}"
TARGET_TRIPLE="${RUST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"

echo "packaging $NAME"

rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/share/aura/runtime" "$STAGE/share/aura/std"

[[ -x "$BIN" ]] || die "missing executable $BIN"

cp "$BIN" "$STAGE/bin/aura"
cp "$ROOT/runtime/runtime.c" "$STAGE/share/aura/runtime/runtime.c"
cp "$ROOT/runtime/aura_runtime_abi.h" "$STAGE/share/aura/runtime/aura_runtime_abi.h"
cp "$ROOT/runtime/aura_ffi.h" "$STAGE/share/aura/runtime/aura_ffi.h"
cp "$ROOT/runtime/llvm_exceptions.h" "$STAGE/share/aura/runtime/llvm_exceptions.h"
cp -R "$ROOT/runtime/src" "$STAGE/share/aura/runtime/src"
[[ -s "$ROOT/runtime/runtime.c" ]] || die "runtime source is missing or empty"

RUNTIME_CC="${RUNTIME_CC:-${CC:-cc}}"
RUNTIME_AR="${RUNTIME_AR:-${AR:-ar}}"
RUNTIME_TARGET_FLAGS="${RUNTIME_TARGET_FLAGS:-}"
if [[ -z "${RUNTIME_TARGET_FLAGS}" && -n "${RUST_TARGET:-}" && "$OS" == darwin ]]; then
  # The macOS cross-target release job uses Apple's clang driver.
  RUNTIME_TARGET_FLAGS="-target $RUST_TARGET"
fi

runtime_abi_version="$(sed -n 's/^#define AURA_RT_ABI_VERSION[[:space:]][[:space:]]*\([0-9][0-9]*\).*/\1/p' runtime/aura_runtime_abi.h | head -1)"
runtime_abi_identity="$(sed -n 's/^#define AURA_RT_ABI_ID[[:space:]][[:space:]]*\(".*"\).*/\1/p' runtime/aura_runtime_abi.h | head -1 | sed 's/^"//; s/"$//')"
[[ -n "$runtime_abi_version" && -n "$runtime_abi_identity" ]] || die "runtime ABI metadata is missing"

write_runtime_metadata() {
  local archive="$1" backend="$2" profile="$3" sanitizer="$4" lto="$5" features="$6"
  local metadata="${archive}.meta"
  local digest
  if command -v sha256sum >/dev/null 2>&1; then
    digest="$(sha256sum "$archive" | awk '{print $1}')"
  else
    digest="$(shasum -a 256 "$archive" | awk '{print $1}')"
  fi
  {
    printf 'schema=1\n'
    printf 'target=%s\n' "$TARGET_NAME"
    printf 'target_triple=%s\n' "$TARGET_TRIPLE"
    printf 'backend=%s\n' "$backend"
    printf 'profile=%s\n' "$profile"
    printf 'sanitizer=%s\n' "$sanitizer"
    printf 'lto=%s\n' "$lto"
    printf 'features=%s\n' "$features"
    printf 'runtime_abi_version=%s\n' "$runtime_abi_version"
    printf 'runtime_abi_identity=%s\n' "$runtime_abi_identity"
    printf 'sha256=%s\n' "$digest"
  } >"$metadata"
}

build_runtime_profile() {
  local profile="$1" cflags llvm_cflags plain_cflags sanitizer lto features artifact_root
  case "$profile" in
    dev)
      cflags="-std=c11 -O0 -g -fPIC -I. -fsanitize=address,undefined $RUNTIME_TARGET_FLAGS"
      plain_cflags="-std=c11 -O0 -g -fPIC -I. $RUNTIME_TARGET_FLAGS"
      llvm_cflags="-std=c11 -O0 -g -fPIC -I. $RUNTIME_TARGET_FLAGS -Wno-implicit-function-declaration -DAURA_LLVM_RUNTIME -DAURA_RUNTIME_NO_MAIN"
      sanitizer="address,undefined"
      lto="off"
      features="none"
      ;;
    release)
      cflags="-std=c11 -O2 -fPIC -I. $RUNTIME_TARGET_FLAGS"
      plain_cflags="$cflags"
      llvm_cflags="$cflags -Wno-implicit-function-declaration -DAURA_LLVM_RUNTIME -DAURA_RUNTIME_NO_MAIN"
      sanitizer="none"
      lto="off"
      features="none"
      ;;
    *) die "unsupported runtime profile: $profile" ;;
  esac
  artifact_root="$STAGE/share/aura/runtime/$TARGET_NAME"
  make -C "$ROOT/runtime" \
    CC="$RUNTIME_CC" AR="$RUNTIME_AR" \
    RUNTIME_CFLAGS="$cflags" LLVM_RUNTIME_CFLAGS="$llvm_cflags" \
    RUNTIME_OBJECT="$artifact_root/c/$profile/libaurart.o" \
    RUNTIME_ARCHIVE="$artifact_root/c/$profile/libaurart.a" \
    LLVM_RUNTIME_OBJECT="$artifact_root/llvm/$profile/libaurart-llvm.o" \
    LLVM_RUNTIME_ARCHIVE="$artifact_root/llvm/$profile/libaurart-llvm.a" \
    all llvm
  write_runtime_metadata "$artifact_root/c/$profile/libaurart.a" c "$profile" "$sanitizer" "$lto" "$features"
  write_runtime_metadata "$artifact_root/llvm/$profile/libaurart-llvm.a" llvm "$profile" none "$lto" "$features"

  if [[ "$profile" == dev ]]; then
    # Normal `aura test` is not sanitizer-enabled; keep a matching dev archive
    # instead of linking an instrumented runtime into an uninstrumented app.
    local plain_root="$artifact_root/c/$profile/none"
    make -C "$ROOT/runtime" \
      CC="$RUNTIME_CC" AR="$RUNTIME_AR" \
      RUNTIME_CFLAGS="$plain_cflags" \
      RUNTIME_OBJECT="$plain_root/libaurart.o" \
      RUNTIME_ARCHIVE="$plain_root/libaurart.a" \
      all
    write_runtime_metadata "$plain_root/libaurart.a" c "$profile" none "$lto" "$features"
  fi
}

# Ship the default dev runtime plus a release archive for explicit production
# application builds. Source remains the bootstrap fallback.
build_runtime_profile dev
build_runtime_profile release
# Object files are build intermediates; only archives and their metadata are
# part of the installed toolchain payload.
find "$STAGE/share/aura/runtime/$TARGET_NAME" -type f -name '*.o' -delete
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

# Make the target-neutral runtime/std payload explicit for cross-target build
# consumers. The executable remains target-specific; this manifest describes
# the source sysroot shipped alongside it.
SYSROOT_MANIFEST="$STAGE/share/aura/sysroot-manifest.txt"
{
  printf 'format=1\n'
  printf 'runtime=share/aura/runtime\n'
  printf 'runtime-artifact=%s/c/dev/libaurart.a\n' "$TARGET_NAME"
  printf 'runtime-artifact=%s/c/dev/none/libaurart.a\n' "$TARGET_NAME"
  printf 'runtime-artifact=%s/llvm/dev/libaurart-llvm.a\n' "$TARGET_NAME"
  printf 'runtime-artifact=%s/c/release/libaurart.a\n' "$TARGET_NAME"
  printf 'runtime-artifact=%s/llvm/release/libaurart-llvm.a\n' "$TARGET_NAME"
  for package_dir in "${std_packages[@]}"; do
    [[ -d "$package_dir" ]] || continue
    printf 'std=%s\n' "$(basename "$package_dir")"
  done
} >"$SYSROOT_MANIFEST"
cp "$ROOT/LICENSE" "$STAGE/LICENSE"
cat >"$STAGE/README.txt" <<EOF
Aura toolchain ${TAG_VERSION} (${OS}/${ARCH})

Install:
  export PATH="\$PWD/bin:\$PATH"
  aura version
  aura new hello && aura run hello

Runtime:
  target-specific C/LLVM static archives are included under share/aura/runtime/.
  runtime.c remains available as a source/bootstrap fallback.

Standard library:
  share/aura/std/* — all standard-library packages used by auto-prelude and \`import std.*\`.
  Optional: export AURA_STD="\$PWD/share/aura/std"

Docs: https://aura.pilotworks.dev
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
  "$NAME/share/aura/runtime/llvm_exceptions.h" \
  "$NAME/share/aura/runtime/$TARGET_NAME/c/dev/libaurart.a" \
  "$NAME/share/aura/runtime/$TARGET_NAME/c/dev/libaurart.a.meta" \
  "$NAME/share/aura/runtime/$TARGET_NAME/c/release/libaurart.a" \
  "$NAME/share/aura/runtime/$TARGET_NAME/c/release/libaurart.a.meta" \
  "$NAME/share/aura/runtime/$TARGET_NAME/llvm/dev/libaurart-llvm.a" \
  "$NAME/share/aura/runtime/$TARGET_NAME/llvm/dev/libaurart-llvm.a.meta" \
  "$NAME/share/aura/runtime/$TARGET_NAME/llvm/release/libaurart-llvm.a" \
  "$NAME/share/aura/runtime/$TARGET_NAME/llvm/release/libaurart-llvm.a.meta" \
  "$NAME/share/aura/sysroot-manifest.txt" \
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
SYSROOT_CONTENT="$(tar -xOzf "$TAR" "$NAME/share/aura/sysroot-manifest.txt")"
grep -Fxq 'format=1' <<<"$SYSROOT_CONTENT" || die "archive sysroot manifest has no format"
grep -Fxq 'runtime=share/aura/runtime' <<<"$SYSROOT_CONTENT" \
  || die "archive sysroot manifest has no runtime entry"
for package_dir in "${std_packages[@]}"; do
  [[ -d "$package_dir" ]] || continue
  grep -Fxq "std=$(basename "$package_dir")" <<<"$SYSROOT_CONTENT" \
    || die "archive sysroot manifest is missing std/$(basename "$package_dir")"
done

echo "wrote $TAR"
echo "wrote $CHECKSUM"
