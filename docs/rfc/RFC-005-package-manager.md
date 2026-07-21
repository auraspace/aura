# RFC-005: Package Manager

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 005                       |
| **Title**    | Package Manager           |
| **Status**   | Accepted                  |
| **Layer**    | Toolchain                 |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-07-21                |
| **Estimate** | 20–40 pages               |
| **Depends**  | RFC-000                   |
| **Blocks**   | RFC-008, RFC-012, RFC-013 |

---

## 1. Abstract

This RFC defines the **Aura package manager**: manifest format (`aura.toml`), lockfile, dependency resolver, registry client, workspaces, and publish flow. The **default registry is GitHub-backed** (index repository + Release crate artifacts; optional direct `github =` deps). Implemented in **Rust** as part of the `aura` CLI, it ensures **reproducible** dependency graphs for libraries and binaries.

**Toolchain today (2026-07-21):** multi-file packages with minimal `aura.toml`, **path** dependencies only, and `aura.lock` write/verify including nested/transitive path entries (C3e–C3p, C4j) plus **registry pin schema v0** (C8k: `version` / `source` / `checksum`, no fetch). No registry HTTP client, semver resolve, git/GitHub deps, workspaces, or publish yet — see [roadmap](../roadmap.md) and `agents/debts.md`.

## 2. Motivation

### 2.1 Problem statement

Without a first-party package manager, ecosystems fragment (ad-hoc git submodules, copy-paste). Reproducible builds require lockfiles and a resolver with clear semver rules.

### 2.2 Why now

Build (RFC-008), CLI (RFC-012), and distribution (RFC-013) consume the package graph.

### 2.3 Success metrics

| Metric          | Target                                            |
| --------------- | ------------------------------------------------- |
| Reproducibility | Same lockfile → same graph on two machines        |
| UX              | `aura add`, `aura publish` for common flows       |
| Offline         | Build with warm cache + lockfile without registry |

## 3. Goals

- Manifest + lockfile as source of truth.
- Semver-compatible resolver with clear conflict errors.
- Workspaces for multi-package repos.
- Registry protocol sufficient for public + private registries.
- **GitHub as the default registry backend** (index + crate artifacts + publish) so the ecosystem needs no separate package host for v1.
- Checksums and integrity verification.

## 4. Non-goals

- Universal multi-language package management (npm/pip bridge).
- Centralized app store for binaries (see RFC-013 for toolchain install).
- Solving all supply-chain social problems (policy docs later).

## 5. Prior art & alternatives

| System     | Notes                           | Take                                               |
| ---------- | ------------------------------- | -------------------------------------------------- |
| Cargo      | Excellent model; alt registries | **Primary inspiration** (manifest, lock, features) |
| Go modules | Path = VCS URL; proxy optional  | Direct GitHub fetch patterns                       |
| npm        | Huge graph pain                 | Avoid lax ranges default                           |
| Swift PM   | GitHub tags as versions         | Tag → semver for GitHub sources                    |
| Maven      | Enterprise repos                | Private registry / org index ideas                 |

## 6. Design

### 6.1 Overview

```text
aura.toml  +  registry/git/path deps
       ↓ resolve
aura.lock  (pinned versions + hashes)
       ↓ fetch
cache/     → build graph (RFC-008)
```

### 6.2 Manifest (`aura.toml`)

```toml
[package]
name = "demo"
version = "0.1.0"
edition = "2026"          # language edition when available
authors = ["..."]
license = "MIT"
description = "..."
# Optional: where this package lives when published (default registry publish target)
repository = "https://github.com/org/demo"

[dependencies]
# From the default GitHub-backed registry (index resolve + tarball fetch)
http = "1.2"
serde = { version = "1", features = ["json"] }

# Local path
local_lib = { path = "../local_lib" }

# Direct GitHub package (no index required; pin by tag or rev)
tool = { github = "org/tool", tag = "v1.0.0" }
# Equivalent explicit git form
tool2 = { git = "https://github.com/org/tool", rev = "abc123def" }

# Non-default registry (still often a GitHub index repo)
private = { version = "2", registry = "github:myorg/aura-index" }

[dev-dependencies]
assert = "1"

[[bin]]
name = "demo"
path = "src/main.aura"

[lib]
path = "src/lib.aura"
```

### 6.3 Lockfile

