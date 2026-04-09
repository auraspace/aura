#!/usr/bin/env bash

# Quick release script without interactive prompts
# Usage: ./scripts/quick-release.sh [major|minor|patch]

set -euo pipefail

BUMP_TYPE="${1:-patch}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Check git status
[[ -z $(git status --porcelain) ]] || error "Working directory not clean"

# Get current version from workspace
CURRENT_VERSION=$(grep -A 5 '^\[workspace\.package\]' Cargo.toml | grep '^version = ' | sed 's/version = "\(.*\)"/\1/')
info "Current version: $CURRENT_VERSION"

# Calculate new version
IFS='.' read -r major minor patch <<< "$CURRENT_VERSION"
case $BUMP_TYPE in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    *) error "Invalid bump type: $BUMP_TYPE" ;;
esac

NEW_VERSION="$major.$minor.$patch"
info "New version: $NEW_VERSION"

# Update version in workspace Cargo.toml
info "Updating workspace version..."
sed -i.bak "/^\[workspace\.package\]/,/^version = / s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
find . -name "*.bak" -delete

# Update Cargo.lock
cargo update -p aurac --quiet

# Generate changelog
info "Generating changelog..."
if command -v git-cliff &>/dev/null; then
    git-cliff --output CHANGELOG.md --tag "v$NEW_VERSION"
else
    info "git-cliff not found, skipping changelog generation"
fi

# Commit and tag
info "Creating commit and tag..."
git add -A
git commit -m "chore: release v$NEW_VERSION" --quiet
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"

info "Successfully created release v$NEW_VERSION"

# Push automatically
info "Pushing to remote..."
git push origin HEAD
git push origin "v$NEW_VERSION"

info "Pushed successfully!"
info "GitHub Actions will now build and publish the release."
info "View release at: https://github.com/auraspace/aura/releases/tag/v$NEW_VERSION"
