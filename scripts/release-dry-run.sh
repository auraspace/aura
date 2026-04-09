#!/usr/bin/env bash

# Dry run to preview what a release would do
# Usage: ./scripts/release-dry-run.sh [major|minor|patch]

set -euo pipefail

BUMP_TYPE="${1:-patch}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
preview() { echo -e "${BLUE}[PREVIEW]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

echo "========================================"
echo "          RELEASE DRY RUN"
echo "========================================"
echo ""

# Get current version from workspace
CURRENT_VERSION=$(grep -A 5 '^\[workspace\.package\]' Cargo.toml | grep '^version = ' | sed 's/version = "\(.*\)"/\1/')
info "Current version: $CURRENT_VERSION"
info "Bump type: $BUMP_TYPE"
echo ""

# Calculate new version
IFS='.' read -r major minor patch <<< "$CURRENT_VERSION"
case $BUMP_TYPE in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    *) echo "Invalid bump type: $BUMP_TYPE"; exit 1 ;;
esac

NEW_VERSION="$major.$minor.$patch"
preview "New version would be: $NEW_VERSION"
echo ""

# Check git status
if [[ -n $(git status --porcelain) ]]; then
    warn "Working directory has uncommitted changes:"
    git status --short
    echo ""
fi

# Preview files that would be modified
preview "Files that would be modified:"
echo "  - Cargo.toml (workspace version)"
find crates runtime -name "Cargo.toml" -type f | sort | sed 's/^/  - /'
echo "  - Cargo.lock"
echo "  - CHANGELOG.md (generated/updated)"
echo ""

# Preview git operations
preview "Git operations that would be performed:"
echo "  1. git add -A"
echo "  2. git commit -m \"chore: release v$NEW_VERSION\""
echo "  3. git tag -a \"v$NEW_VERSION\" -m \"Release v$NEW_VERSION\""
echo "  4. git push origin HEAD"
echo "  5. git push origin v$NEW_VERSION"
echo ""

# Preview GitHub Actions trigger
preview "GitHub Actions would be triggered:"
echo "  - Workflow: .github/workflows/release-binaries.yml"
echo "  - Trigger: tag push (v$NEW_VERSION)"
echo "  - Build target: aarch64-apple-darwin"
echo "  - Artifact: aurac-aarch64-apple-darwin.tar.gz"
echo ""

# Show recent commits that would be included in changelog
preview "Recent commits that would be in changelog:"
git log --oneline --no-decorate -10 | sed 's/^/  /'
echo ""

# Check for conventional commits
CONVENTIONAL_COUNT=$(git log --oneline --no-decorate -20 | grep -E '^[a-f0-9]+ (feat|fix|docs|style|refactor|perf|test|chore|build|ci)(\(.+\))?:' | wc -l | tr -d ' ')
TOTAL_COUNT=$(git log --oneline --no-decorate -20 | wc -l | tr -d ' ')
info "Conventional commits: $CONVENTIONAL_COUNT/$TOTAL_COUNT (last 20 commits)"

if [[ $CONVENTIONAL_COUNT -lt 5 ]]; then
    warn "Few conventional commits found. Changelog may be sparse."
    warn "Consider using conventional commit format: type(scope): message"
fi
echo ""

echo "========================================"
echo "To perform actual release, run:"
echo "  ./scripts/release.sh $BUMP_TYPE"
echo "or for non-interactive:"
echo "  ./scripts/quick-release.sh $BUMP_TYPE"
echo "========================================"