- Pins exact versions, **source IDs**, content hashes (sha256).
- Never hand-edited; **commit lockfiles for all packages** (apps and libraries) for reproducibility.
- Schema v0 (C8k) already accepts:

  ```toml
  # path (legacy string or table)
  local_lib = "../local_lib"
  local_lib = { path = "../local_lib", source = "path" }

  # registry pin (fetch not implemented yet)
  http = { version = "1.2.3", checksum = "sha256:…", source = "registry" }
  ```

- Full lock form (when GitHub registry ships) extends `source` with a **source id** and optional locator fields:

  ```toml
  # Default official index (GitHub-backed)
  http = { version = "1.2.3", checksum = "sha256:…", source = "registry+github:auraspace/crates-index" }

  # Direct GitHub (resolved tag → commit)
  tool = { version = "1.0.0", checksum = "sha256:…", source = "github:org/tool", rev = "abc123def" }

  # Path
  local_lib = { path = "../local_lib", source = "path" }
  ```

- Clients **must** treat `source = "registry"` (bare) as an alias for the configured default registry source id.
- `rev` (git commit SHA) is required in lock for any github/git source; floating `branch` never appears in the lock.

### 6.4 Resolver

- Semver: `^` default for `"1.2"`.
- Unify versions when compatible; error on conflicts with explanation tree.
- Features: additive unification (Cargo-like).
- Overrides: `[patch]` / `[replace]` for forks (advanced).
- GitHub `tag` deps: map `vX.Y.Z` / `X.Y.Z` tags to semver for range intersection when a version is also declared; otherwise pin only the locked rev.

### 6.5 Sources

| Source       | Manifest form                                 | Lock `source` id                       | Use                                                                       |
| ------------ | --------------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------- |
| **Registry** | `"1.2"` or `{ version = "1.2" }`              | `registry+github:<owner>/<index-repo>` | Default path: resolve via **GitHub-hosted index**, download crate tarball |
| **GitHub**   | `{ github = "owner/repo", tag = "v1.0.0" }`   | `github:owner/repo` + `rev`            | Direct package repo without publishing to the index                       |
| **Git**      | `{ git = "https://…", rev/tag/branch = "…" }` | `git+https://…` + `rev`                | Any git host; GitHub URLs normalize to the GitHub source when possible    |
| **Path**     | `{ path = "…" }`                              | `path`                                 | Local / workspace packages                                                |

Priority when a name appears in multiple forms is an error unless `[patch]` replaces it.

### 6.6 Registry (GitHub-backed default)

The **default Aura registry is GitHub-backed**. There is no separate crates.io-style SaaS required for v1. Custom HTTP registries remain allowed later; GitHub is the specified default backend.

#### 6.6.1 Components

| Piece         | Default location / mechanism                                                                 |
| ------------- | -------------------------------------------------------------------------------------------- |
| **Index**     | Git repo `github.com/auraspace/crates-index` (name may track org rename; configurable)       |
| **Metadata**  | Per-package documents in the index (versions, yank flags, **sha256**, download URL template) |
| **Artifacts** | Crate tarballs attached as **GitHub Release** assets on the package’s `repository`           |
| **Auth**      | Unauthenticated HTTPS for public read; `GITHUB_TOKEN` / `gh` auth for private read + publish |
| **Publish**   | `aura publish` → build crate → create git tag + Release asset → update index entry           |

Toolchain install (RFC-013) already uses GitHub Releases; package artifacts reuse the same hosting model, with a **separate index repo** so package discovery is not coupled to the compiler monorepo.

#### 6.6.2 Index layout

Sparse, filesystem-friendly layout (Cargo sparse-inspired, GitHub-raw friendly):

```text
crates-index/
  config.json                 # dl template, api base, min aura version
  packages/
    he/
      ll/
        hello/
          versions.json       # or one line per version (append-only preferred)
  yanks/
    hello.json                # optional side file; may live inside versions.json
```

`config.json` (conceptual):

```json
{
  "dl": "https://github.com/{owner}/{repo}/releases/download/v{version}/{name}-{version}.crate",
  "api": "https://github.com/auraspace/crates-index",
  "github_api": "https://api.github.com"
}
```

Each version record includes at least:

| Field        | Meaning                                         |
| ------------ | ----------------------------------------------- |
| `name`       | Package name (flat namespace)                   |
| `vers`       | Semver string                                   |
| `cksum`      | sha256 of the `.crate` tarball                  |
| `yanked`     | bool                                            |
| `repository` | `owner/repo` used to fill the download template |
| `features`   | optional feature map (when features ship)       |

