#!/usr/bin/env bash

set -euo pipefail

LEVEL="${1:-patch}" # patch | minor | major | <explicit-version>

if ! command -v git-cliff >/dev/null 2>&1; then
  echo "git-cliff is not installed. See https://git-cliff.org/." >&2
  exit 1
fi

if ! cargo release -V >/dev/null 2>&1; then
  echo "cargo-release is not available. Install with: cargo install cargo-release" >&2
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is not clean. Commit or stash your changes before releasing." >&2
  exit 1
fi

echo "Generating changelog with git-cliff..."
git cliff --output CHANGELOG.md

if ! git diff --quiet -- CHANGELOG.md; then
  echo "Committing changelog..."
  git add CHANGELOG.md
  git commit -m "chore: update changelog"
else
  echo "No changes in CHANGELOG.md, skipping commit."
fi

echo "Running cargo release (${LEVEL})..."
cargo release "${LEVEL}"

echo "Release complete."

