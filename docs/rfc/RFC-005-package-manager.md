# RFC-005: Package Manager

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 005                       |
| **Title**    | Package Manager           |
| **Status**   | Draft                     |
| **Layer**    | Toolchain                 |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-07-15                |
| **Estimate** | 20–40 pages               |
| **Depends**  | RFC-000                   |
| **Blocks**   | RFC-008, RFC-012, RFC-013 |

---

## 1. Abstract

This RFC defines the **Aura package manager**: manifest format (`aura.toml`), lockfile, dependency resolver, registry client, workspaces, and publish flow. Implemented in **Rust** as part of the `aura` CLI, it ensures **reproducible** dependency graphs for libraries and binaries.

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
- Checksums and integrity verification.

## 4. Non-goals

- Universal multi-language package management (npm/pip bridge).
- Centralized app store for binaries (see RFC-013 for toolchain install).
- Solving all supply-chain social problems (policy docs later).

## 5. Prior art & alternatives

| System     | Notes            | Take                     |
| ---------- | ---------------- | ------------------------ |
| Cargo      | Excellent model  | **Primary inspiration**  |
| Go modules | Minimal, MVS     | Resolver contrast        |
| npm        | Huge graph pain  | Avoid lax ranges default |
| Maven      | Enterprise repos | Private registry ideas   |

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

[dependencies]
http = "1.2"
serde = { version = "1", features = ["json"] }
local_lib = { path = "../local_lib" }
tool = { git = "https://github.com/org/tool", rev = "abc123" }

[dev-dependencies]
assert = "1"

[[bin]]
name = "demo"
path = "src/main.aura"

[lib]
path = "src/lib.aura"
```

### 6.3 Lockfile

- Pins exact versions, source IDs, content hashes.
- Never hand-edited; **commit lockfiles for all packages** (apps and libraries) for reproducibility.

### 6.4 Resolver

- Semver: `^` default for `"1.2"`.
- Unify versions when compatible; error on conflicts with explanation tree.
- Features: additive unification (Cargo-like).
- Overrides: `[patch]` / `[replace]` for forks (advanced).

### 6.5 Sources

| Source   | Use                                    |
| -------- | -------------------------------------- |
| Registry | Default crates-io-like index           |
| Path     | Local path deps                        |
| Git      | rev/tag/branch (rev preferred in lock) |

### 6.6 Registry

- Index: version metadata + download URLs.
- Auth: token for publish/private read.
- Yank: prevent new resolves; existing locks still fetch if cached/available.
- Names: DNS-like / reserved official `std` / `aura` prefixes.

### 6.7 Workspaces

```toml
[workspace]
members = ["crates/*"]
```

- Shared lockfile at root.
- Dependency hoisting within workspace path deps.

### 6.8 Commands (see also RFC-012)

| Command             | Action                     |
| ------------------- | -------------------------- |
| `aura init` / `new` | Scaffold                   |
| `aura add <pkg>`    | Edit manifest + resolve    |
| `aura update`       | Refresh within constraints |
| `aura publish`      | Package + upload           |
| `aura tree`         | Show graph                 |

### 6.9 Examples

```text
aura new hello
cd hello
aura add stdx-json   # hypothetical
aura build
```

### 6.10 Error model / edge cases

| Case                  | Behavior                            |
| --------------------- | ----------------------------------- |
| Conflict              | Error with candidate paths          |
| Hash mismatch         | Abort fetch; possible mirror attack |
| Yanked used by lock   | Warn; allow with flag               |
| Transitive prerelease | Only if explicitly allowed          |

### 6.11 Compatibility & migration

- Manifest format version field.
- Resolver changes must not churn locks without `update`.

## 7. Open questions

| #   | Question                              | Options       | Owner   | Status       |
| --- | ------------------------------------- | ------------- | ------- | ------------ |
| 1   | Default registry hosting              |               | Project | Open         |
| 2   | Lockfile for pure libraries required? | always commit | Pkg     | **Resolved** |
| 3   | Namespace policy                      |               | Project | Open         |

## 8. Rationale & trade-offs

Cargo-like design is proven for compiled languages with features and workspaces. Strict hashes beat “works on my machine.” Cost: users learn TOML manifest; acceptable.

## 9. Unresolved / future work

- Mirror protocol
- Vendor mode
- Binary dependencies / toolchains as packages

## 10. Security & safety considerations

- Always verify checksums.
- HTTPS by default; pin registry keys when possible.
- `aura publish` requires auth; 2FA policy for official registry later.
- Git deps: commit SHAs in lock, not moving branches.

## 11. Implementation plan (optional)

| Phase | Scope            | Exit criteria       |
| ----- | ---------------- | ------------------- |
| K0    | Path deps + lock | Multi-package build |
| K1    | Registry fetch   | Hello from registry |
| K2    | Publish          | Round-trip          |

## 12. References

- Cargo book; Go modules reference
- RFC-008, RFC-012, RFC-013

---

## Changelog

| Date       | Author | Change                                 |
| ---------- | ------ | -------------------------------------- |
| 2026-07-15 |        | Initial skeleton                       |
| 2026-07-15 |        | Solid draft: aura.toml, lock, resolver |
| 2026-07-15 |        | Lock always-commit lockfiles           |