**Read path:** clone or shallow-fetch the index (git protocol), or HTTP GET of individual sparse paths via `raw.githubusercontent.com` / Contents API. Implementations may cache the index under `~/.aura/registry/index/`.

**Write path (publish):** authenticated push or **pull request** to the index repo (bots may auto-merge signed publish commits). Community packages start via PR; official packages may use a token with direct push.

#### 6.6.3 Fetch protocol (client)

1. Parse `aura.toml` deps → query index for matching versions (semver).
2. Resolve unified graph → write/update `aura.lock` with version + checksum + source id.
3. For each registry pin missing from cache:
   - Expand `dl` template → HTTPS download of `.crate`.
   - Verify **sha256** against lock/index; mismatch → abort (supply-chain).
   - Extract into `~/.aura/registry/src/<name>-<version>/` (or content-addressed cache).
4. Build consumes the cache path as if it were a path dep (RFC-008).

Direct **GitHub** deps skip the index: resolve `tag`/`branch` → commit SHA via GitHub API or git ls-remote; download `https://codeload.github.com/<owner>/<repo>/tar.gz/<rev>` (or git fetch); checksum the archive; pin `rev` in the lock.

#### 6.6.4 Names, yank, private

- **Names:** flat; reserve `std` / `aura` prefixes; reverse-DNS encouraged for public uniqueness (`com.example.foo` OK as a name string).
- **Yank:** set `yanked = true` in the index; new resolves must not select yanked versions; existing locks still fetch if the Release asset remains available.
- **Private:** private package repo + optional private index repo under a GitHub org; token needs `contents:read` (fetch) and publish scopes as documented in the CLI.
- **Alternate registries:** `[registries.myorg]` in user/project config pointing at another `github:owner/index-repo` or, later, a generic HTTP index URL.

#### 6.6.5 `aura publish` (GitHub)

```text
aura publish
  → validate aura.toml (name, version, license, repository)
  → package sources into name-version.crate
  → sha256
  → git tag v{version} on package repository (if clean & matches)
  → upload Release asset via GitHub API
  → append version record to crates-index (PR or push)
```

Idempotency: re-publishing the same version with a different checksum is an error; yank + new version for fixes.

### 6.7 Workspaces

```toml
[workspace]
members = ["crates/*"]
```

- Shared lockfile at root.
- Dependency hoisting within workspace path deps.

### 6.8 Commands (see also RFC-012)

| Command                      | Action                                               |
| ---------------------------- | ---------------------------------------------------- |
| `aura init` / `new`          | Scaffold                                             |
| `aura add <pkg>`             | Edit manifest + resolve (default registry)           |
| `aura add github:owner/repo` | Add direct GitHub dep (tag latest semver tag if any) |
| `aura update`                | Refresh within constraints                           |
| `aura publish`               | Package + GitHub Release + index update              |
| `aura tree`                  | Show graph                                           |
| `aura login`                 | Store GitHub token for publish/private (optional)    |

### 6.9 Examples

```text
# Registry (GitHub index)
aura new hello
cd hello
aura add stdx-json
aura build

# Direct GitHub source
aura add github:acme/aura-metrics --tag v0.3.1

# Publish (requires GITHUB_TOKEN with repo + index rights)
export GITHUB_TOKEN=…
aura publish
```

Manifest snippet after `aura add github:acme/aura-metrics --tag v0.3.1`:

```toml
[dependencies]
aura-metrics = { github = "acme/aura-metrics", tag = "v0.3.1" }
```

### 6.10 Error model / edge cases

| Case                         | Behavior                                             |
| ---------------------------- | ---------------------------------------------------- |
| Conflict                     | Error with candidate paths                           |
| Hash mismatch                | Abort fetch; possible mirror / Release-tamper attack |
| Yanked used by lock          | Warn; allow with flag                                |
| Transitive prerelease        | Only if explicitly allowed                           |
| Missing GitHub Release asset | Clear error with package name, version, expected URL |
| Rate limit (API)             | Retry/backoff; prefer git/raw index paths over API   |
| Private without token        | Error pointing at `aura login` / `GITHUB_TOKEN`      |
| Ambiguous git tag semver     | Require explicit `tag` or `rev`                      |

### 6.11 Compatibility & migration

