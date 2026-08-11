# RFC-005: Package Manager

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 005                       |
| **Title**    | Package Manager           |
| **Status**   | Accepted                  |
| **Layer**    | Toolchain                 |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-08-06                |
| **Estimate** | 20–40 pages               |
| **Depends**  | RFC-000                   |
| **Blocks**   | RFC-008, RFC-012, RFC-013 |

---

## 1. Abstract

This RFC defines the **Aura package manager**: manifest format (`aura.toml`), lockfile, dependency resolver, origin/module protocol, workspaces, and publication flow. The default public package origin is a Git repository, normally hosted on GitHub. A package is published by pushing an immutable `vX.Y.Z` tag; a proxy is an optional read-through cache layered on direct VCS access. Implemented in **Rust** as part of the `aura` CLI, the package manager ensures **reproducible** dependency graphs for libraries and binaries.

**Toolchain today (2026-08-11, production-readiness follow-up):** multi-file packages, workspace member discovery, path dependencies, and `aura.lock` write/verify including nested/transitive entries are implemented. Direct Git origins support semver tag discovery, tag-to-commit resolution, deterministic source archives, SHA-256 verification, credential-safe fetch, cache extraction, and immutable VCS lock pins. The bounded origin proxy read-through cache implements the versioned `@v` object protocol with HTTPS-only upstreams, atomic cache writes, and size/path limits. `ChecksumDatabase` provides append-only, sequence-checked transparency records with conflict and tamper rejection. Local and live public-origin round-trip evidence exist; root procedural macro executables are pinned by package-relative path and SHA-256 in the lockfile.

## 2. Motivation

### 2.1 Problem statement

Without a first-party package manager, ecosystems fragment (ad-hoc git submodules, copy-paste). Reproducible builds require lockfiles and a resolver with clear semver rules.

### 2.2 Why now

Build (RFC-008), CLI (RFC-012), and distribution (RFC-013) consume the package graph.

### 2.3 Success metrics

| Metric          | Target                                            |
| --------------- | ------------------------------------------------- |
| Reproducibility | Same lockfile → same graph on two machines        |
| UX              | `aura add` for common dependency flows            |
| Offline         | Build with warm cache + lockfile without registry |

## 3. Goals

- Manifest + lockfile as source of truth.
- Semver-compatible resolver with clear conflict errors.
- Workspaces for multi-package repos.
- A versioned module protocol sufficient for public + private origins.
- **Git repositories as the package origin** (repository + `vX.Y.Z` tag) so the ecosystem needs no package host for v1.
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
# From the default VCS origin (tag/revision resolve + source fetch)
http = "1.2"
serde = { version = "1", features = ["json"] }

# Local path
local_lib = { path = "../local_lib" }

# Direct GitHub package (no index required; pin by tag or rev)
tool = { github = "org/tool", tag = "v1.0.0" }
# Equivalent explicit git form
tool2 = { git = "https://github.com/org/tool", rev = "abc123def" }

# Non-default origin/proxy configuration is selected outside the manifest.

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

  # VCS origin pin (locked direct consumption)
  http = { version = "1.2.3", checksum = "sha256:…", source = "git+https://github.com/auraspace/http", rev = "abc123…" }
  ```

- Full lock form uses an explicit origin source id and immutable revision:

  ```toml
  # Default Git origin
  http = { version = "1.2.3", checksum = "sha256:…", source = "git+https://github.com/auraspace/http", rev = "abc123…" }

  # Direct GitHub (resolved tag → commit)
  tool = { version = "1.0.0", checksum = "sha256:…", source = "git+https://github.com/org/tool", rev = "abc123def" }

  # Path
  local_lib = { path = "../local_lib", source = "path" }
  ```

- Existing bare `source = "registry"` locks are legacy and must be migrated to an explicit origin URL and `rev` before public VCS consumption.
- `rev` (git commit SHA) is required in lock for any github/git source; floating `branch` never appears in the lock.

### 6.4 Resolver

- Semver: `^` default for `"1.2"`.
- Unify versions when compatible; error on conflicts with explanation tree.
- Features: additive unification (Cargo-like).
- Overrides: `[patch]` / `[replace]` for forks (advanced).
- GitHub `tag` deps: map `vX.Y.Z` / `X.Y.Z` tags to semver for range intersection when a version is also declared; otherwise pin only the locked rev.

### 6.5 Sources

| Source     | Manifest form                                 | Lock `source` id                            | Use                                                                    |
| ---------- | --------------------------------------------- | ------------------------------------------- | ---------------------------------------------------------------------- |
| **Origin** | `"1.2"` or `{ version = "1.2" }`              | `git+https://…` + `rev`                     | Resolve semver tags from the package repository and fetch source       |
| **GitHub** | `{ github = "owner/repo", tag = "v1.0.0" }`   | `git+https://github.com/owner/repo` + `rev` | Explicit direct origin; same VCS contract                              |
| **Git**    | `{ git = "https://…", rev/tag/branch = "…" }` | `git+https://…` + `rev`                     | Any git host; GitHub URLs normalize to the GitHub source when possible |
| **Path**   | `{ path = "…" }`                              | `path`                                      | Local / workspace packages                                             |

