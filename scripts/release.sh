#!/usr/bin/env bash

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
MAIN_PACKAGE="aurac"
MAIN_PACKAGE_DIR="crates/aurac"

# Function to print colored messages
info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
check_prerequisites() {
    info "Checking prerequisites..."
    
    if ! command_exists git; then
        error "git is not installed"
        exit 1
    fi
    
    if ! command_exists cargo; then
        error "cargo is not installed"
        exit 1
    fi
    
    # Check if git-cliff is installed
    if ! command_exists git-cliff; then
        warn "git-cliff is not installed. Installing..."
        cargo install git-cliff
    fi
}

# Check git status
check_git_status() {
    info "Checking git status..."
    
    if [[ -n $(git status --porcelain) ]]; then
        error "Working directory is not clean. Please commit or stash changes."
        git status --short
        exit 1
    fi
    
    local current_branch=$(git branch --show-current)
    if [[ "$current_branch" != "main" && "$current_branch" != "master" ]]; then
        warn "You are not on main/master branch. Current branch: $current_branch"
        read -p "Continue? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
}

# Get current version from workspace
get_current_version() {
    grep -A 5 '^\[workspace\.package\]' Cargo.toml | grep '^version = ' | sed 's/version = "\(.*\)"/\1/'
}

# Bump version
bump_version() {
    local bump_type=$1
    local current_version=$(get_current_version)
    
    info "Current version: $current_version"
    info "Bump type: $bump_type"
    
    # Manual version bump
    IFS='.' read -r major minor patch <<< "$current_version"
    
    case $bump_type in
        major)
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            ;;
        patch)
            patch=$((patch + 1))
            ;;
        *)
            error "Invalid bump type: $bump_type. Use major, minor, or patch."
            exit 1
            ;;
    esac
    
    new_version="$major.$minor.$patch"
    info "New version: $new_version"
    
    # Update workspace version
    sed -i.bak "/^\[workspace\.package\]/,/^version = / s/^version = \".*\"/version = \"$new_version\"/" Cargo.toml
    rm -f Cargo.toml.bak
    
    # Update Cargo.lock
    cargo update --workspace --quiet
    
    local new_version=$(get_current_version)
    info "Version bumped to: $new_version"
    echo "$new_version"
}

# Generate changelog
generate_changelog() {
    info "Generating changelog..."
    
    # Generate root changelog
    git-cliff --output CHANGELOG.md
    
    if [[ -f CHANGELOG.md ]]; then
        info "Changelog generated: CHANGELOG.md"
    fi
}

# Create git commit
create_commit() {
    local version=$1
    
    info "Creating commit..."
    
    # Add all changed files
    git add -A
    
    # Create commit
    git commit -m "chore: release v$version"
    
    info "Commit created"
}

# Create and push tag
create_and_push_tag() {
    local version=$1
    local tag_name="v$version"
    
    info "Creating tag: $tag_name"
    
    # Create annotated tag
    git tag -a "$tag_name" -m "Release $tag_name"
    
    info "Tag created: $tag_name"
    
    # Push commit and tag automatically
    info "Pushing to remote..."
    git push origin HEAD
    git push origin "$tag_name"
    
    info "Pushed successfully!"
    info "GitHub Actions will now build and publish the release."
    info "View release at: https://github.com/auraspace/aura/releases/tag/$tag_name"
}

# Main release workflow
main() {
    info "Starting release process..."
    
    # Default bump type
    local bump_type="${1:-patch}"
    
    if [[ "$bump_type" != "major" && "$bump_type" != "minor" && "$bump_type" != "patch" ]]; then
        error "Invalid bump type: $bump_type"
        echo "Usage: $0 [major|minor|patch]"
        exit 1
    fi
    
    check_prerequisites
    check_git_status
    
    # Generate changelog first (before version bump)
    generate_changelog
    
    # Bump version
    local new_version=$(bump_version "$bump_type")
    
    # Update changelog with new version if needed
    if [[ -f CHANGELOG.md ]]; then
        # Replace [Unreleased] with [version] - date
        local today=$(date +%Y-%m-%d)
        sed -i.bak "s/## \[Unreleased\]/## [Unreleased]\n\n## [$new_version] - $today/" CHANGELOG.md
        rm -f CHANGELOG.md.bak
    fi
    
    # Create commit
    create_commit "$new_version"
    
    # Create and push tag
    create_and_push_tag "$new_version"
    
    info "Release process completed!"
    info "New version: $new_version"
}

# Run main function
main "$@"