- Manifest format version field.
- Resolver changes must not churn locks without `update`.
- Bare `source = "registry"` in existing C8k locks remains valid = default GitHub index.
- Future self-hosted/generic HTTP registries use distinct source ids; migration is config + re-resolve, not silent remap of checksums.
- Toolchain GitHub Releases (RFC-013) stay independent of the package index repo.

## 7. Open questions

| #   | Question                              | Options       | Owner   | Status                                                                                         |
| --- | ------------------------------------- | ------------- | ------- | ---------------------------------------------------------------------------------------------- |
| 1   | Default registry hosting              |               | Project | **Resolved** — **GitHub-backed**: index repo + Release assets; no separate package SaaS for v1 |
| 2   | Lockfile for pure libraries required? | always commit | Pkg     | **Resolved**                                                                                   |
| 3   | Namespace policy                      |               | Project | **Resolved** — flat names; reserve `std`/`aura`; reverse-DNS encouraged public                 |
| 4   | Index update mechanism on publish     | PR vs push    | Pkg     | **Resolved** — PR default for community; token push allowed for official/automation            |
| 5   | Index repo final name                 |               | Project | Open — default design name `auraspace/crates-index` until created                              |

## 8. Rationale & trade-offs

Cargo-like design is proven for compiled languages with features and workspaces. Strict hashes beat “works on my machine.” Cost: users learn TOML manifest; acceptable.

**Why GitHub as registry:** Aura already ships toolchains via GitHub Releases (RFC-013). Reusing GitHub for package index + artifacts removes day-one ops (CDN, registry service, storage) and matches how most early libraries already live (public repos + tags). Trade-offs accepted: GitHub rate limits and availability become part of the supply chain; mitigated by lockfile checksums, local caches, and a later mirror protocol. Direct `github = "owner/repo"` covers packages before they join the index, similar to Go/Swift.

## 9. Unresolved / future work

- Mirror protocol / offline vendor bundles of the GitHub index
- Vendor mode (`aura vendor`)
- Binary dependencies / toolchains as packages
- Generic non-GitHub HTTP registry adapter (protocol-compatible index)
- Provenance (attestations on Release assets)

## 10. Security & safety considerations

- Always verify **sha256** of crate tarballs against the lock (and index at resolve time).
- HTTPS by default; prefer immutable Release assets + commit SHAs.
- `aura publish` requires auth; org 2FA / branch protection on the index repo.
- Git/GitHub deps: **commit SHAs in lock**, never floating branches.
- Treat index commits and Release uploads as trusted only after checksum match; document token least privilege (`contents` on package repo + index).
- Do not execute install scripts from packages (no build.rs MVP — RFC-008).

## 11. Implementation plan (optional)

| Phase | Scope                                        | Exit criteria                              | Status                                |
| ----- | -------------------------------------------- | ------------------------------------------ | ------------------------------------- |
| K0    | Path deps + lock                             | Multi-package build                        | **Done** (incl. nested path lock C4j) |
| K0b   | Lock schema v0 (`registry` pins)             | Parse/verify without fetch                 | **Done** (C8k)                        |
| K1    | GitHub index client + tarball fetch + semver | Hello dep from default registry or fixture | Deferred                              |
| K1b   | Direct `github =` / `git =` deps             | Lock pins rev + checksum                   | Deferred                              |
| K2    | `aura publish` (Release + index PR/push)     | Round-trip public package                  | Deferred                              |

## 12. References

- Cargo book (registries, sparse index); Go modules; Swift Package Manager
- GitHub REST: Releases, git refs, repository archives
- RFC-008, RFC-012, RFC-013
- Plans: `docs/plans/2026-07-20-c8b-registry-spike.md`, `docs/plans/2026-07-20-c8k-lock-schema.md`

---

## Changelog

| Date       | Author | Change                                                                                      |
| ---------- | ------ | ------------------------------------------------------------------------------------------- |
| 2026-07-21 |        | **GitHub as default registry**: index repo, Release artifacts, `github =` source, publish   |
| 2026-07-16 |        | Lock registry hosting model + flat namespace with reserved prefixes                         |
| 2026-07-16 |        | Status → **Accepted** — Review: aura.toml + path lockfile locked; registry deferred cleanly |
| 2026-07-16 |        | Note path deps + lock MVP vs registry                                                       |
| 2026-07-15 |        | Initial skeleton                                                                            |
| 2026-07-15 |        | Solid draft: aura.toml, lock, resolver                                                      |
| 2026-07-15 |        | Lock always-commit lockfiles                                                                |
