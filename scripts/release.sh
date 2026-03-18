#!/bin/bash

# scripts/release.sh
# Ported from .github/workflows/release.yml
# Converts the CI release workflow into a local/CI shell script.

set -e

# --- Configuration ---
BINARY_NAME="aura"
STDLIB_DIR="stdlib"
DIST_DIR="dist"
PKG_DIR="pkg"

# Error handling
error() {
    echo "Error: $1" >&2
    exit 1
}

# Print usage
usage() {
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --tag <version>    The version tag to release (e.g., v0.1.0). If not provided, it will attempt to detect the current tag."
    echo "  --dry-run          Don't actually create a GitHub release, just build and package."
    echo "  --help             Show this help message."
    exit 0
}

# Parse arguments
VERSION_TAG=""
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --tag)
            VERSION_TAG="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help)
            usage
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Detect Version if not provided
if [[ -z "$VERSION_TAG" ]]; then
    VERSION_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    if [[ -z "$VERSION_TAG" ]]; then
        VERSION_TAG=$(git rev-parse --short HEAD)
    fi
fi

echo "--- Releasing $BINARY_NAME version $VERSION_TAG ---"

# Detect OS and Architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

ARTIFACT_NAME="$BINARY_NAME-$OS-$ARCH"

if [[ "$OS" == "darwin" ]]; then
    TARGET="aarch64-apple-darwin" # Assuming ARM64 for MacOS as per project-structure.md
elif [[ "$OS" == "linux" ]]; then
    TARGET="x86_64-unknown-linux-gnu"
else
    error "Unsupported OS: $OS"
fi

echo "OS: $OS, ARCH: $ARCH, TARGET: $TARGET"

# --- 1. Build ---
echo "--- Building release binary ---"
cargo build --release

# --- 2. Package ---
echo "--- Packaging artifact ---"
mkdir -p "$DIST_DIR" "$PKG_DIR"
rm -rf "$PKG_DIR"/*

BIN_PATH="target/release/$BINARY_NAME"
if [[ ! -f "$BIN_PATH" ]]; then
    error "Compiled binary not found at $BIN_PATH"
fi

cp "$BIN_PATH" "$PKG_DIR/$BINARY_NAME"
if [[ -d "$STDLIB_DIR" ]]; then
    cp -R "$STDLIB_DIR" "$PKG_DIR/$STDLIB_DIR"
else
    echo "Warning: $STDLIB_DIR not found, skipping."
fi

TARBALL="$DIST_DIR/$ARTIFACT_NAME.tar.gz"
tar -czf "$TARBALL" -C "$PKG_DIR" "$BINARY_NAME" "$STDLIB_DIR"

echo "Artifact created: $TARBALL"

# --- 3. Release (using gh CLI) ---
if [[ "$DRY_RUN" == true ]]; then
    CHANGELOG_CONTENT=$(awk '/^## \[/{if (count++) exit; next} count' CHANGELOG.md)
    echo "Dry run enabled. Skipping GitHub Release creation."
    echo "--- Latest Changelog Entry ---"
    echo "$CHANGELOG_CONTENT"
    echo "------------------------------"
    echo "To release manually, run: gh release create $VERSION_TAG $TARBALL --notes \"\$CHANGELOG_CONTENT\""
else
    if command -v gh &> /dev/null; then
        echo "--- Extracting Latest Changelog ---"
        CHANGELOG_CONTENT=$(awk '/^## \[/{if (count++) exit; next} count' CHANGELOG.md)
        if [[ -z "$CHANGELOG_CONTENT" ]]; then
            echo "Warning: No changelog content found for the latest version. Using default notes."
            CHANGELOG_CONTENT="Release $VERSION_TAG"
        fi

        echo "--- Creating GitHub Release ---"
        gh release create "$VERSION_TAG" "$TARBALL" --title "$VERSION_TAG" --notes "$CHANGELOG_CONTENT"
    else
        echo "Warning: GitHub CLI 'gh' not found. Skipping release step."
        echo "Artifact remains in $DIST_DIR/"
    fi
fi

echo "Done!"