Priority when a name appears in multiple forms is an error unless `[patch]` replaces it.

### 6.6 Package origin and Go module protocol

Aura follows the Go module publication model. The authoritative source is a
version-controlled repository; for public packages this is normally GitHub.
There is no package registry database, package upload endpoint, mandatory
GitHub Release, or index repository in the v1 contract.

#### 6.6.1 Origin contract

| Piece           | Contract                                                                                          |
| --------------- | ------------------------------------------------------------------------------------------------- |
| Module identity | Stable module path, normally the repository path                                                  |
| Source of truth | Public Git repository and its refs                                                                |
| Version         | Immutable semver tag `vX.Y.Z`                                                                     |
| Artifact        | Source tree at the tagged revision; a deterministic archive may be generated by a client or proxy |
| Publication     | Maintainer pushes the tag to the origin repository                                                |
| GitHub Release  | Optional convenience for humans or binary/toolchain distribution; not package discovery           |
| Proxy           | Optional cache/mirror added later; it must not change the origin semantics                        |

The required package files live in the tagged source tree:

```text
<module-repository>/
  aura.toml
  src/
  ...
```

#### 6.6.2 Read protocol

The initial client reads the origin directly, using Git or the hosting
provider's source archive endpoint. A future proxy may expose the equivalent
Go-shaped objects:

```text
<module>/@v/list
<module>/@v/<version>.info
<module>/@v/<version>.mod
<module>/@v/<version>.zip
```

These are proxy/cache representations, not additional publication records. The
origin remains the repository and tag. The client must resolve a version to an
immutable commit, fetch the tagged source, calculate/verify its checksum, and
pin `version`, `rev`, `source`, and `checksum` in `aura.lock`.

#### 6.6.3 Fetch and version selection

1. Parse `aura.toml` dependencies and resolve the module path to its origin.
2. List semver tags from the origin; ignore malformed tags.
3. Select the highest compatible version and resolve it to a commit SHA.
4. Fetch the tagged source tree directly, or through a future proxy.
5. Verify the archive/source checksum and extract it into the Aura cache.
6. Write the immutable version, source URL, revision, and checksum to `aura.lock`.

Private origins use the user's Git credential/SSH setup or an environment token;
credentials never enter manifests or lockfiles. A future checksum database may
provide Go-like transparency and global verification, but it is not required
for the first direct-origin implementation.

#### 6.6.4 Publication

Publishing a package is a Git operation, not an Aura registry API:

```text
validate aura.toml
  → commit package sources
  → create immutable tag v{version}
  → git push origin v{version}
```

Equivalent commands are:

```bash
git tag v1.0.0
git push origin v1.0.0
```

`gh release create` is reserved for Aura toolchains and binary distributions
(RFC-013). It is optional for packages and must not be used as the package
source of truth. Aura may later provide a helper that validates and pushes a
tag, but the protocol must remain usable with ordinary Git commands.

Idempotency: an existing tag is immutable; publishing the same version with a
different commit is an error. Fixes require a new version. Yank/revoke policy
is future metadata/proxy work; the direct client never silently rewrites an
existing lock.

### 6.7 Workspaces

```toml
[workspace]
members = ["crates/*"]
```

- Shared lockfile at root.
- Dependency hoisting within workspace path deps.

### 6.8 Commands (see also RFC-012)

| Command                       | Action                                       |
| ----------------------------- | -------------------------------------------- |
| `aura init` / `new`           | Scaffold                                     |
| `aura add <origin>[@version]` | Add a direct VCS origin and refresh the lock |
| `aura update`                 | Refresh within constraints                   |
| `aura tree`                   | Show graph                                   |
| `aura login`                  | Configure Git/GitHub credentials (optional)  |

### 6.9 Examples

```text
# Registry (GitHub index)
aura new hello
cd hello
aura add owner/stdx-json@1.2
aura build

# Direct GitHub source
aura add acme/aura-metrics@0.3.1

# Publish from the package origin repository
git tag v1.0.0
git push origin v1.0.0
```

Manifest snippet after `aura add acme/aura-metrics@0.3.1`:

```toml
[dependencies]
aura-metrics = { github = "acme/aura-metrics", tag = "v0.3.1" }
```

### 6.10 Error model / edge cases

| Case                     | Behavior                                             |
| ------------------------ | ---------------------------------------------------- |
| Conflict                 | Error with candidate paths                           |
| Hash mismatch            | Abort fetch; possible mirror / Release-tamper attack |
| Yanked used by lock      | Warn; allow with flag                                |
| Transitive prerelease    | Only if explicitly allowed                           |
| Missing tagged source    | Clear error with module path, version, and origin    |
| Rate limit (VCS/API)     | Retry/backoff; use a future proxy when available     |
| Private without token    | Error pointing at `aura login` / `GITHUB_TOKEN`      |
| Ambiguous git tag semver | Require explicit `tag` or `rev`                      |

