# Release process

How Aura cuts a public toolchain release (alpha → stable uses the same path).

## Flow

```text
┌─────────────────────┐
│  develop on main    │
│  (features, docs)   │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ prepare-release.sh  │  bump Cargo.toml + refresh Cargo.lock
│  <version>          │  prepend CHANGELOG
│                     │  stub docs/releases/<ver>.md
│                     │  git commit: release: <ver>
└──────────┬──────────┘
           │  review / edit notes
           ▼
┌─────────────────────┐
│ git push origin HEAD│  release commit on main
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ git tag v<version>  │  e.g. v0.1.0-alpha
│ git push origin tag │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ GitHub Actions      │  .github/workflows/release.yml
│  on push tags v*    │
│                     │  matrix: linux-amd64, darwin-arm64, darwin-amd64
│                     │  scripts/package-release.sh → tarball + sha256
│                     │  gh release create + attach assets
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ users install       │
│  curl …/install.sh  │  → $AURA_HOME/versions/<ver>/
│  aura-switch <ver>  │
└─────────────────────┘
```

## One-shot (maintainer)

```bash
# 1) Working tree should only have intentional changes (or use --force).
scripts/prepare-release.sh 0.1.0-alpha --message "First dogfood freeze"

# 2) Edit freeze notes if needed
$EDITOR docs/releases/0.1.0-alpha.md CHANGELOG.md

# 3) If you edited after the script commit:
git add docs/releases/0.1.0-alpha.md CHANGELOG.md
git commit --amend --no-edit   # only if not pushed yet

# 4) Publish commit + tag
git push origin HEAD
git tag v0.1.0-alpha
git push origin v0.1.0-alpha

# 5) Wait for Actions → GitHub Release assets, then:
curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=0.1.0-alpha bash
aura version
```

Dry-run without touching the tree:

```bash
scripts/prepare-release.sh 0.1.0-alpha --dry-run
```

Files only (no commit):

```bash
scripts/prepare-release.sh 0.2.0 --no-commit
```

## Scripts

| Script                       | Role                                              |
| ---------------------------- | ------------------------------------------------- |
| `scripts/prepare-release.sh` | Version dump + changelog + **release commit**     |
| `scripts/package-release.sh` | Build local / CI tarball (`dist/aura-…tar.gz`)    |
| `scripts/install.sh`         | End-user installer (site copies to `/install.sh`) |

## Version naming

| Concept             | Example                                | Where                                              |
| ------------------- | -------------------------------------- | -------------------------------------------------- |
| Release version     | `0.1.0-alpha`                          | CHANGELOG, notes, install `AURA_VERSION`           |
| Git tag             | `v0.1.0-alpha`                         | Triggers CI; GitHub Release name                   |
| Cargo workspace ver | `0.1.0-alpha`                          | `Cargo.toml` `[workspace.package]`; `aura version` |
| Artifact name       | `aura-0.1.0-alpha-darwin-arm64.tar.gz` | GH Release assets                                  |

Prerelease tags (`*alpha*`, `*beta*`, `*rc*`) create a **prerelease** on GitHub.

## Notes files

Per-release freeze / ship notes live here:

```text
docs/releases/<version>.md
```

`prepare-release.sh` creates a stub if missing; keep the human-written scope table (in/out of scope) up to date before tagging.

## Local package only (no publish)

```bash
TAG_VERSION=0.1.0-alpha bash scripts/package-release.sh
# → dist/aura-0.1.0-alpha-<os>-<arch>.tar.gz
```
