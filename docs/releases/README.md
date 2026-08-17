# Release process

How Aura cuts a public toolchain release (alpha → stable uses the same path).

Current release: `0.1.1-alpha.8` is prepared for publication with GitHub Release assets. The
older `0.1.1-alpha.7` and `0.1.0-alpha` releases remain available for compatibility and history;
changes after `v0.1.1-alpha.8` belong to a subsequent release or maintenance
update.

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
│ git tag v<version>  │  e.g. v0.2.0-alpha
│ git push origin tag │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ GitHub Actions      │  .github/workflows/release.yml
│  on push tags v*    │
│                     │  matrix: linux-amd64, darwin-arm64,
│                     │          darwin-amd64 (cross from macos-14)
│                     │  scripts/package-release.sh → tarball + sha256
│                     │  gh release create + attach assets
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ users install       │
│  curl …/install.sh  │  → $AURA_HOME/versions/<ver>/
│  avm <ver>          │
└─────────────────────┘
```

## One-shot (maintainer)

```bash
# 1) Working tree should only have intentional changes (or use --force).
scripts/prepare-release.sh 0.1.1-alpha.8 --message "Release 0.1.1-alpha.8"

# 2) Edit freeze notes if needed
$EDITOR docs/releases/0.1.1-alpha.8.md CHANGELOG.md

# 3) If you edited after the script commit:
git add docs/releases/0.1.1-alpha.8.md CHANGELOG.md
git commit --amend --no-edit   # only if not pushed yet

# 4) Publish the release commit + tag
git push origin HEAD
git tag v0.1.1-alpha.8
git push origin v0.1.1-alpha.8

# 5) Wait for Actions → GitHub Release assets, then:
curl -fsSL https://aura.pilotworks.dev/install.sh | AURA_VERSION=0.1.1-alpha.8 bash
aura version
```

Dry-run without touching the tree:

```bash
scripts/prepare-release.sh 0.2.0-alpha --dry-run
```

Files only (no commit):

```bash
scripts/prepare-release.sh 0.2.0 --no-commit
```

## Scripts

| Script                                 | Role                                              |
| -------------------------------------- | ------------------------------------------------- |
| `scripts/prepare-release.sh`           | Version dump + changelog + **release commit**     |
| `scripts/package-release.sh`           | Build local / CI tarball (`dist/aura-…tar.gz`)    |
| `scripts/install.sh`                   | End-user installer (site copies to `/install.sh`) |
| `scripts/avm`                          | Version manager (embedded into CDN install.sh)    |
| `scripts/install-smoke.sh`             | Post-install / post-release verify checklist      |
| `scripts/generate-release-manifest.sh` | Emit target/checksum/acceptance manifest          |
| `scripts/validate-release-bundle.sh`   | Fail-closed artifact/signature verification       |

## Support contract

The CI and release workflows use the same required artifact matrix:

The source of truth is [`scripts/release-targets.tsv`](../../scripts/release-targets.tsv), checked by [`scripts/validate-release-policy.sh`](../../scripts/validate-release-policy.sh). The cross-target validator compares both the tag release matrix and the PR CI `platform-contract` matrix against that manifest, and fails before packaging if either workflow drifts from package/installer support or if production signing is incomplete.

| Target       | Artifact suffix | Build mode                   |
| ------------ | --------------- | ---------------------------- |
| Linux x86_64 | `linux-amd64`   | native on `ubuntu-latest`    |
| Linux arm64  | `linux-arm64`   | native on `ubuntu-24.04-arm` |
| macOS arm64  | `darwin-arm64`  | native on `macos-14`         |
| macOS x86_64 | `darwin-amd64`  | cross-built on `macos-14`    |

Windows amd64/arm64 remain explicit tier2 policy targets. They are not required
CI jobs, release artifacts, or installer targets until native acceptance
evidence exists. The non-claim is verified by JSON fixtures in
`scripts/fixtures/target-policy/`. Unsupported targets should use the
[source-install path](../guide/install.md#install-from-source-alpha) when
their Rust and C toolchains are available.

In machine-readable target names, the remaining policy-only targets are
`windows-amd64` and `windows-arm64`.

Production release tags require the GitHub `production` environment, with
`AURA_MINISIGN_SECRET_KEY` configured as an environment secret and
`AURA_MINISIGN_PUBLIC_KEY` as its matching environment variable. The workflow
signs and verifies `SHA256SUMS` and publishes `SHA256SUMS.minisig` plus
`minisign.pub`. Missing or invalid signing material is a release failure, not
an unsigned success.

## Evidence boundary for v0.1.1-alpha.5

The checked-in policy and fixture gates prove matrix alignment, package layout,
checksums, signed-bundle wiring, and the distinction between native execution
and cross-file inspection. Release acceptance reports use schema 2: `native`
reports must record `execution: "ran"` on the matching OS/architecture, while
`cross-file` reports explicitly record `execution: "not-run"`.

They do not substitute for external evidence. Before a target is promoted from
policy-only or a production release is declared complete, retain links or CI
run IDs for the following:

- Windows amd64 and Windows arm64: native package, installer,
  `aura version`, and `aura new && aura run` results on the declared runner.
- Registry: direct Git-origin consumption with immutable lock pins; proxy serving
  and checksum-database/index services are not part of this release contract
  verification, install/update checksum verification, and a rollback against
  the live endpoint using non-test credentials.
- Release signing: a tag-triggered GitHub Actions run with the configured
  minisign secret/public key, successful detached-signature verification, and
  the resulting GitHub Release assets.

## Version naming

| Concept             | Example                                  | Where                                              |
| ------------------- | ---------------------------------------- | -------------------------------------------------- |
| Release version     | `0.1.1-alpha.8`                          | CHANGELOG, notes, install `AURA_VERSION`           |
| Git tag             | `v0.1.1-alpha.8`                         | Triggers CI; GitHub Release name                   |
| Cargo workspace ver | `0.1.1-alpha.8`                          | `Cargo.toml` `[workspace.package]`; `aura version` |
| Artifact name       | `aura-0.1.1-alpha.8-darwin-arm64.tar.gz` | GH Release assets                                  |

Prerelease tags (`*alpha*`, `*beta*`, `*rc*`) create a **prerelease** on GitHub.

## Notes files

Per-release freeze / ship notes live here:

```text
docs/releases/<version>.md
```

`prepare-release.sh` creates a stub if missing; keep the human-written scope table (in/out of scope) up to date before tagging.

## Local package only (no publish)

```bash
TAG_VERSION=0.1.1-alpha.8 bash scripts/package-release.sh
# → dist/aura-0.1.1-alpha.8-<os>-<arch>.tar.gz
```