### 6.11 Compatibility & migration

- Manifest format version field.
- Resolver changes must not churn locks without `update`.
- Bare `source = "registry"` in existing C8k locks is legacy and requires migration to an explicit VCS source and `rev`.
- Future proxies/checksum services must preserve the origin URL, tag, commit, and checksum; migration must not silently remap locks.
- Toolchain GitHub Releases (RFC-013) stay independent of package source publication.

## 7. Open questions

| #   | Question                              | Options       | Owner   | Status                                                                              |
| --- | ------------------------------------- | ------------- | ------- | ----------------------------------------------------------------------------------- |
| 1   | Default package hosting               |               | Project | **Resolved** — direct Git origin with immutable semver tags; no package SaaS for v1 |
| 2   | Lockfile for pure libraries required? | always commit | Pkg     | **Resolved**                                                                        |
| 3   | Namespace policy                      |               | Project | **Resolved** — flat names; reserve `std`/`aura`; reverse-DNS encouraged public      |
| 4   | Package publication mechanism         | Git tag push  | Pkg     | **Resolved** — ordinary Git tag/push; no package index update                       |
| 5   | Proxy/checksum service                | later         | Project | **Deferred** — direct origin first; proxy and checksum database follow later        |

## 8. Rationale & trade-offs

Cargo-like design is proven for compiled languages with features and workspaces. Strict hashes beat “works on my machine.” Cost: users learn TOML manifest; acceptable.

**Why direct Git origins:** Go modules establish that a public repository and
immutable semver tags are enough to publish a package. This removes a package
database, upload service, and index maintenance from Aura's first release.
Trade-offs accepted: clients initially depend on VCS hosting availability and
tag discovery; local caches, immutable lock revisions, and a future proxy or
checksum database address those concerns without changing the origin contract.

## 9. Unresolved / future work

- Proxy/mirror protocol and offline vendor bundles
- Vendor mode (`aura vendor`)
- Binary dependencies / toolchains as packages
- Generic non-Git HTTP origin adapter
- Provenance and checksum database

## 10. Security & safety considerations

- Always verify **sha256** of source archives against the lock and resolved commit.
- HTTPS/SSH by default; always pin immutable commit SHAs.
- Publication requires repository write auth; protect the default branch and tags.
- Git/GitHub deps: **commit SHAs in lock**, never floating branches.
- Treat origin tags and source archives as trusted only after checksum and commit identity match; document token least privilege on the package repository.
- Do not execute install scripts from packages (no build.rs MVP — RFC-008).

## 11. Implementation plan (optional)

| Phase | Scope                             | Exit criteria                            | Status                                                                         |
| ----- | --------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------ |
| K0    | Path deps + lock                  | Multi-package build                      | **Done** (incl. nested path lock C4j)                                          |
| K0b   | Lock schema v0 (`registry` pins)  | Parse/verify without fetch               | **Done** (C8k)                                                                 |
| K1    | Direct origin fetch + semver tags | Locked origin consumption and fixture    | **Done** — direct Git read/verify/cache path and offline round-trip            |
| K1b   | Direct `github =` / `git =` deps  | Lock pins rev + checksum                 | **Done** — tag/rev selectors, commit pins, checksum verification               |
| K2    | Origin publication (tag push)     | Round-trip public package                | **Resolved** — ordinary Git tag/push; live public-host rehearsal is acceptance |
| K3    | Optional proxy/cache              | Same read objects served through a cache | **Prepared boundary** — serving remains deferred after origin stabilization    |

## 12. References

- Cargo book (registries, sparse index); Go modules; Swift Package Manager
- GitHub REST: Releases, git refs, repository archives
- RFC-008, RFC-012, RFC-013
- Plans: `docs/plans/2026-07-20-c8b-registry-spike.md`, `docs/plans/2026-07-20-c8k-lock-schema.md`

---

## Changelog

| Date       | Author | Change                                                                                      |
| ---------- | ------ | ------------------------------------------------------------------------------------------- |
| 2026-08-06 |        | **Go-style origin**: repository + immutable semver tag; proxy/checksum DB deferred          |
| 2026-07-16 |        | Lock registry hosting model + flat namespace with reserved prefixes                         |
| 2026-07-16 |        | Status → **Accepted** — Review: aura.toml + path lockfile locked; registry deferred cleanly |
| 2026-07-16 |        | Note path deps + lock MVP vs registry                                                       |
| 2026-07-15 |        | Initial skeleton                                                                            |
| 2026-07-15 |        | Solid draft: aura.toml, lock, resolver                                                      |
| 2026-07-15 |        | Lock always-commit lockfiles                                                                |
